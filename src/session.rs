use shakmaty::{Bitboard, ByColor, Chess, Move};

use crate::feedback::{BoardFeedback, compute_feedback};
use crate::game_logic::{GameEngine, GameState};
use crate::opponent::Opponent;
use crate::recovery::recovery_feedback;

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
    opponent: Option<Box<dyn Opponent>>,
}

impl Default for GameSession {
    fn default() -> Self {
        Self::new()
    }
}

impl GameSession {
    /// Create a session from the standard starting position with no opponent.
    pub fn new() -> Self {
        Self {
            engine: GameEngine::new(),
            opponent: None,
        }
    }

    /// Create a session from the standard starting position with an opponent.
    pub fn with_opponent(opponent: Box<dyn Opponent>) -> Self {
        Self {
            engine: GameEngine::new(),
            opponent: Some(opponent),
        }
    }

    /// Create a session from a specific position with an opponent.
    pub fn from_position_with_opponent(position: Chess, opponent: Box<dyn Opponent>) -> Self {
        Self {
            engine: GameEngine::from_position(position),
            opponent: Some(opponent),
        }
    }

    /// Process one sensor frame.
    ///
    /// Advances the game engine, triggers the opponent if a human move
    /// was detected, and computes feedback (falling back to recovery
    /// guidance when idle and the physical board diverges).
    pub fn process_positions(&mut self, positions: ByColor<Bitboard>) -> TickResult {
        let state = self.engine.tick(positions);

        let computer_move = self.handle_opponent(&state);

        let feedback = compute_feedback(&state);
        let feedback = if feedback.is_empty() {
            recovery_feedback(&self.engine.expected_positions(), &positions).unwrap_or(feedback)
        } else {
            feedback
        };

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

    fn handle_opponent(&mut self, state: &GameState) -> Option<Move> {
        let opponent = self.opponent.as_mut()?;
        let human_move = state.human_move()?;

        opponent.start_thinking(self.engine.position(), human_move);

        let reply = opponent.poll_move()?;
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
    use crate::feedback::{FeedbackSource, SquareFeedback};
    use crate::mock::ScriptedSensor;
    use crate::opponent::EmbeddedEngine;
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
        let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
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

        assert_eq!(session.position().turn(), Color::Black);

        sensor.push_script("e7 Be5.").unwrap();
        let result = run_script(&mut sensor, &mut session);
        assert!(result.computer_move.is_some(), "opponent should be active");
    }
}
