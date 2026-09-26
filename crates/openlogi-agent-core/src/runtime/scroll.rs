//! Traditional wheel output owned by one dedicated worker.
//!
//! Hook callbacks submit typed wheel impulses through [`ScrollInputHandle`]
//! without blocking. The worker either scales and emits them directly or
//! glides toward the accumulated target: every frame covers a fixed share of
//! the remaining distance, so bursts merge into one motion that eases out
//! instead of stopping on a fixed timer. Pixel-precise input
//! never enters this runtime, so native trackpad and continuous wheel streams
//! cannot be mixed with wheel ticks.

mod worker;

pub use worker::{ScrollInputHandle, ScrollPreferences, ScrollRuntime};

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use openlogi_core::scroll::ScrollDelta;
use openlogi_inject::SmoothScrollPhase;

use crate::runtime::HidppSessionId;

/// Output cadence. Progress is computed from the real time since the last
/// frame, so delayed wakes do not slow or lengthen the glide.
const FRAME_PERIOD: Duration = Duration::from_millis(8);
/// Remaining distance (wheel ticks) below which a glide snaps to its target.
const SETTLE_TICKS: f64 = 0.02;
/// Notches closer together than this count as a fast spin for acceleration.
const ACCELERATION_WINDOW: Duration = Duration::from_millis(150);
/// Extra distance multiplier at full acceleration for the fastest spin.
const MAX_ACCELERATION_GAIN: f64 = 4.0;

/// The user's smooth-scroll feel, resolved for the motion model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollFeel {
    /// Exponential time constant of the glide, in seconds.
    time_constant: f64,
    /// Acceleration strength, `0.0..=1.0`.
    acceleration: f64,
    /// Longest fingers-down part of a gesture before the glide coasts. A
    /// free spin keeps notches coming, so it coasts after this even while
    /// the wheel still turns; apps then bound the edge overscroll.
    touch: Duration,
    /// Wheel pause after which the next notch starts a new gesture.
    pause: Duration,
    /// Whether frames carry trackpad phases (edge bounce) at all.
    bounce: bool,
}

impl ScrollFeel {
    /// Resolve the configured smooth-scroll feel.
    pub(crate) fn new(settings: &openlogi_core::config::AppSettings) -> Self {
        // A notch is ~95 % settled after three time constants.
        Self {
            time_constant: settings.smooth_scroll_glide.duration().as_secs_f64() / 3.0,
            acceleration: settings.smooth_scroll_acceleration.fraction(),
            touch: settings.smooth_scroll_touch.duration(),
            pause: settings.smooth_scroll_pause.duration(),
            bounce: settings.smooth_scroll_edge_bounce,
        }
    }

    /// Distance multiplier for a notch arriving `interval` after the last.
    fn gain(self, interval: Duration) -> f64 {
        let speed = 1.0 - interval.as_secs_f64() / ACCELERATION_WINDOW.as_secs_f64();
        1.0 + MAX_ACCELERATION_GAIN * self.acceleration * speed.max(0.0)
    }

    /// Share of the remaining distance covered in `elapsed`.
    fn progress(self, elapsed: Duration) -> f64 {
        1.0 - (-elapsed.as_secs_f64() / self.time_constant).exp()
    }
}

