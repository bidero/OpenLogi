//! Synthetic motion-model traces. These values are algorithm fixtures, not
//! measurements captured from physical hardware.

use super::*;

fn source() -> ScrollSource {
    ScrollSource::current_hook()
}

fn hidpp_source(device_key: &str, epoch: u64) -> ScrollSource {
    ScrollSource::Hidpp(HidppSessionId::with_epoch(device_key, epoch))
}

fn wheel(x: f64, y: f64) -> WheelDelta {
    WheelDelta { x, y }
}

fn cumulative(frames: &[ScrollFrame]) -> WheelDelta {
    frames
        .iter()
        .fold(WheelDelta::ZERO, |sum, frame| sum.plus(frame.delta))
}

fn assert_delta(actual: WheelDelta, expected: WheelDelta) {
    const EPSILON: f64 = 1.0e-9;
    assert!(
        (actual.x - expected.x).abs() < EPSILON,
        "x: {} != {}",
        actual.x,
        expected.x
    );
    assert!(
        (actual.y - expected.y).abs() < EPSILON,
        "y: {} != {}",
        actual.y,
        expected.y
    );
}

/// A 100 ms time constant without acceleration, so traces are easy to read.
fn engine() -> ScrollEngine {
    engine_with(0.0)
}

fn engine_with(acceleration: f64) -> ScrollEngine {
    let mut engine = ScrollEngine::default();
    engine.set_feel(ScrollFeel {
        time_constant: 0.1,
        acceleration,
    });
    engine
}

/// Long enough for any test glide to settle.
const SETTLED: Duration = Duration::from_secs(3);

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn a_notch_eases_out_and_finishes_exactly() {
    let base = Instant::now();
    let mut engine = engine();
    let mut frames = Vec::new();
    engine.impulse(source(), wheel(0.0, 1.0), base, &mut |frame| {
        frames.push(frame);
    });

    engine.advance_due(base + ms(100), &mut |frame| frames.push(frame));
    assert_delta(cumulative(&frames), wheel(0.0, 1.0 - (-1.0_f64).exp()));

    engine.advance_due(base + SETTLED, &mut |frame| frames.push(frame));
    assert_delta(cumulative(&frames), wheel(0.0, 1.0));
    assert_eq!(
        frames.first().map(|frame| frame.phase),
        Some(SmoothScrollPhase::Began)
    );
    assert_eq!(
        frames.last().map(|frame| frame.phase),
        Some(SmoothScrollPhase::Ended)
    );
    assert!(engine.active.is_empty());
}

#[test]
fn a_burst_merges_into_one_motion_without_losing_distance() {
    let base = Instant::now();
    let mut engine = engine();
    let mut frames = Vec::new();
    for millis in [0, 10, 20] {
        engine.impulse(
            source(),
            wheel(0.0, 0.25),
            base + ms(millis),
            &mut |frame| {
                frames.push(frame);
            },
        );
    }
    engine.advance_due(base + SETTLED, &mut |frame| frames.push(frame));

    assert_delta(cumulative(&frames), wheel(0.0, 0.75));
    let began = frames
        .iter()
        .filter(|frame| frame.phase == SmoothScrollPhase::Began)
        .count();
    assert_eq!(began, 1, "one continuous gesture, not one per notch");
    assert!(engine.active.is_empty());
}

#[test]
fn acceleration_lengthens_only_a_fast_spin() {
    let base = Instant::now();
    let mut fast = engine_with(1.0);
    let mut frames = Vec::new();
    fast.impulse(source(), wheel(0.0, 1.0), base, &mut |frame| {
        frames.push(frame);
    });
    fast.impulse(source(), wheel(0.0, 1.0), base + ms(15), &mut |frame| {
        frames.push(frame);
    });
    fast.advance_due(base + SETTLED, &mut |frame| frames.push(frame));
    // Second notch: 1 + 4 × (1 − 15/150) = 4.6× its distance.
    assert_delta(cumulative(&frames), wheel(0.0, 5.6));

    let mut slow = engine_with(1.0);
    let mut frames = Vec::new();
    slow.impulse(source(), wheel(0.0, 1.0), base, &mut |frame| {
        frames.push(frame);
    });
    slow.impulse(source(), wheel(0.0, 1.0), base + ms(200), &mut |frame| {
        frames.push(frame);
    });
    slow.advance_due(base + SETTLED, &mut |frame| frames.push(frame));
    assert_delta(cumulative(&frames), wheel(0.0, 2.0));
}

