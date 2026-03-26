use shakmaty::{
    Bitboard, ByColor, CastlingSide, Chess, Color, File, Move, MoveList, Position, Rank, Role,
    Square,
};

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
/// rendering to provide visual cues to the user.
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

/// Game-over state carrying the information needed for visual feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameOutcome {
    /// Checkmate: the side to move has no legal moves and is in check.
    Checkmate {
        king_square: Square,
        checkers: Bitboard,
        loser: Color,
    },
    /// Stalemate: the side to move has no legal moves but is not in check.
    Stalemate {
        white_king: Square,
        black_king: Square,
    },
}

/// Compute visual feedback from chess position and sensor state.
///
/// Pure function — derives lifted piece, captured piece, check, outcome,
/// recovery, and castle guidance entirely from the inputs.
///
/// `reference_sensors` is the last known good board state (used to suppress guidance
/// for pieces that were never physically present in this interaction cycle).
pub fn compute_feedback(
    position: &Chess,
    curr_sensors: ByColor<Bitboard>,
    reference_sensors: ByColor<Bitboard>,
) -> BoardFeedback {
    let curr_combined = curr_sensors.white | curr_sensors.black;
    let ref_combined = reference_sensors.white | reference_sensors.black;

    let turn = position.turn();
    let expected_board = position.board();

    // Derive lifted and captured from expected board vs current sensors,
    // filtered by reference (only pieces physically present can be lifted/captured)
    let lifted = expected_board.by_color(turn) & !curr_combined & ref_combined;
    let captured = expected_board.by_color(turn.other()) & !curr_combined & ref_combined;

    // In-recovery suppression: check for divergence beyond lifted/captured
    let occupancy_diff = (expected_board.occupied() ^ curr_combined) & !lifted & !captured;
    let wrong_color = (expected_board.by_color(Color::White) & curr_sensors.black)
        | (expected_board.by_color(Color::Black) & curr_sensors.white);
    let in_recovery = !occupancy_diff.is_empty() || !wrong_color.is_empty();

    let legal_moves = position.legal_moves();

    // Game outcome
    let outcome = compute_outcome(position, &legal_moves);

    // Outcome takes priority over everything
    if let Some(outcome) = outcome {
        return show_outcome_feedback(outcome);
    }

    // Castle rook guidance: detect mid-castle where king is placed but rook hasn't moved.
    // Check BEFORE recovery — mid-castle looks like recovery (king not on expected square).
    if let Some(fb) = detect_castle_guidance(position, &curr_sensors, &legal_moves) {
        return fb;
    }

    // Recovery: board diverges from expected position
    if in_recovery {
        return recovery_feedback(expected_board, &curr_sensors);
    }

    let lifted_sq = resolve_lifted_piece(&legal_moves, lifted);
    let captured_sq = captured.single_square();

    match (captured_sq, lifted_sq) {
        (None, Some(from)) => show_destinations_for(&legal_moves, from),
        (Some(to), None) => show_capture_options(&legal_moves, to),
        (Some(to), Some(from)) => show_capture_completion(&legal_moves, from, to),
        (None, None) => {
            if !position.checkers().is_empty() {
                show_check_feedback(position)
            } else {
                BoardFeedback::default()
            }
        }
    }
}

fn compute_outcome(position: &Chess, legal_moves: &MoveList) -> Option<GameOutcome> {
    if !legal_moves.is_empty() {
        return None;
    }

    if position.is_check() {
        let king_square = position.our(Role::King).first().expect("king must exist");
        Some(GameOutcome::Checkmate {
            king_square,
            checkers: position.checkers(),
            loser: position.turn(),
        })
    } else {
        let white_king = position
            .board()
            .by_role(Role::King)
            .intersect(position.board().by_color(Color::White))
            .first()
            .expect("white king must exist");
        let black_king = position
            .board()
            .by_role(Role::King)
            .intersect(position.board().by_color(Color::Black))
            .first()
            .expect("black king must exist");
        Some(GameOutcome::Stalemate {
            white_king,
            black_king,
        })
    }
}

