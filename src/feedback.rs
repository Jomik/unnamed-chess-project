use shakmaty::{Bitboard, CastlingSide, Color, File, Move, Rank, Square};

/// Check status information for feedback display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckInfo {
    /// Square of the king that is in check
    pub king_square: Square,
    /// Bitboard of pieces giving check (1-2 pieces)
    pub checkers: Bitboard,
}

/// A single physical action the player must perform to complete a move.
///
/// Used for multi-step move reconciliation (e.g. castling rook, or future
/// computer-opponent moves involving captures/en-passant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuidanceStep {
    /// Move a piece from one square to another (e.g. castling rook).
    Move { from: Square, to: Square },
    /// Remove a piece from the board (e.g. captured piece during computer move).
    Remove { square: Square },
}

/// Game-over state carrying the information needed for visual feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameOutcome {
    /// Checkmate: the side to move has no legal moves and is in check.
    Checkmate {
        /// Square of the checkmated king (loser)
        king_square: Square,
        /// Pieces delivering checkmate
        checkers: Bitboard,
        /// Color of the losing side
        loser: Color,
    },
    /// Stalemate: the side to move has no legal moves but is not in check.
    Stalemate {
        white_king: Square,
        black_king: Square,
    },
}

/// Type of visual feedback for an individual square
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SquareFeedback {
    /// Place your piece here (legal destination or move completion)
    Destination,
    /// Placing here captures an opponent piece
    Capture,
    /// Lift this piece to move or capture (origin of move)
    Origin,
    /// King in check
    Check,
    /// Piece attacking king
    Checker,
    /// Winning piece (delivered checkmate)
    Victory,
    /// King in stalemate (neither side wins)
    Stalemate,
}

/// Non-game status indication (e.g. WiFi connecting, success, failure).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Pending,
    Success,
    Failure,
}

/// Contains the set of squares and their associated feedback types for the current board state.
///
/// `BoardFeedback` is computed by `compute_feedback()` and consumed by LED drivers or terminal
/// rendering to provide visual cues to the user. This struct represents the mapping from squares
/// to their feedback (e.g., highlight for move destinations, captures, or origins) and is the
/// primary interface between the game logic and the hardware/terminal feedback layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardFeedback {
    squares: [Option<SquareFeedback>; 64],
    status: Option<StatusKind>,
}

impl BoardFeedback {
    /// Create empty feedback (no highlights)
    #[inline]
    pub const fn new() -> Self {
        Self {
            squares: [None; 64],
            status: None,
        }
    }

    pub const fn with_status(kind: StatusKind) -> Self {
        Self {
            squares: [None; 64],
            status: Some(kind),
        }
    }

    #[inline]
    pub fn status(&self) -> Option<StatusKind> {
        self.status
    }

    /// Get all square feedback entries as (Square, SquareFeedback) pairs
    #[inline]
    pub fn squares(&self) -> impl Iterator<Item = (Square, SquareFeedback)> + '_ {
        self.squares.iter().enumerate().filter_map(|(i, fb)| {
            fb.map(|f| {
                // SAFETY: i is always 0..63 from a fixed-size array
                (Square::new(i as u32), f)
            })
        })
    }

    /// Get feedback for a specific square, if any
    #[inline]
    pub fn get(&self, square: Square) -> Option<SquareFeedback> {
        self.squares[square as usize]
    }

    /// Set feedback for a specific square
    #[inline]
    pub fn set(&mut self, square: Square, feedback: SquareFeedback) {
        self.squares[square as usize] = Some(feedback);
    }

    /// Check if any feedback exists
    ///
    /// Returns true if there are no feedback squares or status to display.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.status.is_none() && self.squares.iter().all(|s| s.is_none())
    }

    /// Return a copy with the given status merged in (overwrites any existing status).
    pub fn with_merged_status(mut self, kind: StatusKind) -> Self {
        self.status = Some(kind);
        self
    }
}

impl Default for BoardFeedback {
    fn default() -> Self {
        Self::new()
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

    /// Get check information if the side to move is in check
    fn check_info(&self) -> Option<CheckInfo>;

    /// Guidance for the next physical action to complete a move in progress.
    fn move_guidance(&self) -> Option<GuidanceStep>;

    /// Game outcome, if the game has ended.
    fn outcome(&self) -> Option<GameOutcome>;
}

/// Compute visual feedback based on current game state.
///
/// Shows move guidance based on what pieces are lifted or captured:
/// - Piece lifted: shows legal destinations
/// - Opponent piece removed: shows which pieces can capture there
/// - Both: shows where to complete the capture
/// - Move guidance active: shows where to place a piece to complete a move
pub fn compute_feedback(source: &impl FeedbackSource) -> BoardFeedback {
    if let Some(outcome) = source.outcome() {
        return show_outcome_feedback(outcome);
    }

    if let Some(step) = source.move_guidance() {
        return show_guidance_step(step);
    }

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
        (None, None) => {
            if let Some(check_info) = source.check_info() {
                show_check_feedback(&check_info)
            } else {
                BoardFeedback::default()
            }
        }
    }
}

