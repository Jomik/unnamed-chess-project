use shakmaty::{Bitboard, ByColor, Chess, Color, Move, Position};

use crate::feedback::{BoardFeedback, StatusKind, compute_feedback};
use crate::player::{Player, PlayerStatus};

/// Result of processing one sensor frame.
#[derive(Debug, Clone)]
pub struct TickResult {
    /// Computed board feedback (move guidance, recovery, status).
    pub feedback: BoardFeedback,
    /// The move committed during this tick, if any (by either player).
    pub last_move: Option<Move>,
}

/// Owns the game lifecycle: chess position and two players.
///
/// Encapsulates the per-tick orchestration: poll active player → apply move
/// → notify opponent → compute feedback.
pub struct GameSession {
    position: Chess,
    white: Box<dyn Player>,
    black: Box<dyn Player>,
    prev_sensors: ByColor<Bitboard>,
    illegal_move: bool,
}

impl GameSession {
    /// Create a session from the standard starting position.
    pub fn new(white: Box<dyn Player>, black: Box<dyn Player>) -> Self {
        Self::from_position(Chess::default(), white, black)
    }

    /// Create a session from a specific position.
    pub fn from_position(position: Chess, white: Box<dyn Player>, black: Box<dyn Player>) -> Self {
        let board = position.board();
        let prev_sensors = ByColor {
            white: board.by_color(Color::White),
            black: board.by_color(Color::Black),
        };
        Self {
            position,
            white,
            black,
            prev_sensors,
            illegal_move: false,
        }
    }

    /// Process one sensor frame.
    pub fn tick(&mut self, sensors: ByColor<Bitboard>) -> TickResult {
        let mut last_move = None;

        // Poll the active player.
        let turn = self.position.turn();
        let player = match turn {
            Color::White => &mut self.white,
            Color::Black => &mut self.black,
        };
        if let Some(mv) = player.poll_move(&self.position, sensors) {
            if self.position.legal_moves().contains(&mv) {
                self.position.play_unchecked(mv);
                last_move = Some(mv);

                let other = match turn {
                    Color::White => &mut self.black,
                    Color::Black => &mut self.white,
                };
                other.opponent_moved(&self.position, &mv);
            } else {
                log::warn!("Illegal move from {turn:?} player: {mv}");
                self.illegal_move = true;
            }
        }

        let mut feedback = compute_feedback(&self.position, self.prev_sensors, sensors);

        if self.illegal_move
            || self.white.status() == PlayerStatus::Error
            || self.black.status() == PlayerStatus::Error
        {
            feedback = feedback.with_merged_status(StatusKind::Failure);
        }

        self.prev_sensors = sensors;

        TickResult {
            feedback,
            last_move,
        }
    }

    /// Read-only access to the current chess position.
    #[inline]
    pub fn position(&self) -> &Chess {
        &self.position
    }
}

