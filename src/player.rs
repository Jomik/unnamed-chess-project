use shakmaty::{Bitboard, ByColor, Chess, Move};

/// Player health status, checked by the session each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerStatus {
    /// Normal operation.
    Active,
    /// Unrecoverable error (e.g. Lichess disconnect).
    Error,
    /// External game termination (e.g. Lichess resign/timeout).
    GameOver,
}

/// A chess player — either human (detecting moves from sensors) or computer (computing moves).
pub trait Player {
    /// Return a move if one is detected/ready. Called every tick for the active player.
    ///
    /// `position` is the current chess position (owned by the session).
    /// `sensors` is the current physical board state from hall-effect sensors.
    /// Computer players ignore `sensors`.
    fn poll_move(&mut self, position: &Chess, sensors: ByColor<Bitboard>) -> Option<Move>;

    /// Notification that the opponent just played.
    ///
    /// `position` is the post-move state. Computer players start thinking here.
    /// Human players can ignore this — recovery LEDs guide the physical execution.
    fn opponent_moved(&mut self, position: &Chess, opponent_move: &Move);

    /// Player health. Checked by session each tick.
    fn status(&self) -> PlayerStatus {
        PlayerStatus::Active
    }
}

/// Human player that detects moves from physical board sensor state.
///
/// Compares sensor bitboards against the chess position's expected board
/// to find which legal move matches the physical piece placement.
#[derive(Debug, Clone)]
pub struct HumanPlayer {
    last_sensors: ByColor<Bitboard>,
}

impl HumanPlayer {
    /// Create a new human player with the initial sensor state.
    pub fn new(initial_sensors: ByColor<Bitboard>) -> Self {
        Self {
            last_sensors: initial_sensors,
        }
    }
}

impl Player for HumanPlayer {
    fn poll_move(&mut self, position: &Chess, sensors: ByColor<Bitboard>) -> Option<Move> {
        use shakmaty::{Position, Role};

        if sensors == self.last_sensors {
            return None;
        }
        self.last_sensors = sensors;

        let turn = position.turn();
        let expected_our = position.board().by_color(turn);
        let our_current = sensors[turn];
        let our_placed = our_current & !expected_our;

        if our_placed.is_empty() {
            return None;
        }

        let current_combined = sensors.white | sensors.black;

        for mv in position.legal_moves() {
            // Castling: mv.to() is the rook origin, not king destination,
            // so skip the destination pre-filter.
            if !matches!(mv, Move::Castle { .. }) && !our_placed.contains(mv.to()) {
                continue;
            }

            // Only allow queen promotions (no piece-selection mechanism on hardware).
            if mv.promotion().is_some_and(|role| role != Role::Queen) {
                continue;
            }

            let mut after = position.clone();
            after.play_unchecked(mv);
            if after.board().occupied() == current_combined {
                return Some(mv);
            }
        }

        None
    }

    fn opponent_moved(&mut self, _position: &Chess, _opponent_move: &Move) {
        // No-op — recovery LEDs guide the human to physically execute the opponent's move.
    }
}