fn show_guidance_step(step: GuidanceStep) -> BoardFeedback {
    let mut fb = BoardFeedback::new();
    match step {
        GuidanceStep::Move { from, to } => {
            fb.set(from, SquareFeedback::Origin);
            fb.set(to, SquareFeedback::Destination);
        }
        GuidanceStep::Remove { square } => {
            fb.set(square, SquareFeedback::Capture);
        }
    }
    fb
}

/// Show game-over feedback based on the outcome.
fn show_outcome_feedback(outcome: GameOutcome) -> BoardFeedback {
    let mut fb = BoardFeedback::new();
    match outcome {
        GameOutcome::Checkmate {
            king_square,
            checkers,
            loser,
        } => {
            let loser_rank = back_rank(loser);
            let winner_rank = back_rank(loser.other());
            fill_rank(&mut fb, winner_rank, SquareFeedback::Victory);
            fill_rank(&mut fb, loser_rank, SquareFeedback::Check);
            fb.set(king_square, SquareFeedback::Check);
            for sq in checkers {
                fb.set(sq, SquareFeedback::Victory);
            }
        }
        GameOutcome::Stalemate {
            white_king,
            black_king,
        } => {
            fill_rank(&mut fb, Rank::First, SquareFeedback::Stalemate);
            fill_rank(&mut fb, Rank::Eighth, SquareFeedback::Stalemate);
            fb.set(white_king, SquareFeedback::Stalemate);
            fb.set(black_king, SquareFeedback::Stalemate);
        }
    }
    fb
}

fn back_rank(color: Color) -> Rank {
    match color {
        Color::White => Rank::First,
        Color::Black => Rank::Eighth,
    }
}

fn fill_rank(fb: &mut BoardFeedback, rank: Rank, feedback: SquareFeedback) {
    for file in File::ALL {
        fb.set(Square::from_coords(file, rank), feedback);
    }
}

/// Show legal destinations when a piece is lifted
fn show_destinations_for(legal_moves: &[Move], from: Square) -> BoardFeedback {
    let mut fb = BoardFeedback::new();
    fb.set(from, SquareFeedback::Origin);
    for mv in legal_moves.iter().filter(|mv| mv.from() == Some(from)) {
        let (sq, kind) = classify_move(mv);
        fb.set(sq, kind);
    }
    fb
}

/// Show which pieces can capture on the removed opponent piece's square
fn show_capture_options(legal_moves: &[Move], captured_sq: Square) -> BoardFeedback {
    let mut fb = BoardFeedback::new();
    for mv in legal_moves
        .iter()
        .filter(|mv| captures_square(mv, captured_sq))
    {
        fb.set(mv.to(), SquareFeedback::Destination);
        if let Some(from) = mv.from() {
            fb.set(from, SquareFeedback::Origin);
        }
    }
    fb
}

/// Show where to place your piece after opponent piece removed and your piece lifted
fn show_capture_completion(
    legal_moves: &[Move],
    from: Square,
    captured_sq: Square,
) -> BoardFeedback {
    let mut fb = BoardFeedback::new();
    fb.set(from, SquareFeedback::Origin);
    for mv in legal_moves
        .iter()
        .filter(|mv| mv.from() == Some(from) && captures_square(mv, captured_sq))
    {
        fb.set(mv.to(), SquareFeedback::Destination);
    }
    fb
}

/// Show check and checker squares when king is in check
fn show_check_feedback(check_info: &CheckInfo) -> BoardFeedback {
    let mut fb = BoardFeedback::new();
    fb.set(check_info.king_square, SquareFeedback::Check);
    for sq in check_info.checkers {
        fb.set(sq, SquareFeedback::Checker);
    }
    fb
}

