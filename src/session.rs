use shakmaty::{Bitboard, ByColor, Chess, Color, Move, Position};

use crate::board_api::GameStatus;
use crate::feedback::{BoardFeedback, StatusKind, compute_feedback, compute_state_feedback};
use crate::player::{GameAction, Player, PlayerStatus};

#[derive(Debug, Clone)]
pub struct TickResult {
    pub feedback: BoardFeedback,
    pub last_move: Option<Move>,
}

/// Per-tick orchestration: poll active player -> apply move -> notify opponent -> compute feedback.
pub struct GameSession {
    position: Chess,
    white: Box<dyn Player>,
    black: Box<dyn Player>,
    reference_sensors: ByColor<Bitboard>,
    illegal_move: bool,
    resigned: Option<Color>,
}

impl GameSession {
    pub fn new(white: Box<dyn Player>, black: Box<dyn Player>) -> Self {
        Self::from_position(Chess::default(), white, black)
    }

    pub fn from_position(position: Chess, white: Box<dyn Player>, black: Box<dyn Player>) -> Self {
        let board = position.board();
        let reference_sensors = ByColor {
            white: board.by_color(Color::White),
            black: board.by_color(Color::Black),
        };
        Self {
            position,
            white,
            black,
            reference_sensors,
            illegal_move: false,
            resigned: None,
        }
    }

    /// Returns `true` if the resignation was accepted, `false` if rejected
    /// (e.g. resigning on behalf of a non-interactive player).
    pub fn resign(&mut self, color: Color) -> bool {
        if self.is_game_over() {
            return false;
        }

        let player = match color {
            Color::White => &self.white,
            Color::Black => &self.black,
        };
        if !player.is_interactive() {
            log::warn!("Rejecting resign for non-interactive {color:?} player");
            return false;
        }

        self.resigned = Some(color);
        let action = GameAction::Resign(color);
        // Broadcast to both players. Currently all players inherit the
        // default no-op; LichessOpponent will override this in a later task.
        self.white.notify(&action);
        self.black.notify(&action);
        true
    }

    pub fn is_game_over(&self) -> bool {
        self.resigned.is_some() || self.position.legal_moves().is_empty()
    }

    pub fn game_state(&self) -> GameStatus {
        if let Some(color) = self.resigned {
            return GameStatus::Resigned { color };
        }
        let legal_moves = self.position.legal_moves();
        if legal_moves.is_empty() {
            if self.position.is_check() {
                GameStatus::Checkmate {
                    loser: self.position.turn(),
                }
            } else {
                GameStatus::Stalemate
            }
        } else {
            GameStatus::InProgress
        }
    }

    pub fn tick(&mut self, sensors: ByColor<Bitboard>) -> TickResult {
        // Short-circuit: game already ended.
        if self.is_game_over() {
            return TickResult {
                feedback: compute_state_feedback(&self.position, sensors),
                last_move: None,
            };
        }

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

        // Choose reference: stored for interactive players, current for non-interactive
        // (suppresses move guidance during computer turns)
        let active_is_interactive = match self.position.turn() {
            Color::White => self.white.is_interactive(),
            Color::Black => self.black.is_interactive(),
        };
        let mut feedback = if active_is_interactive {
            compute_feedback(&self.position, sensors, self.reference_sensors)
        } else {
            compute_state_feedback(&self.position, sensors)
        };

        if self.illegal_move
            || self.white.status() == PlayerStatus::Error
            || self.black.status() == PlayerStatus::Error
        {
            feedback = feedback.with_merged_status(StatusKind::Failure);
        }

        // Update reference when physical board matches expected position
        let expected = self.position.board();
        if expected.by_color(Color::White) == sensors.white
            && expected.by_color(Color::Black) == sensors.black
        {
            self.reference_sensors = sensors;
        }

        TickResult {
            feedback,
            last_move,
        }
    }

