//! Synthetic trackpad pinch (magnify) gestures: continuous zoom from a wheel
//! and single zoom steps from a button.
//!
//! There is no public API for this. Like Smart Zoom, a pinch is a `CGEvent`
//! of the undocumented type 29 (`kCGSEventGesture`, set through field 55)
//! whose field 110 (`kCGEventGestureHIDType`) names the gesture,
//! `kIOHIDEventTypeZoom` = 8, carrying the magnification delta in field 113
//! and the gesture phase in field 132. Applications handle it as
//! `NSEventTypeMagnify`, zooming around the pointer.

use core_graphics::event::{CGEvent, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use super::tag_synthetic;

const EVENT_TYPE_FIELD: u32 = 55;
const GESTURE_EVENT_TYPE: i64 = 29;
const GESTURE_HID_TYPE_FIELD: u32 = 110;
const HID_ZOOM: i64 = 8;
const MAGNIFICATION_FIELD: u32 = 113;
const GESTURE_PHASE_FIELD: u32 = 132;

/// `NSEventPhase` values carried in [`GESTURE_PHASE_FIELD`].
#[derive(Clone, Copy)]
enum Phase {
    Began = 1,
    Changed = 2,
    Ended = 4,
}

/// Quiet time after which a wheel-driven pinch ends. Applications keep
/// waiting for the end of a gesture, so one must always follow.
const IDLE_END: Duration = Duration::from_millis(120);

/// Magnification of one button-driven zoom step.
const STEP: f64 = 0.25;

/// When the open wheel-driven pinch last moved, or `None` between pinches.
static ACTIVE_PINCH: Mutex<Option<Instant>> = Mutex::new(None);

/// Add one wheel-driven increment to the open pinch, opening it first if
/// needed. A watchdog ends the pinch once the wheel rests for [`IDLE_END`].
pub(in crate::inject) fn post_magnify(magnification: f64) {
    let Ok(mut active) = ACTIVE_PINCH.lock() else {
        tracing::warn!("macOS pinch state mutex poisoned");
        return;
    };
    let began = active.is_none();
    *active = Some(Instant::now());
    post(
        if began { Phase::Began } else { Phase::Changed },
        magnification,
    );
    drop(active);
    if began {
        thread::spawn(end_when_idle);
    }
}

/// Close the open pinch after the wheel has rested for [`IDLE_END`].
fn end_when_idle() {
    loop {
        thread::sleep(IDLE_END / 2);
        let Ok(mut active) = ACTIVE_PINCH.lock() else {
            return;
        };
        match *active {
            Some(last) if last.elapsed() < IDLE_END => {}
            Some(_) => {
                post(Phase::Ended, 0.0);
                *active = None;
                return;
            }
            None => return,
        }
    }
}

/// Post one complete zoom step: `1.0` zooms in, `-1.0` out.
pub(super) fn zoom_step(direction: f64) {
    post(Phase::Began, STEP * direction);
    post(Phase::Ended, 0.0);
}

fn post(phase: Phase, magnification: f64) {
    let Ok(src) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        tracing::warn!("CGEventSource::new failed for pinch");
        return;
    };
    let Ok(event) = CGEvent::new(src) else {
        tracing::warn!("CGEvent::new failed for pinch");
        return;
    };
    event.set_integer_value_field(EVENT_TYPE_FIELD, GESTURE_EVENT_TYPE);
    event.set_integer_value_field(GESTURE_HID_TYPE_FIELD, HID_ZOOM);
    event.set_double_value_field(MAGNIFICATION_FIELD, magnification);
    event.set_integer_value_field(GESTURE_PHASE_FIELD, phase as i64);
    tag_synthetic(&event);
    event.post(CGEventTapLocation::HID);
}