#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;
    use crate::mock::ScriptedSensor;
    use crate::player::{EmbeddedEngine, HumanPlayer};
    use shakmaty::{Color, Position, Square};

    fn run_script(sensor: &mut ScriptedSensor, session: &mut GameSession) -> TickResult {
        let mut last = None;
        sensor
            .drain(|p| {
                last = Some(session.tick(p));
            })
            .expect("script should parse");
        last.expect("script should produce at least one tick")
    }

    fn human_vs_human() -> (ScriptedSensor, GameSession) {
        let sensor = ScriptedSensor::new();
        let initial = sensor.read_positions();
        let session = GameSession::new(
            Box::new(HumanPlayer::new(initial)),
            Box::new(HumanPlayer::new(initial)),
        );
        (sensor, session)
    }

    fn human_vs_engine() -> (ScriptedSensor, GameSession) {
        let sensor = ScriptedSensor::new();
        let initial = sensor.read_positions();
        let session = GameSession::new(
            Box::new(HumanPlayer::new(initial)),
            Box::new(EmbeddedEngine::new(42)),
        );
        (sensor, session)
    }

    /// Test player that delays returning a move for N ticks.
    struct DelayedPlayer {
        delay_ticks: usize,
        ticks_remaining: usize,
        pending: Option<Move>,
    }

    impl DelayedPlayer {
        fn new(delay_ticks: usize) -> Self {
            Self {
                delay_ticks,
                ticks_remaining: 0,
                pending: None,
            }
        }
    }

    impl Player for DelayedPlayer {
        fn opponent_moved(&mut self, position: &Chess, _opponent_move: &Move) {
            let moves = position.legal_moves();
            self.pending = moves.into_iter().next();
            self.ticks_remaining = self.delay_ticks;
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

    /// Test player that reports an error.
    struct ErrorPlayer;

    impl Player for ErrorPlayer {
        fn poll_move(&mut self, _position: &Chess, _sensors: ByColor<Bitboard>) -> Option<Move> {
            None
        }
        fn opponent_moved(&mut self, _position: &Chess, _opponent_move: &Move) {}
        fn status(&self) -> PlayerStatus {
            PlayerStatus::Error
        }
    }

    #[test]
    fn human_move_advances_game() {
        let (mut sensor, mut session) = human_vs_human();

        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        assert!(result.last_move.is_some());
        assert_eq!(session.position().turn(), Color::Black);
    }

    #[test]
    fn human_vs_engine_produces_reply() {
        let (mut sensor, mut session) = human_vs_engine();

        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);
        assert!(result.last_move.is_some());
        // Human moved — it's now black's (engine's) turn
        assert_eq!(session.position().turn(), Color::Black);

        // Next tick: engine replies
        let result = session.tick(sensor.read_positions());
        assert!(result.last_move.is_some());
        assert_eq!(session.position().turn(), Color::White);
    }

    #[test]
    fn human_vs_human_no_auto_reply() {
        let (mut sensor, mut session) = human_vs_human();

        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        assert!(result.last_move.is_some());
        assert_eq!(session.position().turn(), Color::Black);
    }

    #[test]
    fn idle_tick_produces_empty_feedback() {
        let (sensor, mut session) = human_vs_human();

        let result = session.tick(sensor.read_positions());

        assert!(result.feedback.is_empty());
        assert!(result.last_move.is_none());
    }

    #[test]
    fn lifted_piece_shows_destinations() {
        let (mut sensor, mut session) = human_vs_human();

        sensor.push_script("e2.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        assert!(result.feedback.get(Square::E3).is_some());
        assert!(result.feedback.get(Square::E4).is_some());
    }

    #[test]
    fn recovery_feedback_after_engine_move() {
        let (mut sensor, mut session) = human_vs_engine();

        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);
        assert!(result.last_move.is_some());

        // Physical board hasn't been updated for engine's move
        let result = session.tick(sensor.read_positions());
        assert!(!result.feedback.is_empty());
    }

    #[test]
    fn position_accessor() {
        let (_sensor, session) = human_vs_human();
        assert_eq!(session.position().turn(), Color::White);
    }

    #[test]
    fn from_position_starts_at_given_position() {
        let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
        let position: Chess = fen
            .parse::<shakmaty::fen::Fen>()
            .unwrap()
            .into_position(shakmaty::CastlingMode::Standard)
            .unwrap();

        let board = position.board();
        let sensors = ByColor {
            white: board.by_color(Color::White),
            black: board.by_color(Color::Black),
        };
        let session = GameSession::from_position(
            position,
            Box::new(HumanPlayer::new(sensors)),
            Box::new(EmbeddedEngine::new(42)),
        );

        assert_eq!(session.position().turn(), Color::Black);
    }

    #[test]
    fn delayed_player_returns_move_after_ticks() {
        let sensor = ScriptedSensor::new();
        let initial = sensor.read_positions();
        let mut session = GameSession::new(
            Box::new(HumanPlayer::new(initial)),
            Box::new(DelayedPlayer::new(2)),
        );

        let mut sensor = ScriptedSensor::new();
        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        // Move detected but opponent hasn't replied yet
        assert!(result.last_move.is_some());
        // DelayedPlayer delays, so it's still black's turn
        assert_eq!(session.position().turn(), Color::Black);

        // Tick 1-2: still waiting
        let _ = session.tick(sensor.read_positions());
        let _ = session.tick(sensor.read_positions());

        // Tick 3: move arrives
        let result = session.tick(sensor.read_positions());
        assert!(result.last_move.is_some());
        assert_eq!(session.position().turn(), Color::White);
    }

    #[test]
    fn error_player_produces_failure_feedback() {
        let sensor = ScriptedSensor::new();
        let initial = sensor.read_positions();
        let mut session =
            GameSession::new(Box::new(HumanPlayer::new(initial)), Box::new(ErrorPlayer));

        let result = session.tick(sensor.read_positions());
        assert_eq!(result.feedback.status(), Some(StatusKind::Failure));
    }
}
