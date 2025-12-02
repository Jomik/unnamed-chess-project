use crate::feedback::{CheckInfo, FeedbackSource};
use shakmaty::{
    Bitboard, Chess, EnPassantMode, Move, MoveList, Piece, Position, Role, Square, fen::Fen,
};

/// Current game state snapshot for feedback and display
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState {
    legal_moves: MoveList,
    lifted_piece: Option<Square>,
    captured_piece: Option<Square>,
    king_square: Square,
    checkers: Bitboard,
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
}

/// Core game engine that processes sensor input and maintains game state
#[derive(Default)]
pub struct GameEngine {
    /// The logical chess position (piece types, turn, castling rights, etc.)
    position: Chess,

    /// Last known physical board state from sensors.
    /// Tracks actual piece positions independent of game logic.
    last_bitboard: Bitboard,
}

impl GameEngine {
    #[inline]
    pub fn new() -> Self {
        Self::from_position(Chess::default())
    }

    /// Creates a GameEngine from an existing chess position.
    pub fn from_position(position: Chess) -> Self {
        let bb = position.board().occupied();
        Self {
            position,
            last_bitboard: bb,
        }
    }

    /// Get the piece at a given square, if any
    #[inline]
    pub fn piece_at(&self, square: Square) -> Option<Piece> {
        self.position.board().piece_at(square)
    }

    /// Process a board state reading
    ///
    /// Tracks changes in piece positions and executes legal moves when pieces are placed.
    pub fn tick(&mut self, current_bb: Bitboard) -> GameState {
        self.process_moves(current_bb);

        let lifted = self.position.us() & !current_bb;
        let captured = self.position.them() & !current_bb;
        GameState {
            legal_moves: self.position.legal_moves(),
            lifted_piece: lifted.single_square(),
            captured_piece: captured.single_square(),
            checkers: self.position.checkers(),
            king_square: self
                .position
                .our(Role::King)
                .first()
                .expect("king must exist"),
        }
    }

    /// Process any completed moves based on sensor state
    fn process_moves(&mut self, current_bb: Bitboard) {
        if current_bb == self.last_bitboard {
            return; // Physical board hasn't changed
        }

        // What changed since last tick?
        let placed = current_bb & !self.last_bitboard; // Pieces added this tick
        let expected = self.position.board().occupied();
        let lifted = expected & !current_bb; // Pieces lifted from actual game

        // Update last_bitboard
        self.last_bitboard = current_bb;

        // Wait until pieces are placed before processing moves.
        // This allows lifting pieces without triggering move execution.
        // Exception: If exactly 2 pieces are lifted, process anyway as this could be en passant.
        if placed.is_empty() && lifted.count() != 2 {
            return;
        }

        // Find a legal move that results in this physical bitboard state
        for mv in self.position.legal_moves() {
            // We only allow promotions to Queen to simplify physical interaction (no piece selection mechanism on hardware).
            if mv.promotion().is_some_and(|role| role != Role::Queen) {
                continue;
            }

            // For normal captures, verify the piece was placed on the capture square.
            // En passant is excluded because the destination differs from the captured pawn's square,
            // and its unique board state is already validated by the bitboard check below.
            if mv.is_capture() && Some(mv.to()) != placed.first() && !mv.is_en_passant() {
                continue;
            }

            let mut after = self.position.clone();
            after.play_unchecked(mv);

            if after.board().occupied() == current_bb {
                self.position = after;
                break;
            }
        }
    }
}

impl std::fmt::Debug for GameEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fen = Fen::from_position(&self.position, EnPassantMode::Always);
        f.debug_struct("GameEngine")
            .field("position", &fen)
            .field(
                "last_bitboard",
                &format_args!("{:#018X}", self.last_bitboard),
            )
            .finish()
    }
}

