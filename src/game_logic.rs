use crate::feedback::{CheckInfo, FeedbackSource, GameOutcome, GuidanceStep};
use shakmaty::{
    Bitboard, ByColor, CastlingSide, Chess, Color, EnPassantMode, Move, MoveList, Piece, Position,
    Role, Square, fen::Fen,
};

#[derive(Debug, thiserror::Error)]
pub enum OpponentMoveError {
    #[error("opponent move is not legal in the current position")]
    IllegalMove,
}

/// Current game state snapshot for feedback and display
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState {
    legal_moves: MoveList,
    lifted_piece: Option<Square>,
    captured_piece: Option<Square>,
    king_square: Square,
    checkers: Bitboard,
    /// Guidance for the next physical action to complete a move in progress.
    /// Set when a castle move has been executed but the rook hasn't
    /// physically moved to its target square yet.
    move_guidance: Option<GuidanceStep>,
    outcome: Option<GameOutcome>,
    /// The human move committed during this tick, if any.
    human_move: Option<Move>,
}

impl FeedbackSource for GameState {
    fn legal_moves(&self) -> &[Move] {
        &self.legal_moves
    }

    fn lifted_piece(&self) -> Option<Square> {
        self.lifted_piece
    }

    fn captured_piece(&self) -> Option<Square> {
        self.captured_piece
    }

    fn check_info(&self) -> Option<CheckInfo> {
        if self.checkers.is_empty() {
            None
        } else {
            Some(CheckInfo {
                king_square: self.king_square,
                checkers: self.checkers,
            })
        }
    }

    fn move_guidance(&self) -> Option<GuidanceStep> {
        self.move_guidance
    }

    fn outcome(&self) -> Option<GameOutcome> {
        self.outcome
    }
}

impl GameState {
    pub fn human_move(&self) -> Option<&Move> {
        self.human_move.as_ref()
    }
}

/// Core game engine that processes sensor input and maintains game state
impl Default for GameEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub struct GameEngine {
    /// The logical chess position (piece types, turn, castling rights, etc.)
    position: Chess,

    /// Last known physical board state from sensors, per color.
    last_positions: ByColor<Bitboard>,

    /// Which color the human plays. When set, the engine only detects
    /// moves for this color — the opponent's moves are applied via
    /// `apply_opponent_move` instead of board detection.
    human_color: Option<Color>,
}

impl GameEngine {
    #[inline]
    pub fn new() -> Self {
        Self::from_position(Chess::default())
    }

    /// Creates a GameEngine from an existing chess position.
    pub fn from_position(position: Chess) -> Self {
        let board = position.board();
        let last_positions = ByColor {
            white: board.by_color(Color::White),
            black: board.by_color(Color::Black),
        };
        Self {
            position,
            last_positions,
            human_color: None,
        }
    }

    /// Set which color the human plays. When set, the engine only detects
    /// moves for this color — opponent piece movements on the board are ignored.
    pub fn set_human_color(&mut self, color: Color) {
        self.human_color = Some(color);
    }

    /// Get the piece at a given square, if any
    #[inline]
    pub fn piece_at(&self, square: Square) -> Option<Piece> {
        self.position.board().piece_at(square)
    }

    #[inline]
    pub fn expected_positions(&self) -> ByColor<Bitboard> {
        let board = self.position.board();
        ByColor {
            white: board.by_color(Color::White),
            black: board.by_color(Color::Black),
        }
    }

    #[inline]
    pub fn position(&self) -> &Chess {
        &self.position
    }

    #[inline]
    pub fn turn(&self) -> Color {
        self.position.turn()
    }

    /// Apply a computer opponent's move, advancing the logical position.
    pub fn apply_opponent_move(&mut self, mv: &Move) -> Result<(), OpponentMoveError> {
        if !self.position.legal_moves().contains(mv) {
            return Err(OpponentMoveError::IllegalMove);
        }

        self.position.play_unchecked(*mv);
        Ok(())
    }