fn recovery_feedback(
    expected_board: &shakmaty::Board,
    curr_sensors: &ByColor<Bitboard>,
) -> BoardFeedback {
    let expected_all = expected_board.occupied();
    let current_all = curr_sensors.white | curr_sensors.black;

    let missing = expected_all & !current_all;
    let extra = current_all & !expected_all;
    let wrong_color = (expected_board.by_color(Color::White) & curr_sensors.black)
        | (expected_board.by_color(Color::Black) & curr_sensors.white);

    let mut fb = BoardFeedback::new();
    for sq in missing {
        fb.set(sq, SquareFeedback::Destination);
    }
    for sq in extra {
        fb.set(sq, SquareFeedback::Capture);
    }
    for sq in wrong_color {
        fb.set(sq, SquareFeedback::Capture);
    }
    fb
}

fn detect_castle_guidance(
    position: &Chess,
    curr_sensors: &ByColor<Bitboard>,
    legal_moves: &MoveList,
) -> Option<BoardFeedback> {
    let turn = position.turn();
    let expected_our = position.board().by_color(turn);
    let our_current = curr_sensors[turn];
    let newly_placed = our_current & !expected_our;

    for mv in legal_moves {
        if let Move::Castle { king, rook } = *mv {
            let side = CastlingSide::from_king_side(king < rook);
            let king_target = side.king_to(turn);
            if newly_placed.contains(king_target) {
                let rook_target = side.rook_to(turn);
                let mut fb = BoardFeedback::new();
                fb.set(rook, SquareFeedback::Origin);
                fb.set(rook_target, SquareFeedback::Destination);
                return Some(fb);
            }
        }
    }
    None
}

fn resolve_lifted_piece(legal_moves: &MoveList, lifted: Bitboard) -> Option<Square> {
    lifted.single_square().or_else(|| {
        if lifted.count() != 2 {
            return None;
        }
        legal_moves
            .iter()
            .find(|mv| {
                matches!(mv, Move::Castle { king, rook } if lifted.contains(*king) && lifted.contains(*rook))
            })
            .and_then(|mv| mv.from())
    })
}

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

fn show_destinations_for(legal_moves: &[Move], from: Square) -> BoardFeedback {
    let mut fb = BoardFeedback::new();
    fb.set(from, SquareFeedback::Origin);
    for mv in legal_moves.iter().filter(|mv| mv.from() == Some(from)) {
        let (sq, kind) = classify_move(mv);
        fb.set(sq, kind);
    }
    fb
}

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

fn show_check_feedback(position: &Chess) -> BoardFeedback {
    let mut fb = BoardFeedback::new();
    let king_square = position.our(Role::King).first().expect("king must exist");
    fb.set(king_square, SquareFeedback::Check);
    for sq in position.checkers() {
        fb.set(sq, SquareFeedback::Checker);
    }
    fb
}

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