/// Classify a move's target square and feedback type
fn classify_move(mv: &Move) -> (Square, SquareFeedback) {
    match mv {
        Move::Castle { king, rook } => {
            let side = CastlingSide::from_king_side(*king < *rook);
            let target = Square::from_coords(side.king_to_file(), king.rank());
            (target, SquareFeedback::Destination)
        }
        _ if mv.is_capture() => (mv.to(), SquareFeedback::Capture),
        _ => (mv.to(), SquareFeedback::Destination),
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
    use shakmaty::{CastlingMode, Chess, Position, Role, fen::Fen};

    struct MockFeedbackSource {
        moves: Vec<Move>,
        lifted: Option<Square>,
        captured: Option<Square>,
        check: Option<CheckInfo>,
        move_guidance: Option<GuidanceStep>,
        outcome: Option<GameOutcome>,
    }

    impl MockFeedbackSource {
        /// Create from starting chess position
        fn new() -> Self {
            Self::from_position(Chess::default())
        }

        /// Create from a FEN string
        fn from_fen(fen: &str) -> Self {
            let pos: Chess = fen
                .parse::<Fen>()
                .expect("invalid FEN")
                .into_position(CastlingMode::Standard)
                .expect("invalid position");
            Self::from_position(pos)
        }

        /// Create from an existing chess position
        fn from_position(pos: Chess) -> Self {
            let check = if pos.is_check() {
                Some(CheckInfo {
                    king_square: pos.our(Role::King).first().expect("king must exist"),
                    checkers: pos.checkers(),
                })
            } else {
                None
            };

            Self {
                moves: pos.legal_moves().into_iter().collect(),
                lifted: None,
                captured: None,
                check,
                move_guidance: None,
                outcome: None,
            }
        }

        /// Set the lifted piece square
        fn lifted(mut self, square: Square) -> Self {
            self.lifted = Some(square);
            self
        }

        /// Set the captured piece square
        fn captured(mut self, square: Square) -> Self {
            self.captured = Some(square);
            self
        }
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

        fn check_info(&self) -> Option<CheckInfo> {
            self.check
        }

        fn move_guidance(&self) -> Option<GuidanceStep> {
            self.move_guidance
        }

        fn outcome(&self) -> Option<GameOutcome> {
            self.outcome
        }
    }

    #[test]
    fn test_no_feedback_when_nothing_happening() {
        let source = MockFeedbackSource::new();

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.squares().count(), 0);
        assert!(feedback.is_empty());
    }

    #[test]
    fn test_with_status_returns_status() {
        let feedback = BoardFeedback::with_status(StatusKind::Pending);
        assert_eq!(feedback.status(), Some(StatusKind::Pending));
        assert_eq!(feedback.squares().count(), 0);

        let feedback = BoardFeedback::with_status(StatusKind::Success);
        assert_eq!(feedback.status(), Some(StatusKind::Success));

        let feedback = BoardFeedback::with_status(StatusKind::Failure);
        assert_eq!(feedback.status(), Some(StatusKind::Failure));
    }

    #[test]
    fn test_status_feedback_is_not_empty() {
        let feedback = BoardFeedback::with_status(StatusKind::Pending);
        assert!(
            !feedback.is_empty(),
            "status-only feedback should not be empty"
        );
    }

    #[test]
    fn test_default_feedback_has_no_status() {
        let feedback = BoardFeedback::default();
        assert_eq!(feedback.status(), None);
        assert!(feedback.is_empty());
    }

    #[test]
    fn test_show_destinations_when_piece_lifted() {
        let source = MockFeedbackSource::new().lifted(Square::E2);

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::E2), Some(SquareFeedback::Origin));
        assert_eq!(feedback.get(Square::E3), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::E4), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::D2), None);
    }

    #[test]
    fn test_show_capture_options_when_opponent_piece_removed() {
        // Position where e4 pawn can capture d5, and c3 knight can capture d5
        let source = MockFeedbackSource::from_fen(
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/2N5/PPPP1PPP/R1BQKBNR w KQkq - 0 1",
        )
        .captured(Square::D5);

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::D5), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(feedback.get(Square::C3), Some(SquareFeedback::Origin));
    }

    #[test]
    fn test_show_capture_options_when_en_passant() {
        let source = MockFeedbackSource::from_fen(
            "rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1",
        )
        .captured(Square::D5);

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::D6), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::E5), Some(SquareFeedback::Origin));
    }

    #[test]
    fn test_show_capture_completion_when_en_passant() {
        let source = MockFeedbackSource::from_fen(
            "rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1",
        )
        .lifted(Square::E5)
        .captured(Square::D5);

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::D6), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::E5), Some(SquareFeedback::Origin));
    }

    #[test]
    fn test_show_destination_when_both_removed_and_lifted() {
        let source = MockFeedbackSource::from_fen(
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1",
        )
        .lifted(Square::E4)
        .captured(Square::D5);

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(feedback.get(Square::D5), Some(SquareFeedback::Destination));
    }

    #[test]
    fn test_distinguish_captures() {
        let source = MockFeedbackSource::from_fen(
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1",
        )
        .lifted(Square::E4);

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(feedback.get(Square::E5), Some(SquareFeedback::Destination));
        assert_eq!(feedback.get(Square::D5), Some(SquareFeedback::Capture));
    }

    #[test]
    fn test_check_feedback_shown_when_idle() {
        // Black king in check from white queen on h5
        let source = MockFeedbackSource::from_fen(
            "rnbqkbnr/pppp2pp/8/4pp1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 0 1",
        );

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::E8), Some(SquareFeedback::Check));
        assert_eq!(feedback.get(Square::H5), Some(SquareFeedback::Checker));
    }

    #[test]
    fn test_check_feedback_not_shown_when_piece_lifted() {
        // Black king in check, but black is lifting a piece to block
        let source = MockFeedbackSource::from_fen(
            "rnbqkbnr/pppp2pp/8/4pp1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 0 1",
        )
        .lifted(Square::G8); // Lifting knight to potentially block

        let feedback = compute_feedback(&source);

        // Should show destinations, not check feedback
        assert_eq!(feedback.get(Square::G8), Some(SquareFeedback::Origin));
        assert_eq!(feedback.get(Square::E8), None); // No check highlight
        assert_eq!(feedback.get(Square::H5), None); // No checker highlight
    }

    #[test]
    fn test_double_check_feedback() {
        // Double check: black king attacked by both rook and bishop
        let source = MockFeedbackSource::from_fen("4k3/8/8/7B/8/8/8/4R2K b - - 0 1");

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::E8), Some(SquareFeedback::Check));
        assert_eq!(feedback.get(Square::E1), Some(SquareFeedback::Checker)); // Rook
        assert_eq!(feedback.get(Square::H5), Some(SquareFeedback::Checker)); // Bishop
    }

    #[test]
    fn test_checkmate_feedback() {
        let mut source = MockFeedbackSource::new();
        source.outcome = Some(GameOutcome::Checkmate {
            king_square: Square::E8,
            checkers: Bitboard::from(Square::F7),
            loser: Color::Black,
        });

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::E8), Some(SquareFeedback::Check));
        assert_eq!(feedback.get(Square::F7), Some(SquareFeedback::Victory));
        // Back rank fill: white rank 1 = Victory, black rank 8 = Check
        assert_eq!(feedback.get(Square::A1), Some(SquareFeedback::Victory));
        assert_eq!(feedback.get(Square::H1), Some(SquareFeedback::Victory));
        assert_eq!(feedback.get(Square::A8), Some(SquareFeedback::Check));
        // E8 already has Check from king, so just verify it's still Check
        assert_eq!(feedback.get(Square::D8), Some(SquareFeedback::Check));
        assert_eq!(feedback.squares().count(), 17);
    }

    #[test]
    fn test_stalemate_feedback() {
        let mut source = MockFeedbackSource::new();
        source.outcome = Some(GameOutcome::Stalemate {
            white_king: Square::C6,
            black_king: Square::A8,
        });

        let feedback = compute_feedback(&source);

        assert_eq!(feedback.get(Square::C6), Some(SquareFeedback::Stalemate));
        assert_eq!(feedback.get(Square::A8), Some(SquareFeedback::Stalemate));
        assert_eq!(feedback.get(Square::A1), Some(SquareFeedback::Stalemate));
        assert_eq!(feedback.get(Square::H1), Some(SquareFeedback::Stalemate));
        assert_eq!(feedback.get(Square::H8), Some(SquareFeedback::Stalemate));
        // 8 (rank 1) + 8 (rank 8) + 1 (C6 king, off back rank) = 17
        assert_eq!(feedback.squares().count(), 17);
    }

    #[test]
    fn test_outcome_takes_priority_over_check() {
        let mut source = MockFeedbackSource::from_fen(
            "rnbqkbnr/pppp2pp/8/4pp1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 0 1",
        );
        source.outcome = Some(GameOutcome::Checkmate {
            king_square: Square::E8,
            checkers: Bitboard::from(Square::H5),
            loser: Color::Black,
        });
        source.lifted = Some(Square::G8);

        let feedback = compute_feedback(&source);

        assert_eq!(
            feedback.get(Square::E8),
            Some(SquareFeedback::Check),
            "outcome feedback, not move destinations"
        );
        assert_eq!(feedback.get(Square::H5), Some(SquareFeedback::Victory));
        assert_eq!(
            feedback.get(Square::G8),
            Some(SquareFeedback::Check),
            "lifted piece should get rank fill, not Origin"
        );
    }
}
