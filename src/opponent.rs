use shakmaty::{Chess, Move, Position, Role};

/// Trait for computing opponent moves.
///
/// Separates move selection from the game engine so implementations
/// can range from a simple heuristic to a remote engine (e.g. Lichess).
pub trait Opponent {
    /// Begin computing a move for the given position.
    fn start_thinking(&mut self, position: &Chess);

    /// Poll for a computed move. Returns `Some(move)` when ready.
    fn poll_move(&mut self) -> Option<Move>;
}

/// Simple embedded opponent that picks a move immediately.
///
/// Heuristic: prefer captures (by victim value), then castling,
/// then queen promotions, then a random non-capture.
pub struct EmbeddedEngine {
    pending: Option<Move>,
    rng_state: u32,
}

impl EmbeddedEngine {
    pub fn new() -> Self {
        Self {
            pending: None,
            rng_state: 0xDEAD_BEEF,
        }
    }

    /// Xorshift32 PRNG — minimal RNG suitable for embedded use.
    fn next_random(&mut self) -> u32 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        x
    }
}

impl Default for EmbeddedEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Piece value for capture ordering (higher = more desirable target).
fn victim_value(role: Role) -> u8 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0, // can't capture king
    }
}

impl Opponent for EmbeddedEngine {
    fn start_thinking(&mut self, position: &Chess) {
        let moves = position.legal_moves();
        let is_allowed = |mv: &Move| mv.promotion().is_none_or(|r| r == Role::Queen);

        let best_capture = moves
            .iter()
            .filter(|mv| mv.is_capture() && is_allowed(mv))
            .max_by_key(|mv| mv.capture().map(victim_value).unwrap_or(0));

        let castle = moves.iter().find(|mv| matches!(mv, Move::Castle { .. }));

        let promotion = moves.iter().find(|mv| mv.promotion() == Some(Role::Queen));

        let random_move = || {
            let candidates: Vec<_> = moves.iter().filter(|mv| is_allowed(mv)).collect();
            if candidates.is_empty() {
                None
            } else {
                let idx = self.next_random() as usize % candidates.len();
                Some(candidates[idx])
            }
        };

        let chosen = best_capture.or(castle).or(promotion).or_else(random_move);

        self.pending = chosen.cloned();
    }

    fn poll_move(&mut self) -> Option<Move> {
        self.pending.take()
    }
}

#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;
    use shakmaty::{CastlingMode, fen::Fen};

    fn position_from_fen(fen: &str) -> Chess {
        let setup: Fen = fen.parse().expect("valid FEN");
        setup
            .into_position(CastlingMode::Standard)
            .expect("valid position")
    }

    #[test]
    fn prefers_capturing_higher_value_piece() {
        // Black knight on c4 can capture queen on d2 or pawn on e3
        let pos = position_from_fen("8/8/8/8/2n5/4P3/3Q4/4K1k1 b - - 0 1");
        let mut engine = EmbeddedEngine::new();
        engine.start_thinking(&pos);
        let mv = engine.poll_move().expect("should have a move");
        assert!(mv.is_capture());
        assert_eq!(mv.capture(), Some(Role::Queen));
    }

    #[test]
    fn picks_non_capture_when_no_captures_available() {
        // Starting position for black — no captures possible
        let pos = position_from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
        let mut engine = EmbeddedEngine::new();
        engine.start_thinking(&pos);
        let mv = engine.poll_move().expect("should have a move");
        assert!(!mv.is_capture());
    }

    #[test]
    fn prefers_castling_over_regular_moves() {
        // Black can castle kingside: king on e8, rook on h8, no pieces between
        let pos = position_from_fen("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R b KQkq - 0 1");
        let mut engine = EmbeddedEngine::new();
        engine.start_thinking(&pos);
        let mv = engine.poll_move().expect("should have a move");
        assert!(matches!(mv, Move::Castle { .. }));
    }

    #[test]
    fn promotes_to_queen() {
        // Black pawn on d2 about to promote
        let pos = position_from_fen("8/8/8/8/8/k7/3p4/K7 b - - 0 1");
        let mut engine = EmbeddedEngine::new();
        engine.start_thinking(&pos);
        let mv = engine.poll_move().expect("should have a move");
        assert_eq!(mv.promotion(), Some(Role::Queen));
    }

    #[test]
    fn poll_returns_none_after_consumed() {
        let pos = position_from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
        let mut engine = EmbeddedEngine::new();
        engine.start_thinking(&pos);
        let _ = engine.poll_move();
        assert!(engine.poll_move().is_none());
    }
}
