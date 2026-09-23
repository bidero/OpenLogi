//! Core Graphics supplies front-to-back window rectangles in global points.

use crate::{CursorPosition, PointerTarget};

pub(crate) struct Window {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub target: PointerTarget,
}

/// A malformed row is not evidence that the space behind it is desktop.
impl Window {
    /// Whether this window has exactly the given bounds, within half a point
    /// (window-list and display bounds are integral points reported as
    /// floats).
    pub(crate) fn has_bounds(&self, x: f64, y: f64, width: f64, height: f64) -> bool {
        let same = |a: f64, b: f64| (a - b).abs() < 0.5;
        same(self.x, x) && same(self.y, y) && same(self.width, width) && same(self.height, height)
    }
}

pub(crate) fn hit_test(
    point: CursorPosition,
    windows: impl IntoIterator<Item = Option<Window>>,
) -> PointerTarget {
    for window in windows {
        let Some(window) = window else {
            return PointerTarget::Unavailable;
        };
        if point.x >= window.x
            && point.y >= window.y
            && point.x < window.x + window.width
            && point.y < window.y + window.height
        {
            return window.target;
        }
    }
    PointerTarget::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_bounds_matches_a_display_sized_window_only() {
        let dock_overlay = window(0.0, 0.0, 1280.0, 800.0, PointerTarget::Unavailable);
        assert!(dock_overlay.has_bounds(0.0, 0.0, 1280.0, 800.0));
        assert!(dock_overlay.has_bounds(0.2, 0.0, 1280.0, 799.8));
        let dock_strip = window(300.0, 730.0, 680.0, 70.0, PointerTarget::Unavailable);
        assert!(!dock_strip.has_bounds(0.0, 0.0, 1280.0, 800.0));
        // A second display at a negative origin is its own match.
        let other = window(-1920.0, 0.0, 1920.0, 1080.0, PointerTarget::Unavailable);
        assert!(other.has_bounds(-1920.0, 0.0, 1920.0, 1080.0));
        assert!(!other.has_bounds(0.0, 0.0, 1920.0, 1080.0));
    }

    fn window(x: f64, y: f64, width: f64, height: f64, target: PointerTarget) -> Window {
        Window {
            x,
            y,
            width,
            height,
            target,
        }
    }

    #[test]
    fn pointer_hit_test_preserves_z_order_and_negative_display_coordinates() {
        let front = PointerTarget::Window {
            process_id: 41,
            window_id: 7,
        };
        let back = PointerTarget::Window {
            process_id: 41,
            window_id: 9,
        };
        assert_eq!(
            hit_test(
                CursorPosition { x: -70.0, y: 31.0 },
                [
                    Some(window(-90.0, 30.0, 40.0, 80.0, front)),
                    Some(window(-100.0, 0.0, 100.0, 120.0, back)),
                ]
            ),
            front
        );
        // The right edge belongs to the adjacent surface, not the front window.
        assert_eq!(
            hit_test(
                CursorPosition { x: -50.0, y: 31.0 },
                [
                    Some(window(-90.0, 30.0, 40.0, 80.0, front)),
                    Some(window(-100.0, 0.0, 100.0, 120.0, back)),
                ]
            ),
            back
        );
    }

    #[test]
    fn pointer_hit_test_requires_positive_desktop_evidence_and_respects_obstacles() {
        let point = CursorPosition { x: 13.0, y: 27.0 };
        assert_eq!(hit_test(point, []), PointerTarget::Unavailable);
        assert_eq!(
            hit_test(
                point,
                [
                    None,
                    Some(window(0.0, 0.0, 50.0, 50.0, PointerTarget::Desktop))
                ]
            ),
            PointerTarget::Unavailable
        );
        assert_eq!(
            hit_test(
                point,
                [Some(window(0.0, 0.0, 50.0, 50.0, PointerTarget::Desktop))]
            ),
            PointerTarget::Desktop
        );
        assert_eq!(
            hit_test(
                point,
                [
                    Some(window(10.0, 20.0, 8.0, 12.0, PointerTarget::Unavailable)),
                    Some(window(0.0, 0.0, 50.0, 50.0, PointerTarget::Desktop))
                ]
            ),
            PointerTarget::Unavailable
        );
    }
}
