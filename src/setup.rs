use shakmaty::{Bitboard, ByColor, Chess, Color, Position};

use crate::feedback::{BoardFeedback, SquareFeedback};

fn starting_positions() -> ByColor<Bitboard> {
    let chess = Chess::default();
    let board = chess.board();
    ByColor {
        white: board.by_color(Color::White),
        black: board.by_color(Color::Black),
    }
}

/// Compute setup feedback showing which squares still need pieces.
///
/// Returns `None` when the board matches the starting position.
/// Uses `Destination` for missing white pieces and `Capture` for missing black pieces.
pub fn setup_feedback(current: &ByColor<Bitboard>) -> Option<BoardFeedback> {
    let expected = starting_positions();
    let missing_white = expected.white & !current.white;
    let missing_black = expected.black & !current.black;

    if missing_white.is_empty() && missing_black.is_empty() {
        return None;
    }

    let mut fb = BoardFeedback::new();
    for sq in missing_white {
        fb.set(sq, SquareFeedback::Destination);
    }
    for sq in missing_black {
        fb.set(sq, SquareFeedback::Capture);
    }
    Some(fb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::Square;

    #[test]
    fn complete_starting_position_returns_none() {
        let positions = starting_positions();
        assert!(setup_feedback(&positions).is_none());
    }

    #[test]
    fn empty_board_shows_all_squares() {
        let positions = ByColor {
            white: Bitboard::EMPTY,
            black: Bitboard::EMPTY,
        };

        let fb = setup_feedback(&positions).expect("should have feedback");
        let count = fb.squares().count();
        assert_eq!(count, 32);
    }

    #[test]
    fn missing_single_white_piece() {
        let expected = starting_positions();
        let positions = ByColor {
            white: expected.white ^ Bitboard::from(Square::E1),
            black: expected.black,
        };

        let fb = setup_feedback(&positions).expect("should have feedback");
        assert_eq!(fb.get(Square::E1), Some(SquareFeedback::Destination));
        assert_eq!(fb.squares().count(), 1);
    }

    #[test]
    fn missing_single_black_piece() {
        let expected = starting_positions();
        let positions = ByColor {
            white: expected.white,
            black: expected.black ^ Bitboard::from(Square::E8),
        };

        let fb = setup_feedback(&positions).expect("should have feedback");
        assert_eq!(fb.get(Square::E8), Some(SquareFeedback::Capture));
        assert_eq!(fb.squares().count(), 1);
    }

    #[test]
    fn missing_pieces_from_both_sides() {
        let expected = starting_positions();
        let positions = ByColor {
            white: expected.white ^ Bitboard::from(Square::A1),
            black: expected.black ^ Bitboard::from(Square::H8),
        };

        let fb = setup_feedback(&positions).expect("should have feedback");
        assert_eq!(fb.get(Square::A1), Some(SquareFeedback::Destination));
        assert_eq!(fb.get(Square::H8), Some(SquareFeedback::Capture));
        assert_eq!(fb.squares().count(), 2);
    }

    #[test]
    fn extra_pieces_on_board_are_ignored() {
        let expected = starting_positions();
        let positions = ByColor {
            white: expected.white | Bitboard::from(Square::E4),
            black: expected.black,
        };

        assert!(setup_feedback(&positions).is_none());
    }
}
