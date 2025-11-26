use shakmaty::{Bitboard, Chess, EnPassantMode, Piece, Position, Square, fen::Fen};

/// Core game engine that processes sensor input and maintains game state
pub struct GameEngine {
    /// The logical chess position (piece types, turn, castling rights, etc.)
    position: Chess,

    /// Last known physical board state from sensors
    last_bitboard: Bitboard,
}

impl GameEngine {
    pub fn new() -> Self {
        Self {
            position: Chess::default(),
            last_bitboard: Chess::default().board().occupied(),
        }
    }

    /// Get the piece at a given square, if any
    pub fn piece_at(&self, square: Square) -> Option<Piece> {
        self.position.board().piece_at(square)
    }

    /// Process a board state reading
    ///
    /// The engine tracks changes
    pub fn tick(&mut self, current_bb: Bitboard) {
        if current_bb == self.last_bitboard {
            return; // Physical board hasn't changed
        }

        let expected = self.position.board().occupied();

        // Difference from current position
        let missing = expected & !current_bb;

        // Change this tick
        let added = current_bb & !self.last_bitboard;

        let touched_pieces = self.position.us() & missing;
        if touched_pieces.count() == 1 {
            // We are moving, handle castling later.
            if let Some(from) = touched_pieces.first()
                && let Some(to) = added.first()
            {
                let legal = self
                    .position
                    .legal_moves()
                    .into_iter()
                    .find(|m| m.from() == Some(from) && m.to() == to);
                if let Some(mv) = legal {
                    // Play the move and update the position
                    self.position.play_unchecked(mv);
                }
            }
        }

        self.last_bitboard = current_bb;
    }
}

