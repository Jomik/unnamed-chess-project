use shakmaty::{Bitboard, ByColor, Chess, Move, Position, Role};

use super::Player;

/// Simple embedded opponent that picks a move immediately.
///
/// Heuristic: prefer captures (by victim value), then castling,
/// then queen promotions, then a random non-king move.
#[derive(Debug)]
pub struct EmbeddedEngine {
    rng_state: u32,
}

impl EmbeddedEngine {
    pub fn new(seed: u32) -> Self {
        Self { rng_state: seed }
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

fn victim_value(role: Role) -> u8 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    }
}

impl Player for EmbeddedEngine {
    fn poll_move(&mut self, position: &Chess, _sensors: ByColor<Bitboard>) -> Option<Move> {
        let moves = position.legal_moves();
        let is_allowed = |mv: &Move| mv.promotion().is_none_or(|r| r == Role::Queen);

        let best_capture = moves
            .iter()
            .filter(|mv| mv.is_capture() && is_allowed(mv))
            .max_by_key(|mv| mv.capture().map(victim_value).unwrap_or(0));

        let castle = moves.iter().find(|mv| matches!(mv, Move::Castle { .. }));

        let promotion = moves.iter().find(|mv| mv.promotion() == Some(Role::Queen));

        let random_move = || {
            let non_king: Vec<_> = moves
                .iter()
                .filter(|mv| is_allowed(mv) && mv.role() != Role::King)
                .collect();
            let candidates = if non_king.is_empty() {
                moves.iter().filter(|mv| is_allowed(mv)).collect()
            } else {
                non_king
            };
            if candidates.is_empty() {
                None
            } else {
                let idx = self.next_random() as usize % candidates.len();
                Some(candidates[idx])
            }
        };

        best_capture
            .or(castle)
            .or(promotion)
            .or_else(random_move)
            .cloned()
    }

    fn is_interactive(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::{CastlingMode, fen::Fen};

    fn position_from_fen(fen: &str) -> Chess {
        let setup: Fen = fen.parse().expect("valid FEN");
        setup
            .into_position(CastlingMode::Standard)
            .expect("valid position")
    }

    fn dummy_sensors() -> ByColor<Bitboard> {
        ByColor {
            white: Bitboard::EMPTY,
            black: Bitboard::EMPTY,
        }
    }

    #[test]
    fn prefers_capturing_higher_value_piece() {
        // Black knight on c4 can capture queen on d2 or pawn on e3
        let pos = position_from_fen("8/8/8/8/2n5/4P3/3Q4/4K1k1 b - - 0 1");
        let mut engine = EmbeddedEngine::new(42);
        let mv = engine
            .poll_move(&pos, dummy_sensors())
            .expect("should have a move");
        assert!(mv.is_capture());
        assert_eq!(mv.capture(), Some(Role::Queen));
    }

    #[test]
    fn picks_non_capture_when_no_captures_available() {
        // After 1. e4 — black has no captures available
        let pos = position_from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
        let mut engine = EmbeddedEngine::new(42);
        let mv = engine
            .poll_move(&pos, dummy_sensors())
            .expect("should have a move");
        assert!(!mv.is_capture());
    }

    #[test]
    fn prefers_castling_over_regular_moves() {
        // Black can castle both sides
        let pos = position_from_fen("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R b KQkq - 0 1");
        let mut engine = EmbeddedEngine::new(42);
        let mv = engine
            .poll_move(&pos, dummy_sensors())
            .expect("should have a move");
        assert!(matches!(mv, Move::Castle { .. }));
    }

    #[test]
    fn promotes_to_queen() {
        // Black pawn on d2 about to promote
        let pos = position_from_fen("8/8/8/8/8/k7/3p4/K7 b - - 0 1");
        let mut engine = EmbeddedEngine::new(42);
        let mv = engine
            .poll_move(&pos, dummy_sensors())
            .expect("should have a move");
        assert_eq!(mv.promotion(), Some(Role::Queen));
    }

    #[test]
    fn avoids_king_moves_when_other_pieces_can_move() {
        // Black king on g8, knight on f6 — should prefer knight moves
        let pos = position_from_fen("6k1/8/5n2/8/8/8/8/4K3 b - - 0 1");
        let mut engine = EmbeddedEngine::new(42);
        for _ in 0..20 {
            let mv = engine
                .poll_move(&pos, dummy_sensors())
                .expect("should have a move");
            assert_ne!(mv.role(), Role::King);
        }
    }

    #[test]
    fn moves_king_when_only_king_can_move() {
        // Lone black king — only king moves are legal
        let pos = position_from_fen("8/8/8/8/8/8/8/k3K3 b - - 0 1");
        let mut engine = EmbeddedEngine::new(42);
        let mv = engine
            .poll_move(&pos, dummy_sensors())
            .expect("should have a move");
        assert_eq!(mv.role(), Role::King);
    }

    #[test]
    fn poll_returns_move_every_call() {
        // Engine computes fresh each call — always returns Some when moves are available
        let pos = position_from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
        let mut engine = EmbeddedEngine::new(42);
        assert!(engine.poll_move(&pos, dummy_sensors()).is_some());
        assert!(engine.poll_move(&pos, dummy_sensors()).is_some());
    }

    #[test]
    fn engine_moves_as_white_without_opponent_moved() {
        // Regression: engine playing white must move on first poll, before opponent_moved is called
        let pos = Chess::default();
        let mut engine = EmbeddedEngine::new(42);
        let mv = engine.poll_move(&pos, dummy_sensors());
        assert!(
            mv.is_some(),
            "engine should return a move on first poll without needing opponent_moved"
        );
    }
}
