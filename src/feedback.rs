use shakmaty::{Move, Square};

/// Type of visual feedback for an individual square
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SquareFeedback {
    /// Place your piece here (legal destination or move completion)
    Destination,
    /// Placing here captures an opponent piece
    Capture,
    /// Lift this piece to move or capture (origin of move)
    Origin,
}

/// Contains the set of squares and their associated feedback types for the current board state.
///
/// `BoardFeedback` is computed by `compute_feedback()` and consumed by LED drivers or terminal
/// rendering to provide visual cues to the user. This struct represents the mapping from squares
/// to their feedback (e.g., highlight for move destinations, captures, or origins) and is the
/// primary interface between the game logic and the hardware/terminal feedback layer.
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
    #[inline]
    pub fn get(&self, square: Square) -> Option<SquareFeedback> {
        self.squares
            .iter()
            .find(|(sq, _)| *sq == square)
            .map(|(_, feedback)| *feedback)
    }

    /// Check if any feedback exists
    ///
    /// Returns true if there are no feedback squares to display.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.squares.is_empty()
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

    /// Get the square of our currently lifted piece
    fn lifted_piece(&self) -> Option<Square>;

    /// Get the square of the opponent's removed piece (for captures in progress)
    fn captured_piece(&self) -> Option<Square>;
}

/// Compute visual feedback based on current game state.
///
/// Shows move guidance based on what pieces are lifted or captured:
/// - Piece lifted: shows legal destinations
/// - Opponent piece removed: shows which pieces can capture there
/// - Both: shows where to complete the capture
pub fn compute_feedback(source: &impl FeedbackSource) -> BoardFeedback {
    let captured = source.captured_piece();
    let lifted = source.lifted_piece();

    match (captured, lifted) {
        // Our piece lifted, no captures in progress
        (None, Some(from)) => show_destinations_for(source.legal_moves(), from),

        // Opponent piece removed, our piece not lifted yet
        (Some(to), None) => show_capture_options(source.legal_moves(), to),

        // Opponent piece removed AND our piece lifted
        (Some(to), Some(from)) => show_capture_completion(source.legal_moves(), from, to),

        // Nothing happening
        (None, None) => BoardFeedback::default(),
    }
}

/// Show legal destinations when a piece is lifted
fn show_destinations_for(legal_moves: &[Move], from: Square) -> BoardFeedback {
    std::iter::once((from, SquareFeedback::Origin))
        .chain(
            legal_moves
                .iter()
                .filter(|mv| mv.from() == Some(from))
                .map(classify_move),
        )
        .collect::<Vec<_>>()
        .into()
}

/// Show which pieces can capture on the removed opponent piece's square
fn show_capture_options(legal_moves: &[Move], captured_sq: Square) -> BoardFeedback {
    legal_moves
        .iter()
        .filter(|mv| captures_square(mv, captured_sq))
        .flat_map(|mv| {
            // Emit both destination and origin for each capturing move
            std::iter::once((mv.to(), SquareFeedback::Destination))
                .chain(mv.from().map(|from| (from, SquareFeedback::Origin)))
        })
        // Deduplicate
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .into()
}

