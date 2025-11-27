use shakmaty::{Move, Square};

/// Type of visual feedback for an individual square
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SquareFeedback {
    /// Piece can move to this square (not a capture)
    Destination,
    /// Piece can capture a piece on this square
    Capture,
}

/// Visual feedback state for the chess board
///
/// Contains squares and their associated feedback types.
/// Computed by `compute_feedback()` and consumed by LED drivers or terminal rendering.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoardFeedback {
    squares: Vec<(Square, SquareFeedback)>,
}

impl BoardFeedback {
    /// Create empty feedback (no highlights)
    #[inline]
    pub const fn new() -> Self {
        Self {
            squares: Vec::new(),
        }
    }

    /// Get all square feedback entries
    #[inline]
    pub fn squares(&self) -> &[(Square, SquareFeedback)] {
        &self.squares
    }

    /// Get feedback for a specific square, if any
    pub fn get(&self, square: Square) -> Option<SquareFeedback> {
        self.squares
            .iter()
            .find(|(sq, _)| *sq == square)
            .map(|(_, feedback)| *feedback)
    }
}

impl From<Vec<(Square, SquareFeedback)>> for BoardFeedback {
    fn from(squares: Vec<(Square, SquareFeedback)>) -> Self {
        Self { squares }
    }
}

/// Game state information needed to compute feedback
///
/// This trait defines what the feedback system needs from the game engine.
pub trait FeedbackSource {
    /// Get all legal moves in the current position
    fn legal_moves(&self) -> &[Move];

    /// Get the square of the currently lifted piece, if any
    fn lifted_piece(&self) -> Option<Square>;
}

/// Compute visual feedback based on current game state
///
/// Shows legal destinations when a piece is lifted, otherwise shows nothing.
pub fn compute_feedback(source: &impl FeedbackSource) -> BoardFeedback {
    source
        .lifted_piece()
        .map(|from| show_destinations_for(source.legal_moves(), from))
        .unwrap_or_default()
}

/// Show legal destinations for a specific piece
fn show_destinations_for(legal_moves: &[Move], from: Square) -> BoardFeedback {
    legal_moves
        .iter()
        .filter(|mv| mv.from() == Some(from))
        .map(classify_move)
        .collect::<Vec<_>>()
        .into()
}

/// Classify a move as either a capture or regular destination
fn classify_move(mv: &Move) -> (Square, SquareFeedback) {
    if mv.is_capture() {
        (mv.to(), SquareFeedback::Capture)
    } else {
        (mv.to(), SquareFeedback::Destination)
    }
}

#[cfg(test)]
mod tests {
    use shakmaty::{CastlingMode, Chess, Position, fen::Fen};

    use super::*;
    struct MockFeedbackSource {
        moves: Vec<Move>,
        lifted: Option<Square>,
    }

    impl FeedbackSource for MockFeedbackSource {
        fn legal_moves(&self) -> &[Move] {
            &self.moves
        }

        fn lifted_piece(&self) -> Option<Square> {
            self.lifted
        }
    }

    #[test]
    fn test_no_feedback_when_no_piece_lifted() {
        let pos = Chess::default();
        let source = MockFeedbackSource {
            moves: pos.legal_moves().into_iter().collect(),
            lifted: None,
        };

        let feedback = compute_feedback(&source);
        assert_eq!(feedback.squares().len(), 0);
    }

    #[test]
    fn test_show_pawn_destinations() {
        let pos = Chess::default();
        let source = MockFeedbackSource {
            moves: pos.legal_moves().into_iter().collect(),
            lifted: Some(Square::E2),
        };

        let feedback = compute_feedback(&source);

        // e2 pawn can move to e3 and e4
        assert_eq!(feedback.squares().len(), 2);
        assert_eq!(feedback.get(Square::E3), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::E4), Some(SquareFeedback::Destination));
    }

    #[test]
    fn test_distinguish_captures() {
        // Position where e4 pawn can capture on d5 and f5
        let fen: Fen = "rnbqkbnr/ppp1p1pp/8/3p1p2/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let pos: Chess = fen.into_position(CastlingMode::Standard).unwrap();

        let source = MockFeedbackSource {
            moves: pos.legal_moves().into_iter().collect(),
            lifted: Some(Square::E4),
        };

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::D5), Some(SquareFeedback::Capture));
        assert_eq!(feedback.get(Square::E5), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::F5), Some(SquareFeedback::Capture));
    }
}