    /// Process a board state reading
    ///
    /// Tracks changes in piece positions and executes legal moves when pieces are placed.
    pub fn tick(&mut self, current: ByColor<Bitboard>) -> GameState {
        let prev_combined = self.last_positions.white | self.last_positions.black;
        let current_combined = current.white | current.black;

        let played = self.process_moves(current);

        let lifted = self.position.us() & !current_combined;
        // Only report captured for pieces the human physically removed.
        // After apply_opponent_move(), the logical position may expect
        // pieces on squares that were never physically occupied.
        let captured = self.position.them() & !current_combined & prev_combined;

        // Suppress lifted/captured when the board has divergence beyond
        // those squares — indicates recovery, not a human move in progress.
        let expected_white = self.position.board().by_color(Color::White);
        let expected_black = self.position.board().by_color(Color::Black);
        let occupancy_diff =
            (self.position.board().occupied() ^ current_combined) & !lifted & !captured;
        let wrong_color = (expected_white & current.black) | (expected_black & current.white);
        let in_recovery = !occupancy_diff.is_empty() || !wrong_color.is_empty();

        let legal_moves = self.position.legal_moves();

        let move_guidance = played
            .and_then(|mv| self.castle_rook_guidance(&mv, current_combined))
            .or_else(|| self.detect_mid_castle(current, &legal_moves));

        let outcome = Self::compute_outcome(&self.position, &legal_moves);

        let king_square = self
            .position
            .our(Role::King)
            .first()
            .expect("king must exist");

        // During recovery no human move is in progress, so
        // lifted piece, captures, and check are not meaningful.
        if in_recovery {
            GameState {
                lifted_piece: None,
                captured_piece: None,
                checkers: Bitboard::EMPTY,
                king_square,
                move_guidance,
                outcome,
                legal_moves,
                human_move: played,
            }
        } else {
            GameState {
                lifted_piece: Self::resolve_lifted_piece(&legal_moves, lifted),
                captured_piece: captured.single_square(),
                checkers: self.position.checkers(),
                king_square,
                move_guidance,
                outcome,
                legal_moves,
                human_move: played,
            }
        }
    }

    /// Resolve which square to report as the lifted piece.
    ///
    /// Single lifted piece → that square. Two lifted pieces that match
    /// a castling move (king + rook) → the king's origin, so feedback
    /// can show castle destinations. Everything else → None.
    fn resolve_lifted_piece(legal_moves: &MoveList, lifted: Bitboard) -> Option<Square> {
        lifted.single_square().or_else(|| {
            if lifted.count() != 2 {
                return None;
            }
            legal_moves
                .iter()
                .find(|mv| matches!(mv, Move::Castle { king, rook } if lifted.contains(*king) && lifted.contains(*rook)))
                .and_then(|mv| mv.from())
        })
    }

    /// After a castle move is played, check if the rook still needs to
    /// physically move to its target square.
    fn castle_rook_guidance(&self, mv: &Move, physical: Bitboard) -> Option<GuidanceStep> {
        match mv {
            Move::Castle { king, rook } => {
                let side = CastlingSide::from_king_side(*king < *rook);
                let color = self.position.turn().other();
                let rook_target = side.rook_to(color);
                if physical.contains(rook_target) {
                    None
                } else {
                    Some(GuidanceStep::Move {
                        from: *rook,
                        to: rook_target,
                    })
                }
            }
            _ => None,
        }
    }

    /// Detect mid-castle: king placed on castle target but the move
    /// hasn't completed because the rook is still on its origin square.
    fn detect_mid_castle(
        &self,
        current: ByColor<Bitboard>,
        legal_moves: &MoveList,
    ) -> Option<GuidanceStep> {
        let turn = self.position.turn();
        let our_current = current[turn];
        let expected_our = self.position.board().by_color(turn);
        let newly_placed = our_current & !expected_our;

        for mv in legal_moves {
            if let Move::Castle { king, rook } = *mv {
                let side = CastlingSide::from_king_side(king < rook);
                let king_target = side.king_to(turn);
                if newly_placed.contains(king_target) {
                    let rook_target = side.rook_to(turn);
                    return Some(GuidanceStep::Move {
                        from: rook,
                        to: rook_target,
                    });
                }
            }
        }
        None
    }