#[test]
fn a_reversal_drops_the_old_glide_and_answers_at_once() {
    let base = Instant::now();
    let mut engine = engine();
    let mut frames = Vec::new();
    engine.impulse(source(), wheel(0.0, 1.0), base, &mut |frame| {
        frames.push(frame);
    });
    engine.impulse(source(), wheel(0.0, -1.0), base + ms(50), &mut |frame| {
        frames.push(frame);
    });
    engine.advance_due(base + SETTLED, &mut |frame| frames.push(frame));

    let covered_before_reversal = 1.0 - (-0.5_f64).exp();
    assert_delta(
        cumulative(&frames),
        wheel(0.0, covered_before_reversal - 1.0),
    );
    assert!(engine.active.is_empty());
}

#[test]
fn sparse_notches_form_separate_gestures() {
    let base = Instant::now();
    let mut engine = engine();
    let mut frames = Vec::new();
    engine.impulse(source(), wheel(0.0, 1.0), base, &mut |frame| {
        frames.push(frame);
    });
    engine.advance_due(base + SETTLED, &mut |frame| frames.push(frame));
    assert!(engine.active.is_empty());

    let later = base + SETTLED + ms(100);
    engine.impulse(source(), wheel(0.0, 2.0), later, &mut |frame| {
        frames.push(frame);
    });
    engine.advance_due(later + SETTLED, &mut |frame| frames.push(frame));
    assert_delta(cumulative(&frames), wheel(0.0, 3.0));
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame.phase == SmoothScrollPhase::Began)
            .count(),
        2
    );
}

#[test]
fn delayed_frames_use_real_time_not_frame_count() {
    let base = Instant::now();
    let mut dense = engine();
    let mut dense_frames = Vec::new();
    dense.impulse(source(), wheel(0.0, 1.0), base, &mut |frame| {
        dense_frames.push(frame);
    });
    for millis in (8..=80).step_by(8) {
        dense.advance_due(base + ms(millis), &mut |frame| dense_frames.push(frame));
    }

    let mut delayed = engine();
    let mut delayed_frames = Vec::new();
    delayed.impulse(source(), wheel(0.0, 1.0), base, &mut |frame| {
        delayed_frames.push(frame);
    });
    delayed.advance_due(base + ms(80), &mut |frame| delayed_frames.push(frame));
    assert_delta(cumulative(&dense_frames), cumulative(&delayed_frames));
}

#[test]
fn only_finite_nonzero_wheel_ticks_enter_the_model() {
    assert_eq!(
        WheelDelta::try_from(ScrollDelta::wheel_ticks(0.25, -1.0)),
        Ok(wheel(0.25, -1.0))
    );
    WheelDelta::try_from(ScrollDelta::pixels(0.0, 1.0)).unwrap_err();
    WheelDelta::try_from(ScrollDelta::wheel_ticks(0.0, 0.0)).unwrap_err();
    WheelDelta::try_from(ScrollDelta::wheel_ticks(f64::NAN, 1.0)).unwrap_err();
}

#[test]
fn cancellation_emits_one_terminal_phase_only_after_output_began() {
    let base = Instant::now();
    let mut engine = engine();
    let mut frames = Vec::new();
    engine.impulse(source(), wheel(1.0, 0.0), base, &mut |frame| {
        frames.push(frame);
    });
    engine.cancel_all(&mut |frame| frames.push(frame));
    assert!(frames.is_empty());

    engine.impulse(source(), wheel(1.0, 0.0), base, &mut |frame| {
        frames.push(frame);
    });
    engine.advance_due(base + ms(25), &mut |frame| frames.push(frame));
    engine.cancel_all(&mut |frame| frames.push(frame));
    assert_eq!(
        frames.last().map(|frame| frame.phase),
        Some(SmoothScrollPhase::Cancelled)
    );
    assert_delta(cumulative(&frames), wheel(1.0 - (-0.25_f64).exp(), 0.0));
}

#[test]
fn concurrent_sources_share_one_balanced_output_stream() {
    let base = Instant::now();
    let first = hidpp_source("mouse-a", 1);
    let second = hidpp_source("mouse-b", 1);
    let mut engine = engine();
    let mut frames = Vec::new();
    engine.impulse(first, wheel(1.0, 0.0), base, &mut |frame| {
        frames.push(frame);
    });
    engine.impulse(second, wheel(0.0, 1.0), base, &mut |frame| {
        frames.push(frame);
    });
    engine.advance_due(base + ms(25), &mut |frame| frames.push(frame));
    engine.advance_due(base + SETTLED, &mut |frame| frames.push(frame));

    assert_delta(cumulative(&frames), wheel(1.0, 1.0));
    for (phase, expected) in [
        (SmoothScrollPhase::Began, 1),
        (SmoothScrollPhase::Ended, 1),
        (SmoothScrollPhase::Cancelled, 0),
    ] {
        assert_eq!(
            frames.iter().filter(|frame| frame.phase == phase).count(),
            expected,
            "{phase:?}"
        );
    }
    assert!(engine.active.is_empty());
}