impl Default for ScrollFeel {
    fn default() -> Self {
        Self::new(&openlogi_core::config::AppSettings::default())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WheelDelta {
    x: f64,
    y: f64,
}

impl WheelDelta {
    const ZERO: Self = Self { x: 0.0, y: 0.0 };

    fn is_zero(self) -> bool {
        self.x == 0.0 && self.y == 0.0
    }

    fn plus(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    fn minus(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }

    fn scale(self, factor: f64) -> Self {
        Self {
            x: self.x * factor,
            y: self.y * factor,
        }
    }

    fn magnitude(self) -> f64 {
        self.x.abs().max(self.y.abs())
    }

    /// Drop the remaining motion on any axis the impulse reverses, so a
    /// change of direction answers at once instead of first finishing the
    /// old glide.
    fn reversed_by(self, impulse: Self) -> Self {
        let keep = |remaining: f64, next: f64| {
            if remaining * next < 0.0 {
                0.0
            } else {
                remaining
            }
        };
        Self {
            x: keep(self.x, impulse.x),
            y: keep(self.y, impulse.y),
        }
    }

    fn with_vertical_scale(self, factor: f64) -> Option<Self> {
        let y = self.y * factor;
        y.is_finite().then_some(Self { x: self.x, y })
    }

    fn post(self) {
        openlogi_inject::post_scroll(self.into());
    }
}

impl TryFrom<ScrollDelta> for WheelDelta {
    type Error = ();

    fn try_from(delta: ScrollDelta) -> Result<Self, Self::Error> {
        let ScrollDelta::WheelTicks { x, y } = delta else {
            return Err(());
        };
        let delta = Self { x, y };
        if x.is_finite() && y.is_finite() && !delta.is_zero() {
            Ok(delta)
        } else {
            Err(())
        }
    }
}

impl From<WheelDelta> for ScrollDelta {
    fn from(delta: WheelDelta) -> Self {
        Self::wheel_ticks(delta.x, delta.y)
    }
}

/// One output frame from the pure motion model.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollFrame {
    delta: WheelDelta,
    phase: SmoothScrollPhase,
}

impl ScrollFrame {
    fn new(delta: WheelDelta, phase: SmoothScrollPhase) -> Self {
        Self { delta, phase }
    }

    fn post(self) {
        openlogi_inject::post_smooth_scroll(self.delta.into(), self.phase);
    }
}

/// One physical producer. Linux runs one hook callback thread per grabbed
/// mouse; macOS and Windows use one global callback thread. HID++ capture
/// sessions use their epoch-bearing identity so a restarted session cannot
/// inherit motion from the one it replaced.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ScrollSource {
    OsHook(ThreadId),
    Hidpp(HidppSessionId),
}

impl ScrollSource {
    fn current_hook() -> Self {
        Self::OsHook(thread::current().id())
    }
}

/// One source's glide: the distance still to cover and when it was last
/// advanced.
struct ActiveMotion {
    remaining: WheelDelta,
    last_frame: Instant,
    last_impulse: Instant,
    next_frame: Instant,
}

impl ActiveMotion {
    fn new(impulse: WheelDelta, at: Instant) -> Self {
        Self {
            remaining: impulse,
            last_frame: at,
            last_impulse: at,
            next_frame: at + FRAME_PERIOD,
        }
    }

    /// Advance to `at`, then add the (accelerated) impulse to the target.
    fn retarget(&mut self, impulse: WheelDelta, at: Instant, feel: ScrollFeel) -> MotionUpdate {
        let covered = self.step_to(at, feel);
        let gain = feel.gain(at.saturating_duration_since(self.last_impulse));
        self.last_impulse = at;
        self.remaining = self
            .remaining
            .reversed_by(impulse)
            .plus(impulse.scale(gain));
        if self.remaining.magnitude() < SETTLE_TICKS {
            return MotionUpdate::Finished(covered.plus(self.take_remaining()));
        }
        self.next_frame = at + FRAME_PERIOD;
        MotionUpdate::Active(covered)
    }

    /// Advance to `at` and report whether the glide has settled.
    fn advance(&mut self, at: Instant, feel: ScrollFeel) -> MotionUpdate {
        let covered = self.step_to(at, feel);
        if self.remaining.magnitude() < SETTLE_TICKS {
            return MotionUpdate::Finished(covered.plus(self.take_remaining()));
        }
        while self.next_frame <= at {
            self.next_frame += FRAME_PERIOD;
        }
        MotionUpdate::Active(covered)
    }

