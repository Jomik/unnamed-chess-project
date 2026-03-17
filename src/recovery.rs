use shakmaty::{Bitboard, ByColor};

use crate::feedback::{BoardFeedback, SquareFeedback};

/// Compute recovery feedback when the physical board diverges from the game state.
///
/// Returns `None` when the physical board matches. Otherwise highlights:
/// - `Origin` on squares with unexpected pieces (remove these)
/// - `Destination` on squares missing expected pieces (place pieces here)
pub fn recovery_feedback(
    expected: &ByColor<Bitboard>,
    current: &ByColor<Bitboard>,
) -> Option<BoardFeedback> {
    let expected_all = expected.white | expected.black;
    let current_all = current.white | current.black;

    let missing = expected_all & !current_all;
    let extra = current_all & !expected_all;

    if missing.is_empty() && extra.is_empty() {
        return None;
    }

    let mut fb = BoardFeedback::new();
    for sq in missing {
        fb.set(sq, SquareFeedback::Destination);
    }
    for sq in extra {
        fb.set(sq, SquareFeedback::Origin);
    }
    Some(fb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::Square;

    fn starting() -> ByColor<Bitboard> {
        use shakmaty::{Chess, Color, Position};
        let chess = Chess::default();
        let board = chess.board();
        ByColor {
            white: board.by_color(Color::White),
            black: board.by_color(Color::Black),
        }
    }

    #[test]
    fn matching_boards_returns_none() {
        let expected = starting();
        assert!(recovery_feedback(&expected, &expected).is_none());
    }

    #[test]
    fn missing_piece_shows_destination() {
        let expected = starting();
        let current = ByColor {
            white: expected.white ^ Bitboard::from(Square::E1),
            black: expected.black,
        };

        let fb = recovery_feedback(&expected, &current).expect("should have feedback");
        assert_eq!(fb.get(Square::E1), Some(SquareFeedback::Destination));
        assert_eq!(fb.squares().count(), 1);
    }

    #[test]
    fn extra_piece_shows_origin() {
        let expected = starting();
        let current = ByColor {
            white: expected.white | Bitboard::from(Square::E4),
            black: expected.black,
        };

        let fb = recovery_feedback(&expected, &current).expect("should have feedback");
        assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(fb.squares().count(), 1);
    }

    #[test]
    fn missing_and_extra_combined() {
        let expected = starting();
        let current = ByColor {
            white: (expected.white ^ Bitboard::from(Square::E2)) | Bitboard::from(Square::E4),
            black: expected.black,
        };

        let fb = recovery_feedback(&expected, &current).expect("should have feedback");
        assert_eq!(fb.get(Square::E2), Some(SquareFeedback::Destination));
        assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(fb.squares().count(), 2);
    }

    #[test]
    fn piece_on_wrong_color_detected() {
        let expected = ByColor {
            white: Bitboard::from(Square::E2),
            black: Bitboard::EMPTY,
        };
        let current = ByColor {
            white: Bitboard::EMPTY,
            black: Bitboard::from(Square::E2),
        };

        assert!(recovery_feedback(&expected, &current).is_none());
    }
}