fn captures_square(mv: &Move, captured_sq: Square) -> bool {
    match mv {
        Move::Normal {
            capture: Some(_),
            to,
            ..
        } => captured_sq == *to,
        Move::EnPassant { from, to } => Square::from_coords(to.file(), from.rank()) == captured_sq,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::{CastlingMode, fen::Fen};

    fn position_from_fen(fen: &str) -> Chess {
        fen.parse::<Fen>()
            .expect("invalid FEN")
            .into_position(CastlingMode::Standard)
            .expect("invalid position")
    }

    fn starting_sensors() -> ByColor<Bitboard> {
        let chess = Chess::default();
        let board = chess.board();
        ByColor {
            white: board.by_color(Color::White),
            black: board.by_color(Color::Black),
        }
    }

    fn sensors_from_position(position: &Chess) -> ByColor<Bitboard> {
        let board = position.board();
        ByColor {
            white: board.by_color(Color::White),
            black: board.by_color(Color::Black),
        }
    }

    // --- BoardFeedback struct tests ---

    #[test]
    fn no_feedback_when_nothing_happening() {
        let position = Chess::default();
        let sensors = starting_sensors();

        let fb = compute_feedback(&position, sensors, sensors);

        assert!(fb.is_empty());
    }

    #[test]
    fn with_status_returns_status() {
        let feedback = BoardFeedback::with_status(StatusKind::Pending);
        assert_eq!(feedback.status(), Some(StatusKind::Pending));
        assert_eq!(feedback.squares().count(), 0);
    }

    #[test]
    fn status_feedback_is_not_empty() {
        let feedback = BoardFeedback::with_status(StatusKind::Pending);
        assert!(!feedback.is_empty());
    }

    #[test]
    fn default_feedback_has_no_status() {
        let feedback = BoardFeedback::default();
        assert_eq!(feedback.status(), None);
        assert!(feedback.is_empty());
    }

    // --- Lifted piece feedback ---

    #[test]
    fn lifted_piece_shows_destinations() {
        let position = Chess::default();
        let prev = starting_sensors();
        let mut curr = prev;
        curr.white.toggle(Square::E2);

        let fb = compute_feedback(&position, curr, sensors_from_position(&position));

        assert_eq!(fb.get(Square::E2), Some(SquareFeedback::Origin));
        assert_eq!(fb.get(Square::E3), Some(SquareFeedback::Destination));
        assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Destination));
    }

    #[test]
    fn distinguish_captures_from_destinations() {
        // White pawn on e4 can move to e5 (destination) or capture on d5 (capture)
        let position =
            position_from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1");
        let prev = sensors_from_position(&position);
        let mut curr = prev;
        curr.white.toggle(Square::E4); // lift e4

        let fb = compute_feedback(&position, curr, sensors_from_position(&position));

        assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(fb.get(Square::E5), Some(SquareFeedback::Destination));
        assert_eq!(fb.get(Square::D5), Some(SquareFeedback::Capture));
    }

    // --- Captured piece feedback ---

    #[test]
    fn captured_piece_shows_origins() {
        // e4 pawn and c3 knight can both capture on d5
        let position =
            position_from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/2N5/PPPP1PPP/R1BQKBNR w KQkq - 0 1");
        let prev = sensors_from_position(&position);
        let mut curr = prev;
        curr.black.toggle(Square::D5); // remove opponent pawn

        let fb = compute_feedback(&position, curr, sensors_from_position(&position));

        assert_eq!(fb.get(Square::D5), Some(SquareFeedback::Destination));
        assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(fb.get(Square::C3), Some(SquareFeedback::Origin));
    }

    #[test]
    fn both_lifted_and_captured_shows_completion() {
        // White pawn on e4, black pawn on d5 — capture in progress
        let position =
            position_from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1");
        let prev = sensors_from_position(&position);
        let mut curr = prev;
        curr.white.toggle(Square::E4); // lift our pawn
        curr.black.toggle(Square::D5); // remove opponent pawn

        let fb = compute_feedback(&position, curr, sensors_from_position(&position));

        assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Origin));
        assert_eq!(fb.get(Square::D5), Some(SquareFeedback::Destination));
    }

    // --- En passant ---

    #[test]
    fn en_passant_capture_feedback() {
        // White pawn on e5, black just played d7-d5 — en passant available
        let position =
            position_from_fen("rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");
        let prev = sensors_from_position(&position);
        let mut curr = prev;
        curr.black.toggle(Square::D5); // remove en passant pawn

        let fb = compute_feedback(&position, curr, sensors_from_position(&position));

        assert_eq!(fb.get(Square::E5), Some(SquareFeedback::Origin));
        assert_eq!(fb.get(Square::D6), Some(SquareFeedback::Destination));
    }

    // --- Check feedback ---

    #[test]
    fn check_feedback_shown_when_idle() {
        // Black king in check from white queen on h5
        let position =
            position_from_fen("rnbqkbnr/pppp2pp/8/4pp1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 0 1");
        let sensors = sensors_from_position(&position);

        let fb = compute_feedback(&position, sensors, sensors);

        assert_eq!(fb.get(Square::E8), Some(SquareFeedback::Check));
        assert_eq!(fb.get(Square::H5), Some(SquareFeedback::Checker));
    }

    #[test]
    fn check_feedback_not_shown_when_piece_lifted() {
        // Black king in check, lifting g8 knight to block
        let position =
            position_from_fen("rnbqkbnr/pppp2pp/8/4pp1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 0 1");
        let prev = sensors_from_position(&position);
        let mut curr = prev;
        curr.black.toggle(Square::G8); // lift knight

        let fb = compute_feedback(&position, curr, sensors_from_position(&position));

        assert_eq!(fb.get(Square::G8), Some(SquareFeedback::Origin));
        assert_eq!(fb.get(Square::E8), None);
        assert_eq!(fb.get(Square::H5), None);
    }

    #[test]
    fn recovery_suppresses_check_feedback() {
        // Black king in check from white queen on h5 — but board also has recovery divergence
        let position =
            position_from_fen("rnbqkbnr/pppp2pp/8/4pp1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 0 1");
        let sensors = sensors_from_position(&position);
        // Create divergence: add an extra piece on an empty square to trigger recovery mode.
        // An extra piece that doesn't belong anywhere creates occupancy_diff > 0.
        let mut diverged = sensors;
        diverged.black.toggle(Square::A4); // phantom piece on empty square — triggers recovery

        let fb = compute_feedback(&position, diverged, diverged);

        // Should show recovery (extra piece), NOT check feedback
        assert_eq!(
            fb.get(Square::A4),
            Some(SquareFeedback::Capture),
            "extra piece should show Capture"
        );
        assert_eq!(
            fb.get(Square::E8),
            None,
            "check should be suppressed during recovery"
        );
        assert_eq!(
            fb.get(Square::H5),
            None,
            "checker should be suppressed during recovery"
        );
    }

    #[test]
    fn double_check_feedback() {
        // Black king attacked by both rook on e1 and bishop on h5
        let position = position_from_fen("4k3/8/8/7B/8/8/8/4R2K b - - 0 1");
        let sensors = sensors_from_position(&position);

        let fb = compute_feedback(&position, sensors, sensors);

        assert_eq!(fb.get(Square::E8), Some(SquareFeedback::Check));
        assert_eq!(fb.get(Square::E1), Some(SquareFeedback::Checker));
        assert_eq!(fb.get(Square::H5), Some(SquareFeedback::Checker));
    }

    // --- Game outcome ---

    #[test]
    fn checkmate_feedback() {
        // Scholar's mate — Qxf7# checkmates the black king
        let position =
            position_from_fen("rnbqkb1r/pppp1Qpp/5n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4");
        let sensors = sensors_from_position(&position);

        let fb = compute_feedback(&position, sensors, sensors);

        assert_eq!(fb.get(Square::E8), Some(SquareFeedback::Check));
        assert_eq!(fb.get(Square::F7), Some(SquareFeedback::Victory));
        assert_eq!(fb.get(Square::A1), Some(SquareFeedback::Victory));
        assert_eq!(fb.get(Square::A8), Some(SquareFeedback::Check));
    }

    #[test]
    fn stalemate_feedback() {
        // Kc6 + Qb6 vs Ka8 — black has no legal moves, not in check
        let position = position_from_fen("k7/8/1QK5/8/8/8/8/8 b - - 0 1");
        let sensors = sensors_from_position(&position);

        let fb = compute_feedback(&position, sensors, sensors);

        assert_eq!(fb.get(Square::A8), Some(SquareFeedback::Stalemate));
        assert_eq!(fb.get(Square::C6), Some(SquareFeedback::Stalemate));
        assert_eq!(fb.get(Square::A1), Some(SquareFeedback::Stalemate));
        assert_eq!(fb.get(Square::H8), Some(SquareFeedback::Stalemate));
    }

    // --- Recovery ---

    #[test]
    fn recovery_shows_missing_and_extra() {
        // After opponent's e7→e5 applied logically, physical board still has pawn on e7
        let position =
            position_from_fen("rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2");
        let stale = ByColor {
            white: position.board().by_color(Color::White),
            // Physical board has pawn on e7, not e5
            black: (position.board().by_color(Color::Black) ^ Bitboard::from(Square::E5))
                | Bitboard::from(Square::E7),
        };

        let fb = compute_feedback(&position, stale, stale);

        assert_eq!(fb.get(Square::E7), Some(SquareFeedback::Capture));
        assert_eq!(fb.get(Square::E5), Some(SquareFeedback::Destination));
    }

    #[test]
    fn recovery_clears_when_board_matches() {
        let position = Chess::default();
        let sensors = starting_sensors();

        let fb = compute_feedback(&position, sensors, sensors);

        assert!(fb.is_empty());
    }

    // --- Castle guidance ---

    #[test]
    fn mid_castle_shows_rook_guidance() {
        // White can castle kingside — king placed on g1 but rook still on h1
        let position =
            position_from_fen("rnbqkbnr/pppppppp/8/8/8/5NP1/PPPPPPBP/RNBQK2R w KQkq - 0 1");
        let prev = sensors_from_position(&position);
        // King placed on g1 (castle target), rook still on h1
        let mut curr = prev;
        curr.white.toggle(Square::E1); // king left e1
        curr.white.toggle(Square::G1); // king placed on g1

        let fb = compute_feedback(&position, curr, sensors_from_position(&position));

        assert_eq!(fb.get(Square::H1), Some(SquareFeedback::Origin));
        assert_eq!(fb.get(Square::F1), Some(SquareFeedback::Destination));
    }
}