    /// Compute game outcome from the current position.
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

    /// Process any completed moves based on sensor state.
    /// Returns the move that was played, if any.
    fn process_moves(&mut self, current: ByColor<Bitboard>) -> Option<Move> {
        if current == self.last_positions {
            return None;
        }
        self.last_positions = current;

        let turn = self.position.turn();

        // When a human color is set, only detect moves for that color.
        // The opponent's moves are applied via apply_opponent_move().
        if self.human_color.is_some_and(|c| c != turn) {
            return None;
        }
        let expected_our = self.position.board().by_color(turn);
        let our_current = current[turn];

        // Pieces of our color that are newly placed relative to the game's expected state.
        let our_placed = our_current & !expected_our;

        // Wait until our piece is placed before processing moves.
        if our_placed.is_empty() {
            return None;
        }

        let current_combined = current.white | current.black;

        // Find a legal move that results in this physical bitboard state.
        // Pre-filter by destination: skip moves that don't land
        // on a newly placed square to avoid expensive clone+play.
        for mv in self.position.legal_moves() {
            // Castling: mv.to() is the rook origin, not king
            // destination, so skip the destination pre-filter.
            if !matches!(mv, Move::Castle { .. }) && !our_placed.contains(mv.to()) {
                continue;
            }

            // We only allow promotions to Queen to simplify physical
            // interaction (no piece selection mechanism on hardware).
            if mv.promotion().is_some_and(|role| role != Role::Queen) {
                continue;
            }

            let mut after = self.position.clone();
            after.play_unchecked(mv);

            if after.board().occupied() == current_combined {
                self.position = after;
                return Some(mv);
            }
        }

        None
    }
}

impl std::fmt::Debug for GameEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fen = Fen::from_position(&self.position, EnPassantMode::Always);
        f.debug_struct("GameEngine")
            .field("position", &fen)
            .field(
                "last_positions",
                &format_args!(
                    "white={:#018X}, black={:#018X}",
                    self.last_positions.white, self.last_positions.black
                ),
            )
            .finish()
    }
}

