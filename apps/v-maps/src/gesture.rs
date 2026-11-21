//! What a touch was, from where the finger went down and came up.

/// A touch that moved this far sideways (and mostly sideways) is a swipe.
pub const SWIPE_PX: i32 = 70;
/// A touch that stayed within this far is a tap or a hold.
pub const TAP_PX: i32 = 36;
/// A touch held this long (and still) switches between the map turning with you and the map held where it is.
pub const HOLD_MS: u32 = 600;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gesture {
    None,
    Tap,
    Hold,
    SwipeRight,
    SwipeLeft,
}

/// `dx`, `dy`: how far the finger went from where it came down (right and down are positive); `held_ms`: for how long.
pub fn classify(dx: i32, dy: i32, held_ms: u32) -> Gesture {
    let (ax, ay) = (dx.abs(), dy.abs());
    if ax >= SWIPE_PX && ax >= 2 * ay {
        return if dx > 0 { Gesture::SwipeRight } else { Gesture::SwipeLeft };
    }
    if ax.max(ay) <= TAP_PX {
        return if held_ms >= HOLD_MS { Gesture::Hold } else { Gesture::Tap };
    }
    Gesture::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_holds_and_swipes() {
        assert_eq!(classify(3, -4, 120), Gesture::Tap);
        assert_eq!(classify(0, 0, 700), Gesture::Hold);
        assert_eq!(classify(150, 20, 200), Gesture::SwipeRight);
        assert_eq!(classify(-150, -30, 400), Gesture::SwipeLeft);
        // a slow swipe is still a swipe, not a hold
        assert_eq!(classify(120, 0, 1500), Gesture::SwipeRight);
    }

    #[test]
    fn a_diagonal_or_short_drag_is_nothing() {
        assert_eq!(classify(100, 100, 200), Gesture::None);
        assert_eq!(classify(100, -80, 200), Gesture::None);
        assert_eq!(classify(50, 0, 200), Gesture::None);
        assert_eq!(classify(0, 120, 200), Gesture::None);
    }

    #[test]
    fn the_edges_of_the_ranges() {
        assert_eq!(classify(SWIPE_PX, 0, 100), Gesture::SwipeRight);
        assert_eq!(classify(SWIPE_PX - 1, 0, 100), Gesture::None);
        assert_eq!(classify(TAP_PX, 0, 100), Gesture::Tap);
        assert_eq!(classify(TAP_PX + 1, 0, 100), Gesture::None);
    }
}
