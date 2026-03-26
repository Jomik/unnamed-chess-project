use shakmaty::{Bitboard, ByColor, Chess, Move};

use crate::feedback::{BoardFeedback, StatusKind, compute_feedback};
use crate::game_logic::{GameEngine, GameState};
use crate::player::Player;

/// Result of processing one sensor frame.
#[derive(Debug, Clone)]
pub struct TickResult {
    /// Game state snapshot from the engine.
    pub state: GameState,
    /// Computed board feedback (includes recovery fallback when idle).
    pub feedback: BoardFeedback,
    /// The computer opponent's reply, if one was produced this tick.
    pub computer_move: Option<Move>,
}

/// Owns the game lifecycle: engine state and optional opponent.
///
/// Encapsulates the per-tick orchestration sequence that both
/// the hardware loop and terminal simulator share:
/// `engine.tick()` → opponent handling → feedback computation → recovery fallback.
pub struct GameSession {
    engine: GameEngine,
    opponent: Option<Box<dyn Player>>,
    prev_sensors: ByColor<Bitboard>,
}

impl Default for GameSession {
    fn default() -> Self {
        Self::new()
    }
}

impl GameSession {
    /// Create a session from the standard starting position with no opponent.
    pub fn new() -> Self {
        let engine = GameEngine::new();
        let prev_sensors = engine.expected_positions();
        Self {
            engine,
            opponent: None,
            prev_sensors,
        }
    }

    /// Create a session from the standard starting position with an opponent.
    /// The human plays White; the opponent controls Black.
    pub fn with_opponent(opponent: Box<dyn Player>) -> Self {
        let engine =
            GameEngine::from_position_for_color(Chess::default(), Some(shakmaty::Color::White));
        let prev_sensors = engine.expected_positions();
        Self {
            engine,
            opponent: Some(opponent),
            prev_sensors,
        }
    }

    /// Create a session from a specific position with an opponent.
    /// The human plays White; the opponent controls Black.
    pub fn from_position_with_opponent(position: Chess, opponent: Box<dyn Player>) -> Self {
        let engine = GameEngine::from_position_for_color(position, Some(shakmaty::Color::White));
        let prev_sensors = engine.expected_positions();
        Self {
            engine,
            opponent: Some(opponent),
            prev_sensors,
        }
    }

    /// Process one sensor frame.
    ///
    /// Advances the game engine, triggers the opponent if a human move
    /// was detected, and computes feedback (falling back to recovery
    /// guidance when idle and the physical board diverges).
    pub fn process_positions(&mut self, positions: ByColor<Bitboard>) -> TickResult {
        let state = self.engine.tick(positions);

        self.handle_human_move(&state);
        let computer_move = self.poll_opponent_move();

        let mut feedback = compute_feedback(self.engine.position(), self.prev_sensors, positions);

        // Merge error status AFTER recovery fallback so recovery squares still appear
        if self
            .opponent
            .as_ref()
            .is_some_and(|o| o.status() == crate::player::PlayerStatus::Error)
        {
            feedback = feedback.with_merged_status(StatusKind::Failure);
        }

        self.prev_sensors = positions;

        TickResult {
            state,
            feedback,
            computer_move,
        }
    }

    /// Read-only access to the underlying engine.
    #[inline]
    pub fn engine(&self) -> &GameEngine {
        &self.engine
    }

    /// Read-only access to the current chess position.
    #[inline]
    pub fn position(&self) -> &Chess {
        self.engine.position()
    }

    fn handle_human_move(&mut self, state: &GameState) {
        let Some(opponent) = self.opponent.as_mut() else {
            return;
        };
        let Some(human_move) = state.human_move() else {
            return;
        };
        opponent.opponent_moved(self.engine.position(), human_move);
    }

    fn poll_opponent_move(&mut self) -> Option<Move> {
        let opponent = self.opponent.as_mut()?;
        let sensors = self.engine.expected_positions();
        let reply = opponent.poll_move(self.engine.position(), sensors)?;
        match self.engine.apply_opponent_move(&reply) {
            Ok(()) => Some(reply),
            Err(e) => {
                log::warn!("Computer move failed: {e}");
                None
            }
        }
    }
}