#[cfg(test)]
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
        let mut sensor = ScriptedSensor::from_bitboard(engine.last_bitboard);
        sensor
            .push_script(script)
            .expect("test script should be valid");
        sensor.drain(|bb| {
            engine.tick(bb);
        });
    }

    #[test_case("e2e3. "; "one tick")]
    #[test_case("e2.  e3."; "two tick")]
    fn test_simple_move(moves: &str) {
        let mut engine = GameEngine::new();

        execute_script(&mut engine, moves);

        assert_empty(&engine, "e2");
        assert_piece(&engine, "e3", Role::Pawn, Color::White);
    }

    #[test]
    fn test_knight_move() {
        let mut engine = GameEngine::new();

        execute_script(&mut engine, "g1.  f3.");

        assert_empty(&engine, "g1");
        assert_piece(&engine, "f3", Role::Knight, Color::White);
    }

    #[test]
    fn test_illegal_move_ignored() {
        let mut engine = GameEngine::new();

        execute_script(&mut engine, "e2.  e5.");

        // Illegal move should be ignored, board unchanged
        assert_piece(&engine, "e2", Role::Pawn, Color::White);
        assert_empty(&engine, "e5");
    }

    #[test]
    fn test_game_sequence() {
        let mut engine = GameEngine::new();

        execute_script(&mut engine, "e2e4. e7e5. g1f3. b8c6.");

        assert_piece(&engine, "e4", Role::Pawn, Color::White);
        assert_piece(&engine, "e5", Role::Pawn, Color::Black);
        assert_piece(&engine, "f3", Role::Knight, Color::White);
        assert_piece(&engine, "c6", Role::Knight, Color::Black);
    }

    #[test]
    fn test_bishop_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        execute_script(&mut engine, "c1. g5.");
        assert_piece(&engine, "g5", Role::Bishop, Color::White);
        assert_empty(&engine, "c1");
    }

    #[test]
    fn test_rook_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        execute_script(&mut engine, "a1. a3.");
        assert_piece(&engine, "a3", Role::Rook, Color::White);
        assert_empty(&engine, "a1");
    }

    #[test]
    fn test_king_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        execute_script(&mut engine, "e1.  d2.");
        assert_piece(&engine, "d2", Role::King, Color::White);
        assert_empty(&engine, "e1");
    }

    #[test]
    fn test_queen_ortho_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        execute_script(&mut engine, "d1. d3.");
        assert_piece(&engine, "d3", Role::Queen, Color::White);
        assert_empty(&engine, "d1");
    }

    #[test]
    fn test_queen_diag_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/ppp1pppp/8/3p4/2P5/8/PP1PPPPP/RNBQKBNR w KQkq d6 0 1");

        execute_script(&mut engine, "d1. a4.");
        assert_piece(&engine, "a4", Role::Queen, Color::White);
        assert_empty(&engine, "d1");
    }

    #[test_case("d5. e4.  d5."; "slow")]
    #[test_case("d5 e4.  d5."; "quick take")]
    #[test_case("d5.  e4 d5."; "quick move")]
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

    #[test_case("e5. d5.  d6."; "capture first")]
    #[test_case("e5.  d6.  d5."; "capture last")]
    fn test_en_passant(moves: &str) {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");

        execute_script(&mut engine, moves);
        assert_empty(&engine, "e5");
        assert_piece(&engine, "d6", Role::Pawn, Color::White);
        assert_empty(&engine, "d5");
    }

    #[test_case("e5d6.  d6.  e6.  "; "correction")]
    #[test_case("e5e6."; "direct")]
    fn test_regular_pawn_move_with_en_passant_available(moves: &str) {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");

        execute_script(&mut engine, moves);
        assert_piece(&engine, "e6", Role::Pawn, Color::White);
        assert_piece(&engine, "d5", Role::Pawn, Color::Black);
        assert_empty(&engine, "e5");
    }

    #[test_case("e1.  g1.  h1.  f1."; "king first, slow")]
    #[test_case("e1g1. h1f1."; "king first, quick")]
    #[test_case("e1. h1. f1.  g1."; "rook first, slow")]
    #[test_case("e1.  h1f1. g1."; "rook first, quick")]
    #[test_case("e1h1. f1g1."; "two handed")]
    #[test_case("e1.  h1g1.  g1f1.  g1. "; "rook slide")]
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

    #[test_case("e1.  c1. a1. d1."; "king first, slow")]
    #[test_case("e1c1.  a1d1."; "king first, quick")]
    #[test_case("e1. a1. d1. c1. "; "rook first, slow")]
    #[test_case("e1. a1d1. c1."; "quick")]
    #[test_case("e1a1. c1d1."; "two handed")]
    #[test_case("e1. a1b1. b1c1. c1d1.  c1. "; "rook slide")]
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

        execute_script(&mut engine, "b7b8.");
        assert_piece(&engine, "b8", Role::Queen, Color::White);
        assert_empty(&engine, "b7");
    }

    #[test]
    fn test_promotion_capture() {
        let mut engine =
            GameEngine::from_fen("r1bqkbnr/pPpppppp/2n5/8/8/8/PP1PPPPP/RNBQKBNR w KQkq - 0 1");

        execute_script(&mut engine, "a8b7.  a8.");
        assert_piece(&engine, "a8", Role::Queen, Color::White);
        assert_empty(&engine, "b7");
    }

    #[test]
    fn test_tick_returns_valid_state() {
        let mut engine = GameEngine::new();
        let bb = engine.last_bitboard;

        let state = engine.tick(bb);

        assert_eq!(state.legal_moves().len(), 20);
        assert_eq!(state.lifted_piece(), None);
    }

    #[test]
    fn test_tick_detects_single_lifted_piece() {
        let mut engine = GameEngine::new();
        let mut bb = engine.last_bitboard;
        bb.toggle(Square::E2);

        let state = engine.tick(bb);

        assert_eq!(state.lifted_piece(), Some(Square::E2));
    }

    #[test]
    fn test_tick_no_lifted_piece_when_multiple_missing() {
        let mut engine = GameEngine::new();
        let mut bb = engine.last_bitboard;
        bb.toggle(Square::E2);
        bb.toggle(Square::D2);

        let state = engine.tick(bb);

        assert_eq!(state.lifted_piece(), None);
    }

    #[test]
    fn test_captures_correct() {
        let mut engine =
            GameEngine::from_fen("Q2qkbnr/p1pppppp/b1n5/8/8/8/PP1PPPPP/RNBQKBNR w KQk - 0 1");

        execute_script(&mut engine, "a8. d8. d8.");

        assert_piece(&engine, "c6", Role::Knight, Color::Black);
        assert_piece(&engine, "d8", Role::Queen, Color::White);
        assert_empty(&engine, "a8");
    }
}