impl Default for GameEngine {
    fn default() -> Self {
        Self::new()
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
mod test_dsl {
    use super::*;

    /// A sequence of physical board actions
    #[derive(Debug)]
    pub struct BoardScript {
        batches: Vec<Vec<Square>>,
    }

    impl BoardScript {
        /// Parse a script from a string
        ///
        /// Format:
        /// - Squares are 2 characters (e.g., "e2", "a1")
        /// - Spaces separate squares in the same batch
        /// - Periods (".") trigger a tick
        ///
        /// Examples:
        /// ```
        /// "e2e4."          // Toggle e2 & e4 together, then tick
        /// "e2 e4."         // Same (explicit space)
        /// "e2. e4."        // Toggle e2, tick, toggle e4, tick
        /// "d5. e4d5."     // Toggle d5, tick, toggle e4&d5, tick
        /// ```
        pub fn parse(script: &str) -> Self {
            let mut batches = vec![Vec::new()];
            let mut current_token = String::new();

            for ch in script.chars() {
                match ch {
                    '.' => {
                        Self::flush_token(&mut current_token, &mut batches);
                        batches.push(Vec::new());
                    }
                    c if c.is_whitespace() => {
                        Self::flush_token(&mut current_token, &mut batches);
                    }
                    _ => {
                        current_token.push(ch);

                        // Squares are exactly 2 characters (e.g., "e2", "a1")
                        if current_token.len() == 2 {
                            Self::flush_token(&mut current_token, &mut batches);
                        }
                    }
                }
            }

            // Flush any remaining token
            Self::flush_token(&mut current_token, &mut batches);

            Self { batches }
        }

        /// Add current token to the last batch and clear it
        fn flush_token(token: &mut String, batches: &mut [Vec<Square>]) {
            if !token.is_empty() {
                let square = token
                    .trim()
                    .parse()
                    .expect("invalid square notation in script");
                batches
                    .last_mut()
                    .expect("batches should never be empty")
                    .push(square);
                token.clear();
            }
        }

        /// Execute the script against a game engine
        ///
        /// Simulates a hardware sensor sending board states to the engine
        pub fn execute(&self, engine: &mut GameEngine) {
            let mut current_board = engine.last_bitboard;

            for batch in &self.batches {
                // Toggle all squares in the batch
                for &square in batch {
                    current_board.toggle(square);
                }

                // Send the sensor reading to the engine
                engine.tick(current_board);
            }
        }
    }
}

#[cfg(test)]
mod test_helpers {
    use super::*;
    use shakmaty::{CastlingMode, Color, Role, fen::Fen};

    pub fn assert_piece(engine: &GameEngine, square: &str, role: Role, color: Color) {
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

    pub fn assert_empty(engine: &GameEngine, square: &str) {
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
        pub fn from_fen(fen: &str) -> Self {
            let position: Chess = fen
                .parse::<Fen>()
                .expect("invalid FEN")
                .into_position(CastlingMode::Standard)
                .expect("invalid position");
            let last_bitboard = position.board().occupied();
            Self {
                position,
                last_bitboard,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::{Color, Role};
    use test_dsl::BoardScript;
    use test_helpers::*;

    #[test]
    fn test_simple_move_one_tick() {
        let mut engine = GameEngine::new();

        BoardScript::parse("e2e3.").execute(&mut engine);

        assert_empty(&engine, "e2");
        assert_piece(&engine, "e3", Role::Pawn, Color::White);
    }

    #[test]
    fn test_simple_move_two_ticks() {
        let mut engine = GameEngine::new();

        BoardScript::parse("e2. e3.").execute(&mut engine);

        assert_empty(&engine, "e2");
        assert_piece(&engine, "e3", Role::Pawn, Color::White);
    }

    #[test]
    fn test_knight_move() {
        let mut engine = GameEngine::new();

        BoardScript::parse("g1. f3.").execute(&mut engine);

        assert_empty(&engine, "g1");
        assert_piece(&engine, "f3", Role::Knight, Color::White);
    }

    #[test]
    fn test_illegal_move_ignored() {
        let mut engine = GameEngine::new();

        BoardScript::parse("e2. e5.").execute(&mut engine);

        // Illegal move should be ignored, board unchanged
        assert_piece(&engine, "e2", Role::Pawn, Color::White);
        assert_empty(&engine, "e5");
    }

    #[test]
    fn test_game_sequence() {
        let mut engine = GameEngine::new();

        BoardScript::parse("e2e4. e7e5. g1f3. b8c6.").execute(&mut engine);

        assert_piece(&engine, "e4", Role::Pawn, Color::White);
        assert_piece(&engine, "e5", Role::Pawn, Color::Black);
        assert_piece(&engine, "f3", Role::Knight, Color::White);
        assert_piece(&engine, "c6", Role::Knight, Color::Black);
    }

    #[test]
    fn test_bishop_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        BoardScript::parse("c1. g5.").execute(&mut engine);
        assert_piece(&engine, "g5", Role::Bishop, Color::White);
        assert_empty(&engine, "c1");
    }

    #[test]
    fn test_rook_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        BoardScript::parse("a1. a3.").execute(&mut engine);
        assert_piece(&engine, "a3", Role::Rook, Color::White);
        assert_empty(&engine, "a1");
    }

    #[test]
    fn test_king_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        BoardScript::parse("e1. d2.").execute(&mut engine);
        assert_piece(&engine, "d2", Role::King, Color::White);
        assert_empty(&engine, "e1");
    }

    #[test]
    fn test_queen_ortho_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/8/p2p4/P2P4/8/1PP1PPPP/RNBQKBNR w KQkq a6 0 1");

        BoardScript::parse("d1. d3.").execute(&mut engine);
        assert_piece(&engine, "d3", Role::Queen, Color::White);
        assert_empty(&engine, "d1");
    }

    #[test]
    fn test_queen_diag_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/ppp1pppp/8/3p4/2P5/8/PP1PPPPP/RNBQKBNR w KQkq d6 0 1");

        BoardScript::parse("d1. a4.").execute(&mut engine);
        assert_piece(&engine, "a4", Role::Queen, Color::White);
        assert_empty(&engine, "d1");
    }

    #[test]
    fn test_capture_slow() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");

        BoardScript::parse("d5. e4. d5.").execute(&mut engine);

        assert_piece(&engine, "d5", Role::Pawn, Color::White);
    }

    #[test]
    fn test_capture_quick_take() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");

        BoardScript::parse("d5 e4. d5.").execute(&mut engine);

        assert_piece(&engine, "d5", Role::Pawn, Color::White);
    }

    #[test]
    fn test_capture_quick_move() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");

        BoardScript::parse("d5. e4 d5.").execute(&mut engine);

        assert_piece(&engine, "d5", Role::Pawn, Color::White);
    }

    #[test]
    fn test_en_passant() {
        let mut engine =
            GameEngine::from_fen("rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");

        BoardScript::parse("e5. d5 d6.").execute(&mut engine);
        assert_piece(&engine, "d6", Role::Pawn, Color::White);
        assert_empty(&engine, "d5");
    }
}
