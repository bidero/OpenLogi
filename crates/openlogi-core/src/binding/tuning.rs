//! Per-device timing and travel thresholds for gestures and long presses.
//!
//! Each value is a validated newtype whose `DEFAULT` is the single owner of
//! the out-of-the-box behavior; [`GestureTuning`] groups the three so the
//! agent can carry one per device.

use std::time::Duration;

use az::SaturatingAs;
use nutype::nutype;

const SWIPE_DISTANCE_MIN: u16 = 20;
const SWIPE_DISTANCE_MAX: u16 = 200;
const SWIPE_DISTANCE_DEFAULT: u16 = 50;

const SWIPE_HOLD_MIN_MS: u16 = 0;
const SWIPE_HOLD_MAX_MS: u16 = 500;
const SWIPE_HOLD_DEFAULT_MS: u16 = 160;

const LONG_PRESS_MIN_MS: u16 = 200;
const LONG_PRESS_MAX_MS: u16 = 1500;
const LONG_PRESS_DEFAULT_MS: u16 = 500;

/// Round and clamp a floating-point slider value into `min..=max`.
fn rounded(value: f32, min: u16, max: u16) -> u16 {
    let value = if value.is_nan() {
        f32::from(min)
    } else {
        value
    };
    value
        .clamp(f32::from(min), f32::from(max))
        .round()
        .saturating_as::<u16>()
}

/// Minimum dominant-axis travel (raw-XY units) before a held gesture commits
/// to a direction. Larger values need a longer, more deliberate swipe.
#[nutype(
    const_fn,
    validate(greater_or_equal = SWIPE_DISTANCE_MIN, less_or_equal = SWIPE_DISTANCE_MAX),
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        TryFrom,
        Into,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct SwipeDistance(u16);

impl SwipeDistance {
    /// Shortest selectable swipe.
    pub const MIN: Self = match Self::try_new(SWIPE_DISTANCE_MIN) {
        Ok(value) => value,
        Err(_) => panic!("valid minimum swipe distance"),
    };
    /// Longest selectable swipe.
    pub const MAX: Self = match Self::try_new(SWIPE_DISTANCE_MAX) {
        Ok(value) => value,
        Err(_) => panic!("valid maximum swipe distance"),
    };
    /// Out-of-the-box travel, tuned to match Logitech Options+.
    pub const DEFAULT: Self = match Self::try_new(SWIPE_DISTANCE_DEFAULT) {
        Ok(value) => value,
        Err(_) => panic!("valid default swipe distance"),
    };

    /// Round and clamp a slider value into the valid range.
    #[must_use]
    pub fn from_rounded(value: f32) -> Self {
        let Ok(value) = Self::try_new(rounded(value, SWIPE_DISTANCE_MIN, SWIPE_DISTANCE_MAX))
        else {
            unreachable!("clamped swipe distance is always valid");
        };
        value
    }

    /// The travel as a raw-XY magnitude.
    #[must_use]
    pub fn units(self) -> i32 {
        i32::from(self.into_inner())
    }
}

impl Default for SwipeDistance {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl From<SwipeDistance> for f32 {
    fn from(value: SwipeDistance) -> Self {
        Self::from(value.into_inner())
    }
}

/// How long (ms) a gesture button must be held before its travel can commit
/// to a swipe, so a quick click whose cursor drifted stays a click.
#[nutype(
    const_fn,
    validate(greater_or_equal = SWIPE_HOLD_MIN_MS, less_or_equal = SWIPE_HOLD_MAX_MS),
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        TryFrom,
        Into,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct SwipeHold(u16);

impl SwipeHold {
    /// Shortest selectable hold (no delay).
    pub const MIN: Self = match Self::try_new(SWIPE_HOLD_MIN_MS) {
        Ok(value) => value,
        Err(_) => panic!("valid minimum swipe hold"),
    };
    /// Longest selectable hold.
    pub const MAX: Self = match Self::try_new(SWIPE_HOLD_MAX_MS) {
        Ok(value) => value,
        Err(_) => panic!("valid maximum swipe hold"),
    };
    /// Out-of-the-box hold.
    pub const DEFAULT: Self = match Self::try_new(SWIPE_HOLD_DEFAULT_MS) {
        Ok(value) => value,
        Err(_) => panic!("valid default swipe hold"),
    };