    /// Cover the share of the remaining distance due since the last frame.
    fn step_to(&mut self, at: Instant, feel: ScrollFeel) -> WheelDelta {
        let elapsed = at.saturating_duration_since(self.last_frame);
        self.last_frame = self.last_frame.max(at);
        let step = self.remaining.scale(feel.progress(elapsed));
        self.remaining = self.remaining.minus(step);
        step
    }

    fn take_remaining(&mut self) -> WheelDelta {
        std::mem::replace(&mut self.remaining, WheelDelta::ZERO)
    }
}

/// Result of evaluating one source-local motion.
#[derive(Clone, Copy)]
enum MotionUpdate {
    Active(WheelDelta),
    Finished(WheelDelta),
}

impl MotionUpdate {
    fn is_finished(&self) -> bool {
        matches!(self, Self::Finished(_))
    }
}

/// The one phase stream visible to the foreground application. Source-local
/// motions may overlap, but Core Graphics has no source identity with which to
/// pair multiple synthetic gestures; all distances therefore share this single
/// balanced lifecycle.
///
/// A gesture starts "fingers down"; after [`ScrollFeel::touch`] it ends and
/// the rest of the glide coasts as momentum, as after a trackpad lift-off. Applications
/// rubber-band a fingers-down gesture past the content edge for as long as it
/// lasts, but bounce a momentum coast back briefly.
#[derive(Default)]
enum OutputStream {
    #[default]
    Idle,
    Touching {
        since: Instant,
    },
    Coasting,
}

impl OutputStream {
    /// A notch after a wheel pause arrived: a coast in progress stops, so the
    /// notch starts a fresh gesture. Notches of a continuing spin instead
    /// extend the coast.
    fn touch(&mut self, emit: &mut impl FnMut(ScrollFrame)) {
        if matches!(self, Self::Coasting) {
            emit(ScrollFrame::new(
                WheelDelta::ZERO,
                SmoothScrollPhase::MomentumEnded,
            ));
            *self = Self::Idle;
        }
    }

    fn progress(
        &mut self,
        delta: WheelDelta,
        touch: Duration,
        at: Instant,
        emit: &mut impl FnMut(ScrollFrame),
    ) {
        if delta.is_zero() {
            return;
        }
        let phase = match self {
            Self::Idle => {
                *self = Self::Touching { since: at };
                SmoothScrollPhase::Began
            }
            Self::Touching { since } if at.saturating_duration_since(*since) >= touch => {
                emit(ScrollFrame::new(WheelDelta::ZERO, SmoothScrollPhase::Ended));
                *self = Self::Coasting;
                SmoothScrollPhase::MomentumBegan
            }
            Self::Touching { .. } => SmoothScrollPhase::Changed,
            Self::Coasting => SmoothScrollPhase::MomentumChanged,
        };
        emit(ScrollFrame::new(delta, phase));
    }

    fn finish(&mut self, delta: WheelDelta, emit: &mut impl FnMut(ScrollFrame)) {
        match self {
            Self::Idle if !delta.is_zero() => {
                emit(ScrollFrame::new(delta, SmoothScrollPhase::Began));
                emit(ScrollFrame::new(WheelDelta::ZERO, SmoothScrollPhase::Ended));
            }
            Self::Touching { .. } => emit(ScrollFrame::new(delta, SmoothScrollPhase::Ended)),
            Self::Coasting => emit(ScrollFrame::new(delta, SmoothScrollPhase::MomentumEnded)),
            Self::Idle => {}
        }
        *self = Self::Idle;
    }