#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;
    use crate::feedback::SquareFeedback;
    use crate::mock::ScriptedSensor;
    use crate::player::EmbeddedEngine;
    use shakmaty::{Color, Position, Square};

    /// Helper: push script, drain through session, return last TickResult.
    fn run_script(sensor: &mut ScriptedSensor, session: &mut GameSession) -> TickResult {
        let mut last = None;
        sensor
            .drain(|p| {
                last = Some(session.process_positions(p));
            })
            .expect("script should parse");
        last.expect("script should produce at least one tick")
    }

    /// Test opponent that delays returning a move for N ticks.
    #[cfg(test)]
    struct DelayedOpponent {
        delay_ticks: usize,
        ticks_remaining: usize,
        pending: Option<Move>,
    }

    #[cfg(test)]
    impl DelayedOpponent {
        fn new(delay_ticks: usize) -> Self {
            Self {
                delay_ticks,
                ticks_remaining: 0,
                pending: None,
            }
        }
    }

    #[cfg(test)]
    impl Player for DelayedOpponent {
        fn opponent_moved(&mut self, position: &Chess, _opponent_move: &Move) {
            // Pick first legal move
            let moves = position.legal_moves();
            self.pending = moves.into_iter().next();
            // Add 1 to delay_ticks because the first poll_move call happens in the same tick as opponent_moved
            self.ticks_remaining = self.delay_ticks + 1;
        }

        fn poll_move(&mut self, _position: &Chess, _sensors: ByColor<Bitboard>) -> Option<Move> {
            if self.ticks_remaining > 0 {
                self.ticks_remaining -= 1;
                None
            } else if self.pending.is_some() {
                self.pending.take()
            } else {
                None
            }
        }
    }

    /// Test opponent that produces an error.
    #[cfg(test)]
    struct ErrorOpponent {
        errored: bool,
    }

    #[cfg(test)]
    impl Player for ErrorOpponent {
        fn opponent_moved(&mut self, _position: &Chess, _opponent_move: &Move) {
            self.errored = true;
        }

        fn poll_move(&mut self, _position: &Chess, _sensors: ByColor<Bitboard>) -> Option<Move> {
            None
        }

        fn status(&self) -> crate::player::PlayerStatus {
            if self.errored {
                crate::player::PlayerStatus::Error
            } else {
                crate::player::PlayerStatus::Active
            }
        }
    }

    #[test]
    fn process_positions_advances_game() {
        let mut sensor = ScriptedSensor::new();
        let mut session = GameSession::new();

        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        assert!(result.state.human_move().is_some());
        assert_eq!(result.computer_move, None);
        assert_eq!(session.position().turn(), Color::Black);
    }

    #[test]
    fn with_opponent_produces_computer_move() {
        let mut sensor = ScriptedSensor::new();
        let mut session = GameSession::with_opponent(Box::new(EmbeddedEngine::new(42)));

        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        assert!(result.state.human_move().is_some());
        assert!(result.computer_move.is_some());
        assert_eq!(session.position().turn(), Color::White);
    }

    #[test]
    fn no_opponent_no_computer_move() {
        let mut sensor = ScriptedSensor::new();
        let mut session = GameSession::new();

        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        assert!(result.state.human_move().is_some());
        assert_eq!(result.computer_move, None);
        assert_eq!(session.position().turn(), Color::Black);
    }

    #[test]
    fn idle_tick_produces_empty_feedback() {
        let sensor = ScriptedSensor::new();
        let mut session = GameSession::new();

        let positions = sensor.read_positions();
        let result = session.process_positions(positions);

        assert!(result.state.human_move().is_none());
        assert!(result.feedback.is_empty());
        assert_eq!(result.computer_move, None);
    }

    #[test]
    fn lifted_piece_shows_destinations() {
        let mut sensor = ScriptedSensor::new();
        let mut session = GameSession::new();

        sensor.push_script("e2.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        assert!(result.state.human_move().is_none());
        assert_eq!(result.state.lifted_piece(), Some(Square::E2));
        assert!(result.feedback.get(Square::E3).is_some());
        assert!(result.feedback.get(Square::E4).is_some());
    }

    #[test]
    fn recovery_feedback_when_board_diverges() {
        let mut sensor = ScriptedSensor::new();
        let mut session = GameSession::with_opponent(Box::new(EmbeddedEngine::new(42)));

        // Human plays e2-e4, computer replies
        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);
        let computer_move = result.computer_move.expect("opponent should reply");

        // Physical board hasn't been updated for the computer move yet,
        // so next tick should produce recovery feedback.
        let positions = sensor.read_positions();
        let result = session.process_positions(positions);

        assert!(!result.feedback.is_empty());

        // Computer move target should show as Destination (piece needs placing)
        let to = computer_move.to();
        assert_eq!(
            result.feedback.get(to),
            Some(SquareFeedback::Destination),
            "computer move target {to} should show as Destination"
        );
    }

    #[test]
    fn from_position_with_opponent_starts_at_given_position() {
        // Start from a position where it's White's turn (human plays White).
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let position: Chess = fen
            .parse::<shakmaty::fen::Fen>()
            .unwrap()
            .into_position(shakmaty::CastlingMode::Standard)
            .unwrap();

        let board = position.board();
        let mut sensor = ScriptedSensor::from_bitboards(
            board.by_color(Color::White),
            board.by_color(Color::Black),
        )
        .unwrap();
        let mut session =
            GameSession::from_position_with_opponent(position, Box::new(EmbeddedEngine::new(42)));

        assert_eq!(session.position().turn(), Color::White);

        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);
        assert!(result.computer_move.is_some(), "opponent should be active");
    }

    #[test]
    fn async_opponent_returns_move_after_delay() {
        let mut sensor = ScriptedSensor::new();
        let mut session = GameSession::with_opponent(Box::new(DelayedOpponent::new(2)));

        // Human plays e2-e4
        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        // Move detected but opponent hasn't replied yet (delay=2)
        assert!(result.state.human_move().is_some());
        assert!(result.computer_move.is_none());

        // Tick 1: still waiting
        let positions = sensor.read_positions();
        let result = session.process_positions(positions);
        assert!(result.computer_move.is_none());

        // Tick 2: still waiting (ticks_remaining was 2, now 1)
        let positions = sensor.read_positions();
        let result = session.process_positions(positions);
        assert!(result.computer_move.is_none());

        // Tick 3: move arrives
        let positions = sensor.read_positions();
        let result = session.process_positions(positions);
        assert!(result.computer_move.is_some());
        assert_eq!(session.position().turn(), Color::White);
    }

    #[test]
    fn error_opponent_produces_failure_feedback() {
        use crate::feedback::StatusKind;

        let mut sensor = ScriptedSensor::new();
        let mut session = GameSession::with_opponent(Box::new(ErrorOpponent { errored: false }));

        // Human plays — triggers start_thinking which sets error
        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        assert_eq!(result.feedback.status(), Some(StatusKind::Failure));
    }
}