#[test]
fn source_cancellation_does_not_interrupt_another_source() {
    let base = Instant::now();
    let first = hidpp_source("mouse-a", 1);
    let second = hidpp_source("mouse-b", 1);
    let mut engine = engine();
    let mut frames = Vec::new();
    engine.impulse(first.clone(), wheel(1.0, 0.0), base, &mut |_| {});
    engine.impulse(second.clone(), wheel(0.0, 1.0), base, &mut |_| {});
    engine.advance_due(base + ms(25), &mut |frame| frames.push(frame));

    engine.cancel_source(&first, &mut |frame| frames.push(frame));
    assert!(!engine.active.contains_key(&first));
    assert!(engine.active.contains_key(&second));
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame.phase == SmoothScrollPhase::Cancelled)
            .count(),
        0,
        "a source-local cancellation cannot terminate the shared output stream"
    );

    engine.advance_due(base + SETTLED, &mut |frame| frames.push(frame));
    assert!(engine.active.is_empty());
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame.phase == SmoothScrollPhase::Ended)
            .count(),
        1,
        "the other device completes normally"
    );
}

#[test]
fn the_default_feel_glides_for_a_few_hundred_milliseconds() {
    let feel = ScrollFeel::default();
    // 95 % of a notch is covered by three time constants.
    let settle = Duration::from_secs_f64(feel.time_constant * 3.0);
    assert!(settle > ms(300) && settle < ms(450), "{settle:?}");
}

use SmoothScrollPhase::{Began, Changed, Ended, MomentumBegan, MomentumChanged, MomentumEnded};

fn phases(frames: &[ScrollFrame]) -> Vec<SmoothScrollPhase> {
    let mut phases: Vec<SmoothScrollPhase> = frames.iter().map(|frame| frame.phase).collect();
    phases.dedup();
    phases
}

#[test]
fn the_glide_coasts_as_momentum_once_the_wheel_pauses() {
    let base = Instant::now();
    let mut engine = engine();
    let mut frames = Vec::new();
    engine.impulse(source(), wheel(0.0, 1.0), base, &mut |frame| {
        frames.push(frame);
    });
    for millis in (8..=800).step_by(8) {
        engine.advance_due(base + ms(millis), &mut |frame| frames.push(frame));
    }

    assert_eq!(
        phases(&frames),
        [
            Began,
            Changed,
            Ended,
            MomentumBegan,
            MomentumChanged,
            MomentumEnded
        ]
    );
    assert_delta(cumulative(&frames), wheel(0.0, 1.0));
    let touching: f64 = frames
        .iter()
        .filter(|frame| matches!(frame.phase, Began | Changed))
        .map(|frame| frame.delta.y)
        .sum();
    assert!(touching < 0.5, "most of the glide coasts: {touching}");
}

#[test]
fn a_notch_during_the_coast_starts_a_new_gesture() {
    let base = Instant::now();
    let mut engine = engine();
    let mut frames = Vec::new();
    engine.impulse(source(), wheel(0.0, 1.0), base, &mut |frame| {
        frames.push(frame);
    });
    for millis in (8..=80).step_by(8) {
        engine.advance_due(base + ms(millis), &mut |frame| frames.push(frame));
    }
    engine.impulse(source(), wheel(0.0, 1.0), base + ms(85), &mut |frame| {
        frames.push(frame);
    });
    for millis in (88..=1600).step_by(8) {
        engine.advance_due(base + ms(millis), &mut |frame| frames.push(frame));
    }

    assert_eq!(
        phases(&frames),
        [
            Began,
            Changed,
            Ended,
            MomentumBegan,
            MomentumChanged,
            MomentumEnded,
            Began,
            Changed,
            Ended,
            MomentumBegan,
            MomentumChanged,
            MomentumEnded,
        ]
    );
    assert_delta(cumulative(&frames), wheel(0.0, 2.0));
}

#[test]
fn a_continuous_spin_coasts_after_a_short_touch() {
    let base = Instant::now();
    let mut engine = engine();
    let mut frames = Vec::new();
    for millis in (0..=400).step_by(4) {
        let at = base + ms(millis);
        if millis % 12 == 0 {
            engine.impulse(source(), wheel(0.0, 1.0), at, &mut |frame| {
                frames.push(frame)
            });
        }
        engine.advance_due(at, &mut |frame| frames.push(frame));
    }
    engine.advance_due(base + SETTLED, &mut |frame| frames.push(frame));

    assert_eq!(
        phases(&frames),
        [
            Began,
            Changed,
            Ended,
            MomentumBegan,
            MomentumChanged,
            MomentumEnded
        ],
        "the spin keeps coasting instead of restarting gestures"
    );
    let last_touch = frames
        .iter()
        .rposition(|frame| frame.phase == Ended)
        .expect("touch ends");
    assert!(
        last_touch < 12,
        "touch lasts only a few frames: {last_touch}"
    );
    assert_delta(cumulative(&frames), wheel(0.0, 34.0));
}