    /// Round and clamp a slider value (milliseconds) into the valid range.
    #[must_use]
    pub fn from_rounded(value: f32) -> Self {
        let Ok(value) = Self::try_new(rounded(value, SWIPE_HOLD_MIN_MS, SWIPE_HOLD_MAX_MS)) else {
            unreachable!("clamped swipe hold is always valid");
        };
        value
    }

    /// The hold as a [`Duration`].
    #[must_use]
    pub fn duration(self) -> Duration {
        Duration::from_millis(u64::from(self.into_inner()))
    }
}

impl Default for SwipeHold {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl From<SwipeHold> for f32 {
    fn from(value: SwipeHold) -> Self {
        Self::from(value.into_inner())
    }
}

/// How long (ms) a button must stay down before its long-press action fires.
#[nutype(
    const_fn,
    validate(greater_or_equal = LONG_PRESS_MIN_MS, less_or_equal = LONG_PRESS_MAX_MS),
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        TryFrom,
        Into,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct LongPressDelay(u16);

impl LongPressDelay {
    /// Shortest selectable long press.
    pub const MIN: Self = match Self::try_new(LONG_PRESS_MIN_MS) {
        Ok(value) => value,
        Err(_) => panic!("valid minimum long-press delay"),
    };
    /// Longest selectable long press.
    pub const MAX: Self = match Self::try_new(LONG_PRESS_MAX_MS) {
        Ok(value) => value,
        Err(_) => panic!("valid maximum long-press delay"),
    };
    /// Out-of-the-box long press.
    pub const DEFAULT: Self = match Self::try_new(LONG_PRESS_DEFAULT_MS) {
        Ok(value) => value,
        Err(_) => panic!("valid default long-press delay"),
    };

    /// Round and clamp a slider value (milliseconds) into the valid range.
    #[must_use]
    pub fn from_rounded(value: f32) -> Self {
        let Ok(value) = Self::try_new(rounded(value, LONG_PRESS_MIN_MS, LONG_PRESS_MAX_MS)) else {
            unreachable!("clamped long-press delay is always valid");
        };
        value
    }

    /// The delay as a [`Duration`].
    #[must_use]
    pub fn duration(self) -> Duration {
        Duration::from_millis(u64::from(self.into_inner()))
    }
}

impl Default for LongPressDelay {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl From<LongPressDelay> for f32 {
    fn from(value: LongPressDelay) -> Self {
        Self::from(value.into_inner())
    }
}

/// One device's gesture and long-press thresholds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GestureTuning {
    /// Travel before a held gesture commits a direction.
    pub swipe_distance: SwipeDistance,
    /// Hold before travel can commit a swipe.
    pub swipe_hold: SwipeHold,
    /// Hold before a long-press action fires.
    pub long_press: LongPressDelay,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_the_previous_constants() {
        let tuning = GestureTuning::default();
        assert_eq!(tuning.swipe_distance.units(), 50);
        assert_eq!(tuning.swipe_hold.duration(), Duration::from_millis(160));
        assert_eq!(tuning.long_press.duration(), Duration::from_millis(500));
    }

    #[test]
    fn slider_values_are_rounded_and_clamped() {
        assert_eq!(SwipeDistance::from_rounded(5.0), SwipeDistance::MIN);
        assert_eq!(SwipeDistance::from_rounded(1e6), SwipeDistance::MAX);
        assert_eq!(SwipeDistance::from_rounded(f32::NAN), SwipeDistance::MIN);
        assert_eq!(SwipeHold::from_rounded(-3.0), SwipeHold::MIN);
        assert_eq!(
            SwipeHold::from_rounded(249.6).duration(),
            Duration::from_millis(250)
        );
        assert_eq!(LongPressDelay::from_rounded(9999.0), LongPressDelay::MAX);
        assert_eq!(LongPressDelay::from_rounded(0.0), LongPressDelay::MIN);
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        SwipeDistance::try_new(19).unwrap_err();
        SwipeHold::try_new(501).unwrap_err();
        LongPressDelay::try_new(199).unwrap_err();
    }
}