    fn cancel(&mut self, emit: &mut impl FnMut(ScrollFrame)) {
        match self {
            Self::Touching { .. } => emit(ScrollFrame::new(
                WheelDelta::ZERO,
                SmoothScrollPhase::Cancelled,
            )),
            Self::Coasting => emit(ScrollFrame::new(
                WheelDelta::ZERO,
                SmoothScrollPhase::MomentumEnded,
            )),
            Self::Idle => {}
        }
        *self = Self::Idle;
    }
}

/// Pure per-source state machine. Absence from the map represents idle, so an
/// idle source cannot accidentally retain a target or scheduled deadline. All
/// source-local distances feed one application-visible [`OutputStream`].
#[derive(Default)]
struct ScrollEngine {
    active: HashMap<ScrollSource, ActiveMotion>,
    output: OutputStream,
    feel: ScrollFeel,
    /// When the latest notch from any source arrived.
    last_impulse: Option<Instant>,
}

impl ScrollEngine {
    /// Use `feel` for every following impulse and frame.
    fn set_feel(&mut self, feel: ScrollFeel) {
        self.feel = feel;
    }

    /// Hand a frame to `emit`, stripped of its phases when the edge bounce
    /// is off: plain continuous scrolls stop hard at the content edge.
    fn sink(bounce: bool, emit: &mut impl FnMut(ScrollFrame)) -> impl FnMut(ScrollFrame) {
        move |frame: ScrollFrame| {
            if bounce {
                emit(frame);
            } else if !frame.delta.is_zero() {
                emit(ScrollFrame::new(frame.delta, SmoothScrollPhase::Unphased));
            }
        }
    }

    fn impulse(
        &mut self,
        source: ScrollSource,
        impulse: WheelDelta,
        at: Instant,
        emit: &mut impl FnMut(ScrollFrame),
    ) {
        let feel = self.feel;
        let new_burst = self
            .last_impulse
            .is_none_or(|last| at.saturating_duration_since(last) >= feel.pause);
        self.last_impulse = Some(at);
        if new_burst {
            self.output.touch(&mut Self::sink(feel.bounce, emit));
        }
        let update = match self.active.entry(source) {
            Entry::Occupied(mut entry) => {
                let update = entry.get_mut().retarget(impulse, at, feel);
                if update.is_finished() {
                    entry.remove();
                }
                Some(update)
            }
            Entry::Vacant(entry) => {
                entry.insert(ActiveMotion::new(impulse, at));
                None
            }
        };
        if let Some(update) = update {
            self.emit_update(update, at, emit);
        }
    }

    fn advance_due(&mut self, at: Instant, emit: &mut impl FnMut(ScrollFrame)) {
        let feel = self.feel;
        let due: Vec<ScrollSource> = self
            .active
            .iter()
            .filter(|(_, motion)| motion.next_frame <= at)
            .map(|(source, _)| source.clone())
            .collect();
        for source in due {
            let Some(update) = self
                .active
                .get_mut(&source)
                .map(|motion| motion.advance(at, feel))
            else {
                continue;
            };
            if update.is_finished() {
                self.active.remove(&source);
            }
            self.emit_update(update, at, emit);
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.active.values().map(|motion| motion.next_frame).min()
    }

    fn cancel_source(&mut self, source: &ScrollSource, emit: &mut impl FnMut(ScrollFrame)) {
        if self.active.remove(source).is_some() && self.active.is_empty() {
            self.output.cancel(&mut Self::sink(self.feel.bounce, emit));
        }
    }

    fn cancel_all(&mut self, emit: &mut impl FnMut(ScrollFrame)) {
        self.active.clear();
        self.output.cancel(&mut Self::sink(self.feel.bounce, emit));
    }

    fn emit_update(
        &mut self,
        update: MotionUpdate,
        at: Instant,
        emit: &mut impl FnMut(ScrollFrame),
    ) {
        let mut emit = Self::sink(self.feel.bounce, emit);
        match update {
            MotionUpdate::Finished(delta) if self.active.is_empty() => {
                self.output.finish(delta, &mut emit);
            }
            MotionUpdate::Active(delta) | MotionUpdate::Finished(delta) => {
                self.output.progress(delta, self.feel.touch, at, &mut emit);
            }
        }
    }
}

#[cfg(test)]
mod tests;
