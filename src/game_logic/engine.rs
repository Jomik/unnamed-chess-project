use shakmaty::{Bitboard, Chess, Piece, Position, Square};

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
            last_bitboard: Bitboard::EMPTY,
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