#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;
    use crate::mock::ScriptedSensor;
    use shakmaty::{Chess, Color, Position, Role, Square};

    fn position_from_fen(fen: &str) -> Chess {
        use shakmaty::{CastlingMode, fen::Fen};
        fen.parse::<Fen>()
            .expect("invalid FEN")
            .into_position(CastlingMode::Standard)
            .expect("invalid position")
    }

    fn run_script(
        player: &mut HumanPlayer,
        sensor: &mut ScriptedSensor,
        position: &mut Chess,
    ) -> Vec<Move> {
        let mut moves = Vec::new();
        sensor
            .drain(|p| {
                if let Some(mv) = player.poll_move(position, p) {
                    position.play_unchecked(mv);
                    moves.push(mv);
                }
            })
            .expect("script should parse");
        moves
    }

    fn run_script_one(
        player: &mut HumanPlayer,
        sensor: &mut ScriptedSensor,
        position: &mut Chess,
    ) -> Option<Move> {
        let moves = run_script(player, sensor, position);
        assert!(
            moves.len() <= 1,
            "expected at most 1 move, got {}",
            moves.len()
        );
        moves.into_iter().next()
    }

    #[test]
    fn detects_simple_pawn_move() {
        let mut position = Chess::default();
        let mut sensor = ScriptedSensor::new();
        let mut player = HumanPlayer::new(sensor.read_positions());

        sensor.push_script("e2 We4.").unwrap();
        let mv = run_script_one(&mut player, &mut sensor, &mut position);

        assert!(mv.is_some(), "should detect e2-e4");
        let mv = mv.unwrap();
        assert_eq!(mv.from(), Some(Square::E2));
        assert_eq!(mv.to(), Square::E4);
    }

    #[test]
    fn detects_knight_move() {
        let mut position = Chess::default();
        let mut sensor = ScriptedSensor::new();
        let mut player = HumanPlayer::new(sensor.read_positions());

        sensor.push_script("g1 Wf3.").unwrap();
        let mv = run_script_one(&mut player, &mut sensor, &mut position);

        assert!(mv.is_some());
        assert_eq!(mv.unwrap().role(), Role::Knight);
    }

    #[test]
    fn ignores_illegal_move() {
        let mut position = Chess::default();
        let mut sensor = ScriptedSensor::new();
        let mut player = HumanPlayer::new(sensor.read_positions());

        sensor.push_script("e2 We5.").unwrap();
        let mv = run_script_one(&mut player, &mut sensor, &mut position);

        assert!(mv.is_none());
    }

    #[test]
    fn detects_capture() {
        let mut position =
            position_from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");
        let mut sensor = ScriptedSensor::from_bitboards(
            position.board().by_color(Color::White),
            position.board().by_color(Color::Black),
        )
        .unwrap();
        let mut player = HumanPlayer::new(sensor.read_positions());

        sensor.push_script("d5 e4 Wd5.").unwrap();
        let mv = run_script_one(&mut player, &mut sensor, &mut position);

        assert!(mv.is_some());
        assert!(mv.unwrap().is_capture());
    }

    #[test]
    fn detects_kingside_castle() {
        let mut position = position_from_fen(
            "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 0 1",
        );
        let mut sensor = ScriptedSensor::from_bitboards(
            position.board().by_color(Color::White),
            position.board().by_color(Color::Black),
        )
        .unwrap();
        let mut player = HumanPlayer::new(sensor.read_positions());

        sensor.push_script("e1 h1. Wg1 Wf1.").unwrap();
        let mv = run_script_one(&mut player, &mut sensor, &mut position);

        assert!(mv.is_some());
        assert!(matches!(mv.unwrap(), Move::Castle { .. }));
    }

    #[test]
    fn detects_en_passant() {
        let mut position =
            position_from_fen("rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");
        let mut sensor = ScriptedSensor::from_bitboards(
            position.board().by_color(Color::White),
            position.board().by_color(Color::Black),
        )
        .unwrap();
        let mut player = HumanPlayer::new(sensor.read_positions());

        sensor.push_script("e5 d5 Wd6.").unwrap();
        let mv = run_script_one(&mut player, &mut sensor, &mut position);

        assert!(mv.is_some());
        assert!(matches!(mv.unwrap(), Move::EnPassant { .. }));
    }

    #[test]
    fn detects_promotion() {
        let mut position =
            position_from_fen("r1bqkbnr/pPpppppp/2n5/8/8/8/PP1PPPPP/RNBQKBNR w KQkq - 0 1");
        let mut sensor = ScriptedSensor::from_bitboards(
            position.board().by_color(Color::White),
            position.board().by_color(Color::Black),
        )
        .unwrap();
        let mut player = HumanPlayer::new(sensor.read_positions());

        sensor.push_script("b7 Wb8.").unwrap();
        let mv = run_script_one(&mut player, &mut sensor, &mut position);

        assert!(mv.is_some());
        assert_eq!(mv.unwrap().promotion(), Some(Role::Queen));
    }

    #[test]
    fn detects_multi_move_sequence() {
        let mut position = Chess::default();
        let mut sensor = ScriptedSensor::new();
        let mut player = HumanPlayer::new(sensor.read_positions());

        sensor
            .push_script("e2 We4. e7 Be5. g1 Wf3. b8 Bc6.")
            .unwrap();
        let moves = run_script(&mut player, &mut sensor, &mut position);

        assert_eq!(moves.len(), 4);
    }

    #[test]
    fn no_change_returns_none() {
        let position = Chess::default();
        let sensor = ScriptedSensor::new();
        let mut player = HumanPlayer::new(sensor.read_positions());

        let mv = player.poll_move(&position, sensor.read_positions());
        assert!(mv.is_none());
    }

    #[test]
    fn opponent_moved_is_noop() {
        let position = Chess::default();
        let sensor = ScriptedSensor::new();
        let mut player = HumanPlayer::new(sensor.read_positions());
        let dummy_move = Move::Normal {
            role: Role::Pawn,
            from: Square::E2,
            to: Square::E4,
            capture: None,
            promotion: None,
        };

        player.opponent_moved(&position, &dummy_move);
        assert_eq!(player.status(), PlayerStatus::Active);
    }
}