/// Show where to place your piece after opponent piece removed and your piece lifted
fn show_capture_completion(
    legal_moves: &[Move],
    from: Square,
    captured_sq: Square,
) -> BoardFeedback {
    std::iter::once((from, SquareFeedback::Origin))
        .chain(
            legal_moves
                .iter()
                .filter(|mv| mv.from() == Some(from) && captures_square(mv, captured_sq))
                .map(|mv| (mv.to(), SquareFeedback::Destination)),
        )
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

/// Tests if a move captures a piece on the given square
fn captures_square(mv: &Move, captured_sq: Square) -> bool {
    match mv {
        Move::Normal {
            capture: Some(_),
            to,
            ..
        } => captured_sq == *to,
        Move::EnPassant { from, to } => {
            // En passant captures the pawn on a different square than the destination
            Square::from_coords(to.file(), from.rank()) == captured_sq
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::{CastlingMode, Chess, Position, fen::Fen};

    struct MockFeedbackSource {
        moves: Vec<Move>,
        lifted: Option<Square>,
        captured: Option<Square>,
    }

    impl FeedbackSource for MockFeedbackSource {
        fn legal_moves(&self) -> &[Move] {
            &self.moves
        }

        fn lifted_piece(&self) -> Option<Square> {
            self.lifted
        }

        fn captured_piece(&self) -> Option<Square> {
            self.captured
        }
    }

    #[test]
    fn test_no_feedback_when_nothing_happening() {
        let pos = Chess::default();
        let source = MockFeedbackSource {
            moves: pos.legal_moves().into_iter().collect(),
            lifted: None,
            captured: None,
        };

        let feedback = compute_feedback(&source);
        assert_eq!(feedback.squares().len(), 0);
        assert!(feedback.is_empty());
    }

    #[test]
    fn test_show_destinations_when_piece_lifted() {
        let pos = Chess::default();
        let source = MockFeedbackSource {
            moves: pos.legal_moves().into_iter().collect(),
            lifted: Some(Square::E2),
            captured: None,
        };

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::E2), Some(SquareFeedback::Origin));
        assert_eq!(feedback.get(Square::E3), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::E4), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::D2), None);
    }

    #[test]
    fn test_show_capture_options_when_opponent_piece_removed() {
        // Position where e4 pawn can capture d5, and c3 knight can capture d5
        let fen: Fen = "rnbqkbnr/ppp1pppp/8/3p4/4P3/2N5/PPPP1PPP/R1BQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let pos: Chess = fen.into_position(CastlingMode::Standard).unwrap();

        let source = MockFeedbackSource {
            moves: pos.legal_moves().into_iter().collect(),
            lifted: None,
            captured: Some(Square::D5),
        };

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::D5), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(feedback.get(Square::C3), Some(SquareFeedback::Origin));
    }

    #[test]
    fn test_show_capture_options_when_en_passant() {
        let fen: Fen = "rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1"
            .parse()
            .unwrap();
        let pos: Chess = fen.into_position(CastlingMode::Standard).unwrap();

        let source = MockFeedbackSource {
            moves: pos.legal_moves().into_iter().collect(),
            lifted: None,
            captured: Some(Square::D5),
        };

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::D6), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::E5), Some(SquareFeedback::Origin));
    }

    #[test]
    fn test_show_capture_completion_when_en_passant() {
        let fen: Fen = "rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1"
            .parse()
            .unwrap();
        let pos: Chess = fen.into_position(CastlingMode::Standard).unwrap();

        let source = MockFeedbackSource {
            moves: pos.legal_moves().into_iter().collect(),
            lifted: Some(Square::E5),
            captured: Some(Square::D5),
        };

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::D6), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::E5), Some(SquareFeedback::Origin));
    }

    #[test]
    fn test_show_destination_when_both_removed_and_lifted() {
        let fen: Fen = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let pos: Chess = fen.into_position(CastlingMode::Standard).unwrap();

        let source = MockFeedbackSource {
            moves: pos.legal_moves().into_iter().collect(),
            lifted: Some(Square::E4),
            captured: Some(Square::D5),
        };

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(feedback.get(Square::D5), Some(SquareFeedback::Destination));
    }

    #[test]
    fn test_distinguish_captures() {
        let fen: Fen = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1"
            .parse()
            .unwrap();
        let pos: Chess = fen.into_position(CastlingMode::Standard).unwrap();

        let source = MockFeedbackSource {
            moves: pos.legal_moves().into_iter().collect(),
            lifted: Some(Square::E4),
            captured: None,
        };

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(feedback.get(Square::E5), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::D5), Some(SquareFeedback::Capture));
    }
}