#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;
    use crate::mock::ScriptedSensor;
    use shakmaty::{CastlingMode, Color, Role, fen::Fen};
    use test_case::test_case;

    fn assert_piece(engine: &GameEngine, square: &str, role: Role, color: Color) {
        let sq: Square = square.parse().expect("asserted square is invalid");
        let expected = Piece { role, color };
        assert_eq!(
            engine.piece_at(sq),
            Some(expected),
            "Expected {:?} at {}, found {:?}",
            expected,
            square,
            engine.piece_at(sq)
        );
    }

    fn assert_empty(engine: &GameEngine, square: &str) {
        let sq: Square = square.parse().expect("asserted square is invalid");
        assert_eq!(
            engine.piece_at(sq),
            None,
            "Expected empty at {}, found {:?}",
            square,
            engine.piece_at(sq)
        );
    }

    impl GameEngine {
        fn from_fen(fen: &str) -> Self {
            let position: Chess = fen
                .parse::<Fen>()
                .expect("invalid FEN")
                .into_position(CastlingMode::Standard)
                .expect("invalid position");
            Self::from_position(position)
        }
    }

    /// Helper to execute a script against an engine.
    fn execute_script(engine: &mut GameEngine, script: &str) {
        let board = engine.position.board();
        let mut sensor = ScriptedSensor::from_bitboards(
            board.by_color(Color::White),
            board.by_color(Color::Black),
        )
        .expect("board positions cannot overlap");
        sensor
            .push_script(script)
            .expect("test script should be valid");
        sensor
            .drain(|p| {
                engine.tick(p);
            })
            .expect("test script should produce valid sensor state");
    }

    #[test_case("e2 We3. "; "one tick")]
    #[test_case("e2.  We3."; "two tick")]
    fn test_simple_move(moves: &str) {
        let mut engine = GameEngine::new();

        execute_script(&mut engine, moves);

        assert_empty(&engine, "e2");
        assert_piece(&engine, "e3", Role::Pawn, Color::White);
    }

    #[test]
    fn test_knight_move() {
        let mut engine = GameEngine::new();

        execute_script(&mut engine, "g1.  Wf3.");

        assert_empty(&engine, "g1");
        assert_piece(&engine, "f3", Role::Knight, Color::White);
    }

    #[test]
    fn test_illegal_move_ignored() {
        let mut engine = GameEngine::new();

        execute_script(&mut engine, "e2.  We5.");

        // Illegal move should be ignored, board unchanged
        assert_piece(&engine, "e2", Role::Pawn, Color::White);
        assert_empty(&engine, "e5");
    }

    #[test]
    fn test_game_sequence() {
        let mut engine = GameEngine::new();

        execute_script(&mut engine, "e2 We4. e7 Be5. g1 Wf3. b8 Bc6.");

        assert_piece(&engine, "e4", Role::Pawn, Color::White);
        assert_piece(&engine, "e5", Role::Pawn, Color::Black);
        assert_piece(&engine, "f3", Role::Knight, Color::White);
        assert_piece(&engine, "c6", Role::Knight, Color::Black);
    }

    #[test]
    fn test_bishop_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        execute_script(&mut engine, "c1. Wg5.");
        assert_piece(&engine, "g5", Role::Bishop, Color::White);
        assert_empty(&engine, "c1");
    }

    #[test]
    fn test_rook_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        execute_script(&mut engine, "a1. Wa3.");
        assert_piece(&engine, "a3", Role::Rook, Color::White);
        assert_empty(&engine, "a1");
    }

    #[test]
    fn test_king_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        execute_script(&mut engine, "e1.  Wd2.");
        assert_piece(&engine, "d2", Role::King, Color::White);
        assert_empty(&engine, "e1");
    }

    #[test]
    fn test_queen_ortho_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        execute_script(&mut engine, "d1. Wd3.");
        assert_piece(&engine, "d3", Role::Queen, Color::White);
        assert_empty(&engine, "d1");
    }

    #[test]
    fn test_queen_diag_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/ppp1pppp/8/3p4/2P5/8/PP1PPPPP/RNBQKBNR w KQkq d6 0 1");

        execute_script(&mut engine, "d1. Wa4.");
        assert_piece(&engine, "a4", Role::Queen, Color::White);
        assert_empty(&engine, "d1");
    }

    #[test_case("d5. e4.  Wd5."; "slow")]
    #[test_case("d5 e4.  Wd5."; "quick take")]
    #[test_case("d5.  e4 Wd5."; "quick move")]
    fn test_capture(moves: &str) {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");

        execute_script(&mut engine, moves);

        assert_piece(&engine, "d5", Role::Pawn, Color::White);
    }

    #[test]
    fn test_pawn_lift() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");

        execute_script(&mut engine, "e4.");

        assert_piece(&engine, "d5", Role::Pawn, Color::Black);
        assert_piece(&engine, "e4", Role::Pawn, Color::White);
    }

    #[test_case("e5. d5.  Wd6."; "capture first")]
    #[test_case("e5.  Wd6.  d5."; "capture last")]
    fn test_en_passant(moves: &str) {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");

        execute_script(&mut engine, moves);
        assert_empty(&engine, "e5");
        assert_piece(&engine, "d6", Role::Pawn, Color::White);
        assert_empty(&engine, "d5");
    }

    #[test_case("e5 Wd6.  d6.  We6.  "; "correction")]
    #[test_case("e5 We6."; "direct")]
    fn test_regular_pawn_move_with_en_passant_available(moves: &str) {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");

        execute_script(&mut engine, moves);
        assert_piece(&engine, "e6", Role::Pawn, Color::White);
        assert_piece(&engine, "d5", Role::Pawn, Color::Black);
        assert_empty(&engine, "e5");
    }

    #[test_case("e1.  Wg1.  h1.  Wf1."; "king first, slow")]
    #[test_case("e1 Wg1. h1 Wf1."; "king first, quick")]
    #[test_case("e1. h1. Wf1.  Wg1."; "rook first, slow")]
    #[test_case("e1.  h1 Wf1. Wg1."; "rook first, quick")]
    #[test_case("e1 h1. Wf1 Wg1."; "two handed")]
    #[test_case("e1.  h1 Wg1.  g1 Wf1.  Wg1. "; "rook slide")]
    fn test_castle_king_side(moves: &str) {
        let mut engine = GameEngine::from_fen(
            "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 0 1",
        );

        execute_script(&mut engine, moves);
        assert_piece(&engine, "g1", Role::King, Color::White);
        assert_piece(&engine, "f1", Role::Rook, Color::White);
        assert_empty(&engine, "e1");
        assert_empty(&engine, "h1");
    }

    #[test_case("e1.  Wc1. a1. Wd1."; "king first, slow")]
    #[test_case("e1 Wc1.  a1 Wd1."; "king first, quick")]
    #[test_case("e1. a1. Wd1. Wc1. "; "rook first, slow")]
    #[test_case("e1. a1 Wd1. Wc1."; "quick")]
    #[test_case("e1 a1. Wc1 Wd1."; "two handed")]
    #[test_case("e1. a1 Wb1. b1 Wc1. c1 Wd1.  Wc1. "; "rook slide")]
    fn test_castle_queen_side(moves: &str) {
        let mut engine = GameEngine::from_fen(
            "r1bqkbnr/ppp3pp/2n1pp2/3p4/3P1B2/2NQ4/PPP1PPPP/R3KBNR w KQkq - 0 1",
        );

        execute_script(&mut engine, moves);
        assert_piece(&engine, "c1", Role::King, Color::White);
        assert_piece(&engine, "d1", Role::Rook, Color::White);
        assert_empty(&engine, "e1");
        assert_empty(&engine, "a1");
    }

    #[test]
    fn test_promotion() {
        let mut engine =
            GameEngine::from_fen("r1bqkbnr/pPpppppp/2n5/8/8/8/PP1PPPPP/RNBQKBNR w KQkq - 0 1");

        execute_script(&mut engine, "b7 Wb8.");
        assert_piece(&engine, "b8", Role::Queen, Color::White);
        assert_empty(&engine, "b7");
    }

    #[test]
    fn test_promotion_capture() {
        let mut engine =
            GameEngine::from_fen("r1bqkbnr/pPpppppp/2n5/8/8/8/PP1PPPPP/RNBQKBNR w KQkq - 0 1");

        execute_script(&mut engine, "a8 b7.  Wa8.");
        assert_piece(&engine, "a8", Role::Queen, Color::White);
        assert_empty(&engine, "b7");
    }

    #[test]
    fn test_tick_returns_valid_state() {
        let mut engine = GameEngine::new();
        let positions = engine.last_positions;

        let state = engine.tick(positions);

        assert_eq!(state.legal_moves().len(), 20);
        assert_eq!(state.lifted_piece(), None);
    }

    #[test]
    fn test_tick_detects_single_lifted_piece() {
        let mut engine = GameEngine::new();
        let mut positions = engine.last_positions;
        positions.white.toggle(Square::E2);

        let state = engine.tick(positions);

        assert_eq!(state.lifted_piece(), Some(Square::E2));
    }

    #[test]
    fn test_tick_no_lifted_piece_when_multiple_missing() {
        let mut engine = GameEngine::new();
        let mut positions = engine.last_positions;
        positions.white.toggle(Square::E2);
        positions.white.toggle(Square::D2);

        let state = engine.tick(positions);

        assert_eq!(state.lifted_piece(), None);
    }

    #[test]
    fn test_captures_correct() {
        let mut engine =
            GameEngine::from_fen("Q2qkbnr/p1pppppp/b1n5/8/8/8/PP1PPPPP/RNBQKBNR w KQk - 0 1");

        execute_script(&mut engine, "a8. d8. Wd8.");

        assert_piece(&engine, "c6", Role::Knight, Color::Black);
        assert_piece(&engine, "d8", Role::Queen, Color::White);
        assert_empty(&engine, "a8");
    }

    #[test]
    fn test_black_simple_pawn_move() {
        let mut engine = GameEngine::new();

        execute_script(&mut engine, "e2 We4. e7 Be5.");

        assert_piece(&engine, "e5", Role::Pawn, Color::Black);
        assert_empty(&engine, "e7");
    }

    #[test]
    fn test_black_knight_move() {
        let mut engine = GameEngine::new();

        execute_script(&mut engine, "e2 We4. b8 Bc6.");

        assert_piece(&engine, "c6", Role::Knight, Color::Black);
        assert_empty(&engine, "b8");
    }

    #[test]
    fn test_scholars_mate_is_checkmate() {
        let mut engine = GameEngine::new();
        // 1.e4 e5 2.Bc4 Nc6 3.Qh5 Nf6?? 4.Qxf7#
        let script = "e2 We4. e7 Be5. f1 Wc4. b8 Bc6. d1 Wh5. g8 Bf6. f7 h5 Wf7.";

        // Use ScriptedSensor directly so we can read final positions
        let board = engine.position.board();
        let mut sensor = ScriptedSensor::from_bitboards(
            board.by_color(Color::White),
            board.by_color(Color::Black),
        )
        .expect("board positions cannot overlap");
        sensor.push_script(script).expect("valid script");
        sensor
            .drain(|p| {
                engine.tick(p);
            })
            .expect("valid sensor state");

        // Tick once more with current positions to get final state
        let state = engine.tick(sensor.read_positions());

        assert!(state.legal_moves().is_empty(), "should be checkmate");
        assert!(state.check_info().is_some(), "king should be in check",);
        assert_piece(&engine, "f7", Role::Queen, Color::White);
    }

    #[test]
    fn test_stalemate() {
        // White: Qb6, Kc6. Black: Ka8. Black to move, no legal
        // White: Qb6, Kc6. Black: Ka8. Black to move, no legal
        // moves, not in check = stalemate.
        let mut engine = GameEngine::from_fen("k7/8/1QK5/8/8/8/8/8 b - - 0 1");

        // Already stalemate — just tick with current positions
        let board = engine.position.board();
        let sensor = ScriptedSensor::from_bitboards(
            board.by_color(Color::White),
            board.by_color(Color::Black),
        )
        .expect("board positions cannot overlap");

        let state = engine.tick(sensor.read_positions());

        assert!(state.legal_moves().is_empty(), "should be stalemate");
        assert!(
            state.check_info().is_none(),
            "should NOT be in check (stalemate, not checkmate)",
        );
    }

    #[test_case("e8 Bg8. h8 Bf8."; "king first slow")]
    #[test_case("e8 h8. Bg8 Bf8."; "two handed")]
    fn test_black_castle_king_side(moves: &str) {
        let mut engine = GameEngine::from_fen(
            "rnbqk2r/pppp1ppp/5n2/2b1p3/2B1P3/5N2/PPPP1PPP/RNBQ1RK1 b kq - 5 4",
        );

        execute_script(&mut engine, moves);

        assert_piece(&engine, "g8", Role::King, Color::Black);
        assert_piece(&engine, "f8", Role::Rook, Color::Black);
        assert_empty(&engine, "e8");
        assert_empty(&engine, "h8");
    }

    #[test_case("e8 Bc8. a8 Bd8."; "king first slow")]
    #[test_case("e8 a8. Bc8 Bd8."; "two handed")]
    fn test_black_castle_queen_side(moves: &str) {
        let mut engine = GameEngine::from_fen(
            "r3kbnr/pppqpppp/2n5/3p1b2/3P1B2/2NQ4/PPP1PPPP/R3KBNR b KQkq - 6 4",
        );

        execute_script(&mut engine, moves);

        assert_piece(&engine, "c8", Role::King, Color::Black);
        assert_piece(&engine, "d8", Role::Rook, Color::Black);
        assert_empty(&engine, "e8");
        assert_empty(&engine, "a8");
    }

    #[test]
    fn test_black_promotion() {
        let mut engine = GameEngine::from_fen("8/8/8/8/8/8/1p5k/4K3 b - - 0 1");

        execute_script(&mut engine, "b2 Bb1.");

        assert_piece(&engine, "b1", Role::Queen, Color::Black);
        assert_empty(&engine, "b2");
    }

    #[test_case("d4. c4. Bc3."; "slow")]
    #[test_case("d4 c4. Bc3."; "quick")]
    fn test_black_en_passant(moves: &str) {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/pp1ppppp/8/8/2Pp4/8/PP1PPPPP/RNBQKBNR b KQkq c3 0 1");

        execute_script(&mut engine, moves);

        assert_piece(&engine, "c3", Role::Pawn, Color::Black);
        assert_empty(&engine, "d4");
        assert_empty(&engine, "c4");
    }

    mod apply_opponent_move {
        use super::*;

        fn setup_for_black_move() -> GameEngine {
            // After 1. e4, it's black's turn
            GameEngine::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1")
        }

        #[test]
        fn rejects_illegal_move() {
            let mut engine = setup_for_black_move();
            let mv = Move::Normal {
                role: Role::Pawn,
                from: Square::E2,
                capture: None,
                to: Square::E4,
                promotion: None,
            };

            let result = engine.apply_opponent_move(&mv);
            assert!(matches!(result, Err(OpponentMoveError::IllegalMove)));
        }

        #[test]
        fn advances_position_after_apply() {
            let mut engine = setup_for_black_move();
            let mv = Move::Normal {
                role: Role::Pawn,
                from: Square::E7,
                capture: None,
                to: Square::E5,
                promotion: None,
            };

            engine.apply_opponent_move(&mv).unwrap();

            assert_eq!(engine.turn(), Color::White);
            assert_piece(&engine, "e5", Role::Pawn, Color::Black);
            assert_empty(&engine, "e7");
        }

        #[test]
        fn expected_positions_match_post_move_board() {
            let mut engine = setup_for_black_move();
            let mv = Move::Normal {
                role: Role::Pawn,
                from: Square::E7,
                capture: None,
                to: Square::E5,
                promotion: None,
            };

            engine.apply_opponent_move(&mv).unwrap();

            let expected = engine.expected_positions();
            let board = engine.position().board();
            assert_eq!(expected.white, board.by_color(Color::White));
            assert_eq!(expected.black, board.by_color(Color::Black));
        }
    }

    mod opponent_move_recovery {
        use super::*;

        /// Set up after 1. e4, apply black's e5 as opponent move, return engine
        /// and a sensor reflecting the pre-move physical board (pawn on e7, not e5).
        fn engine_with_opponent_normal_move() -> (GameEngine, ScriptedSensor) {
            let mut engine =
                GameEngine::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
            let pre_move_board = engine.expected_positions();
            engine.tick(pre_move_board);

            let mv = Move::Normal {
                role: Role::Pawn,
                from: Square::E7,
                capture: None,
                to: Square::E5,
                promotion: None,
            };
            engine.apply_opponent_move(&mv).unwrap();

            let sensor = ScriptedSensor::from_bitboards(pre_move_board.white, pre_move_board.black)
                .expect("board positions cannot overlap");
            (engine, sensor)
        }

        #[test]
        fn tick_does_not_detect_lifted_for_opponent_pieces() {
            let (mut engine, sensor) = engine_with_opponent_normal_move();

            // After opponent move (e7→e5), it's white's turn.
            // tick() only reports lifted pieces for the current side (white).
            // Black's e7 pawn missing from physical doesn't trigger lifted_piece.
            let state = engine.tick(sensor.read_positions());
            assert!(state.lifted_piece.is_none());
        }

        #[test]
        fn human_can_play_after_opponent_move_reconciled() {
            let (mut engine, mut sensor) = engine_with_opponent_normal_move();

            // Complete the physical move: e7→e5
            sensor.push_script("e7 Be5.").unwrap();
            let physical = sensor.tick().unwrap().expect("script produces positions");
            engine.tick(physical);

            assert_eq!(engine.turn(), Color::White);

            // Human plays d2→d4
            sensor.push_script("d2 Wd4.").unwrap();
            let physical = sensor.tick().unwrap().expect("script produces positions");
            engine.tick(physical);

            assert_piece(&engine, "d4", Role::Pawn, Color::White);
            assert_empty(&engine, "d2");
        }
    }
}