    #[inline]
    pub fn position(&self) -> &Chess {
        &self.position
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{EmbeddedEngine, HumanPlayer};
    use crate::testutil::ScriptedSensor;
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

    fn engine_vs_human() -> (ScriptedSensor, GameSession) {
        let sensor = ScriptedSensor::new();
        let initial = sensor.read_positions();
        let session = GameSession::new(
            Box::new(EmbeddedEngine::new(42)),
            Box::new(HumanPlayer::new(initial)),
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
    fn lifted_piece_guidance_persists_across_ticks() {
        use crate::feedback::SquareFeedback;

        let (mut sensor, mut session) = human_vs_human();

        // Tick 1: lift e2 pawn
        sensor.push_script("e2.").unwrap();
        let result = run_script(&mut sensor, &mut session);
        assert_eq!(
            result.feedback.get(Square::E2),
            Some(SquareFeedback::Origin),
            "tick 1: lifted square should be Origin"
        );
        assert_eq!(
            result.feedback.get(Square::E3),
            Some(SquareFeedback::Destination),
            "tick 1: legal destination should show"
        );
        assert_eq!(
            result.feedback.get(Square::E4),
            Some(SquareFeedback::Destination),
            "tick 1: legal destination e4 should show"
        );

        // Tick 2: same sensors (piece still held) — guidance must persist
        let result = session.tick(sensor.read_positions());
        assert_eq!(
            result.feedback.get(Square::E2),
            Some(SquareFeedback::Origin),
            "tick 2: lifted square should still be Origin, not recovery"
        );
        assert_eq!(
            result.feedback.get(Square::E3),
            Some(SquareFeedback::Destination),
            "tick 2: legal destination should still show"
        );
        assert_eq!(
            result.feedback.get(Square::E4),
            Some(SquareFeedback::Destination),
            "tick 2: legal destination e4 should still show"
        );
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

    #[test]
    fn no_capture_guidance_during_opponent_move_recovery() {
        use crate::feedback::SquareFeedback;

        let (mut sensor, mut session) = human_vs_engine();

        // Human plays e2→e4
        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);
        assert!(result.last_move.is_some(), "human move should be detected");

        // Engine replies immediately on next tick; physical board is still post-human-move
        let result = session.tick(sensor.read_positions());
        assert!(result.last_move.is_some(), "engine should reply");
        // It is now white's (human's) turn again

        // Physical board hasn't been updated for engine's move — should show recovery guidance
        let result = session.tick(sensor.read_positions());
        assert!(
            !result.feedback.is_empty(),
            "should show recovery guidance while physical board is stale"
        );

        // Recovery feedback must only contain Destination or Capture — no Origin or move guidance
        for (sq, fb) in result.feedback.squares() {
            assert!(
                fb == SquareFeedback::Destination || fb == SquareFeedback::Capture,
                "during recovery, square {sq:?} should be Destination or Capture, got {fb:?}"
            );
        }
    }

    #[test]
    fn no_guidance_when_non_interactive_player_active() {
        use crate::feedback::SquareFeedback;

        struct NonInteractiveDelayed;
        impl Player for NonInteractiveDelayed {
            fn poll_move(
                &mut self,
                _position: &Chess,
                _sensors: ByColor<Bitboard>,
            ) -> Option<Move> {
                None // never moves — simulates a thinking engine
            }
            fn is_interactive(&self) -> bool {
                false
            }
        }

        let sensor = ScriptedSensor::new();
        let initial = sensor.read_positions();
        let mut session = GameSession::new(
            Box::new(HumanPlayer::new(initial)),
            Box::new(NonInteractiveDelayed),
        );

        // Human plays e2→e4
        let mut sensor = ScriptedSensor::new();
        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);
        assert!(result.last_move.is_some(), "human move should be detected");
        // It is now Black's turn (non-interactive, never moves)

        // Human physically lifts the a2 pawn during the computer's turn
        sensor.push_script("a2.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        // Should show recovery (place piece back), not move guidance
        assert_eq!(
            result.feedback.get(Square::A2),
            Some(SquareFeedback::Destination),
            "a2 should show Destination (recovery: put piece back), not move guidance"
        );

        // No Origin feedback should appear — that would mean move guidance was shown
        for (sq, fb) in result.feedback.squares() {
            assert_ne!(
                fb,
                SquareFeedback::Origin,
                "no Origin feedback should appear during non-interactive player's turn, \
                 but {sq:?} has Origin"
            );
        }
    }

    #[test]
    fn is_interactive_returns_correct_values() {
        let sensors = ByColor {
            white: Bitboard::EMPTY,
            black: Bitboard::EMPTY,
        };
        assert!(
            HumanPlayer::new(sensors).is_interactive(),
            "HumanPlayer should be interactive"
        );
        assert!(
            !EmbeddedEngine::new(42).is_interactive(),
            "EmbeddedEngine should not be interactive"
        );
    }

    #[test]
    fn engine_as_white_moves_on_first_tick() {
        let (sensor, mut session) = engine_vs_human();

        // Engine is white — should produce a move on the very first tick.
        let result = session.tick(sensor.read_positions());
        assert!(
            result.last_move.is_some(),
            "engine playing white should move on first tick"
        );
        assert_eq!(
            session.position().turn(),
            Color::Black,
            "after engine's first move, it should be black's turn"
        );
    }

    // ── resign / is_game_over / game_state ────────────────────────────────────

    #[test]
    fn resign_sets_game_over() {
        let (_sensor, mut session) = human_vs_human();

        assert!(!session.is_game_over(), "new game should not be over");
        assert!(session.resign(Color::White));
        assert!(
            session.is_game_over(),
            "game should be over after resignation"
        );
    }

    #[test]
    fn game_state_in_progress_for_new_session() {
        use crate::board_api::GameStatus;

        let (_sensor, session) = human_vs_human();
        let state = session.game_state();

        assert_eq!(state, GameStatus::InProgress);
    }

    #[test]
    fn game_state_resignation_after_resign() {
        use crate::board_api::GameStatus;

        let (_sensor, mut session) = human_vs_human();
        assert!(session.resign(Color::Black));

        let state = session.game_state();
        assert_eq!(
            state,
            GameStatus::Resigned {
                color: Color::Black
            }
        );
    }

    #[test]
    fn tick_short_circuits_when_resigned() {
        let (mut sensor, mut session) = human_vs_human();

        assert!(session.resign(Color::White));

        // Even pushing a human move script should not advance the position.
        sensor.push_script("e2 We4.").unwrap();
        let result = run_script(&mut sensor, &mut session);

        assert!(
            result.last_move.is_none(),
            "no move should be processed after resignation"
        );
        assert_eq!(
            session.position().turn(),
            Color::White,
            "turn should not advance after resignation"
        );
    }

    #[test]
    fn resign_rejected_for_non_interactive_player() {
        let (_sensor, mut session) = human_vs_engine();
        // Black is EmbeddedEngine (non-interactive) — resign should be rejected
        assert!(
            !session.resign(Color::Black),
            "resign should be rejected for non-interactive player"
        );
        assert!(
            !session.is_game_over(),
            "game should not be over after rejected resign"
        );
    }

    #[test]
    fn resign_rejected_when_game_already_over() {
        let (_sensor, mut session) = human_vs_human();
        assert!(session.resign(Color::White));
        // Second resign should be rejected — game is already over
        assert!(
            !session.resign(Color::Black),
            "resign should be rejected when game is already over"
        );
    }

    #[test]
    fn game_state_checkmate() {
        use crate::board_api::GameStatus;

        // Fool's mate: white is checkmated
        let fen = "rnb1kbnr/pppp1ppp/4p3/8/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
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

        assert!(
            session.is_game_over(),
            "game should be over in checkmate position"
        );
        let state = session.game_state();
        assert_eq!(
            state,
            GameStatus::Checkmate {
                loser: Color::White
            },
            "game_state should report Checkmate with white as loser"
        );
    }

    #[test]
    fn game_state_stalemate() {
        use crate::board_api::GameStatus;

        // Classic stalemate: black king trapped, no legal moves, not in check
        let fen = "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1";
        let position: Chess = fen
            .parse::<shakmaty::fen::Fen>()
            .unwrap()
            .into_position(shakmaty::CastlingMode::Standard)
            .unwrap();

        // Verify this is actually a stalemate before testing
        let legal_moves = position.legal_moves();
        assert!(
            legal_moves.is_empty(),
            "test FEN should have no legal moves, but got {:?}",
            legal_moves
        );
        assert!(!position.is_check(), "test FEN should not be in check");

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

        assert!(
            session.is_game_over(),
            "game should be over in stalemate position"
        );
        let state = session.game_state();
        assert_eq!(
            state,
            GameStatus::Stalemate,
            "game_state should report Stalemate status"
        );
    }
}
