#![cfg(not(target_os = "espidf"))]

use shakmaty::{
    Bitboard, ByColor, CastlingMode, Chess, Color, Move, Position, Role, Square, fen::Fen,
};
use unnamed_chess_project::feedback::{SquareFeedback, compute_feedback};
use unnamed_chess_project::game_logic::GameEngine;
use unnamed_chess_project::mock::ScriptedSensor;

fn position_from_fen(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .expect("valid FEN")
        .into_position(CastlingMode::Standard)
        .expect("valid position")
}

fn sensors_from_position(position: &Chess) -> ByColor<Bitboard> {
    let board = position.board();
    ByColor {
        white: board.by_color(Color::White),
        black: board.by_color(Color::Black),
    }
}

/// Helper: create engine + sensor from the default starting position.
fn setup() -> (GameEngine, ScriptedSensor) {
    (GameEngine::new(), ScriptedSensor::new())
}

/// Helper: create engine + sensor from a FEN string.
fn setup_fen(fen: &str) -> (GameEngine, ScriptedSensor) {
    let pos = position_from_fen(fen);
    let board = pos.board();
    let sensor =
        ScriptedSensor::from_bitboards(board.by_color(Color::White), board.by_color(Color::Black))
            .expect("no overlapping squares");
    let engine = GameEngine::from_position(pos);
    (engine, sensor)
}

/// Helper: tick sensor, feed engine, compute feedback using new pure function.
fn tick_feedback(
    engine: &mut GameEngine,
    sensor: &mut ScriptedSensor,
    prev: &mut ByColor<Bitboard>,
) -> unnamed_chess_project::feedback::BoardFeedback {
    let curr = sensor
        .tick()
        .expect("valid sensor state")
        .expect("pending batch");
    engine.tick(curr);
    let fb = compute_feedback(engine.position(), *prev, curr);
    *prev = curr;
    fb
}

// ---------------------------------------------------------------
// Simple move: lift → destinations shown → place → cleared
// ---------------------------------------------------------------

#[test]
fn lift_pawn_shows_destinations() {
    let (mut engine, mut sensor) = setup();
    let mut prev = sensor.read_positions();
    engine.tick(prev);
    sensor.push_script("e2.").expect("valid script");

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);

    assert_eq!(fb.get(Square::E2), Some(SquareFeedback::Origin));
    assert_eq!(fb.get(Square::E3), Some(SquareFeedback::Destination));
    assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Destination));
    assert_eq!(fb.get(Square::A1), None);
}

#[test]
fn completed_move_clears_feedback() {
    let (mut engine, mut sensor) = setup();
    let mut prev = sensor.read_positions();
    engine.tick(prev);
    sensor.push_script("e2 We4.").expect("valid script");

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);

    assert!(fb.is_empty());
}

#[test]
fn two_step_move_shows_destinations_then_clears() {
    let (mut engine, mut sensor) = setup();
    let mut prev = sensor.read_positions();
    engine.tick(prev);
    sensor.push_script("e2. We4.").expect("valid script");

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);
    assert_eq!(fb.get(Square::E2), Some(SquareFeedback::Origin));
    assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Destination));

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);
    assert!(fb.is_empty());
}

// ---------------------------------------------------------------
// Capture flow
// ---------------------------------------------------------------

#[test]
fn capture_remove_opponent_shows_origins() {
    let (mut engine, mut sensor) =
        setup_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1");
    let mut prev = sensor.read_positions();
    engine.tick(prev);
    sensor.push_script("d5.").expect("valid script");

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);

    assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Origin));
    assert_eq!(fb.get(Square::D5), Some(SquareFeedback::Destination));
}

#[test]
fn capture_lift_and_place_completes() {
    let (mut engine, mut sensor) =
        setup_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1");
    let mut prev = sensor.read_positions();
    engine.tick(prev);
    sensor.push_script("d5 e4 Wd5.").expect("valid script");

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);

    assert!(fb.is_empty());
    assert_eq!(
        engine.piece_at(Square::D5),
        Some(shakmaty::Piece {
            role: Role::Pawn,
            color: Color::White,
        }),
    );
}

#[test]
fn capture_two_step_shows_completion_square() {
    let (mut engine, mut sensor) =
        setup_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1");
    let mut prev = sensor.read_positions();
    engine.tick(prev);
    sensor.push_script("d5 e4.").expect("valid script");

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);
    assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Origin));
    assert_eq!(fb.get(Square::D5), Some(SquareFeedback::Destination));

    sensor.push_script("Wd5.").expect("valid script");
    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);
    assert!(fb.is_empty());
}

// ---------------------------------------------------------------
// Check feedback
// ---------------------------------------------------------------

#[test]
fn check_feedback_shown_when_idle() {
    let position =
        position_from_fen("rnbqkbnr/pppp2pp/8/4pp1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 0 1");
    let sensors = sensors_from_position(&position);

    let fb = compute_feedback(&position, sensors, sensors);

    assert_eq!(fb.get(Square::E8), Some(SquareFeedback::Check));
    assert_eq!(fb.get(Square::H5), Some(SquareFeedback::Checker));
}

#[test]
fn check_feedback_replaced_by_destinations_on_lift() {
    let position =
        position_from_fen("rnbqkbnr/pppp2pp/8/4pp1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 0 1");
    let prev = sensors_from_position(&position);
    let mut curr = prev;
    curr.black.toggle(Square::G8);

    let fb = compute_feedback(&position, prev, curr);

    assert_eq!(fb.get(Square::G8), Some(SquareFeedback::Origin));
    assert_eq!(fb.get(Square::E8), None);
}

// ---------------------------------------------------------------
// En passant
// ---------------------------------------------------------------

#[test]
fn en_passant_capture_feedback() {
    let position =
        position_from_fen("rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");
    let prev = sensors_from_position(&position);
    let mut curr = prev;
    curr.black.toggle(Square::D5);

    let fb = compute_feedback(&position, prev, curr);

    assert_eq!(fb.get(Square::E5), Some(SquareFeedback::Origin));
    assert_eq!(fb.get(Square::D6), Some(SquareFeedback::Destination));
}

#[test]
fn en_passant_full_sequence() {
    let (mut engine, mut sensor) =
        setup_fen("rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1");
    let mut prev = sensor.read_positions();
    engine.tick(prev);
    sensor.push_script("d5 e5 Wd6.").expect("valid script");

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);

    assert!(fb.is_empty());
    assert_eq!(
        engine.piece_at(Square::D6),
        Some(shakmaty::Piece {
            role: Role::Pawn,
            color: Color::White,
        }),
    );
    assert_eq!(engine.piece_at(Square::D5), None);
    assert_eq!(engine.piece_at(Square::E5), None);
}

// ---------------------------------------------------------------
// Castling
// ---------------------------------------------------------------

#[test]
fn kingside_castle_king_lifted_shows_king_target() {
    let position = position_from_fen("rnbqkbnr/pppppppp/8/8/8/5NP1/PPPPPPBP/RNBQK2R w KQkq - 0 1");
    let prev = sensors_from_position(&position);
    let mut curr = prev;
    curr.white.toggle(Square::E1);

    let fb = compute_feedback(&position, prev, curr);

    assert_eq!(fb.get(Square::E1), Some(SquareFeedback::Origin));
    assert_eq!(fb.get(Square::G1), Some(SquareFeedback::Destination));
    assert_eq!(fb.get(Square::F1), Some(SquareFeedback::Destination));
    assert_eq!(fb.get(Square::H1), None);
}

#[test]
fn kingside_castle_king_placed_shows_rook_guidance() {
    let position = position_from_fen("rnbqkbnr/pppppppp/8/8/8/5NP1/PPPPPPBP/RNBQK2R w KQkq - 0 1");
    let prev = sensors_from_position(&position);
    // King placed on g1, rook still on h1
    let mut curr = prev;
    curr.white.toggle(Square::E1); // king left
    curr.white.toggle(Square::G1); // king placed

    let fb = compute_feedback(&position, prev, curr);

    assert_eq!(fb.get(Square::H1), Some(SquareFeedback::Origin));
    assert_eq!(fb.get(Square::F1), Some(SquareFeedback::Destination));
}

#[test]
fn kingside_castling_completes() {
    let (mut engine, mut sensor) =
        setup_fen("rnbqkbnr/pppppppp/8/8/8/5NP1/PPPPPPBP/RNBQK2R w KQkq - 0 1");
    let mut prev = sensor.read_positions();
    engine.tick(prev);
    sensor.push_script("e1 h1. Wg1 Wf1.").expect("valid script");

    tick_feedback(&mut engine, &mut sensor, &mut prev); // lift
    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev); // place

    assert!(fb.is_empty());
    assert_eq!(
        engine.piece_at(Square::G1),
        Some(shakmaty::Piece {
            role: Role::King,
            color: Color::White,
        }),
    );
    assert_eq!(
        engine.piece_at(Square::F1),
        Some(shakmaty::Piece {
            role: Role::Rook,
            color: Color::White,
        }),
    );
}

// ---------------------------------------------------------------
// Promotion
// ---------------------------------------------------------------

#[test]
fn promotion_completes_as_queen() {
    let (mut engine, mut sensor) = setup_fen("k7/4P3/8/8/8/8/8/4K3 w - - 0 1");
    let mut prev = sensor.read_positions();
    engine.tick(prev);
    sensor.push_script("e7 We8.").expect("valid script");

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);

    assert_eq!(fb.get(Square::A8), Some(SquareFeedback::Check));
    assert_eq!(fb.get(Square::E8), Some(SquareFeedback::Checker));
}

// ---------------------------------------------------------------
// Multi-move sequence
// ---------------------------------------------------------------

#[test]
fn multi_move_sequence_alternates_feedback() {
    let (mut engine, mut sensor) = setup();
    let mut prev = sensor.read_positions();
    engine.tick(prev);

    sensor.push_script("e2. We4.").expect("valid script");
    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);
    assert_eq!(fb.get(Square::E2), Some(SquareFeedback::Origin));

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);
    assert!(fb.is_empty());

    sensor.push_script("e7. Be5.").expect("valid script");
    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);
    assert_eq!(fb.get(Square::E7), Some(SquareFeedback::Origin));

    let fb = tick_feedback(&mut engine, &mut sensor, &mut prev);
    assert!(fb.is_empty());
}

// ---------------------------------------------------------------
// Knight lift
// ---------------------------------------------------------------

#[test]
fn knight_lift_shows_only_valid_destinations() {
    let position = Chess::default();
    let prev = sensors_from_position(&position);
    let mut curr = prev;
    curr.white.toggle(Square::B1);

    let fb = compute_feedback(&position, prev, curr);

    assert_eq!(fb.get(Square::B1), Some(SquareFeedback::Origin));
    assert_eq!(fb.get(Square::A3), Some(SquareFeedback::Destination));
    assert_eq!(fb.get(Square::C3), Some(SquareFeedback::Destination));
    assert_eq!(fb.get(Square::D2), None);
}

// ---------------------------------------------------------------
// Game-over
// ---------------------------------------------------------------

#[test]
fn scholars_mate_shows_checkmate_feedback() {
    let (mut engine, mut sensor) = setup();
    let mut prev = sensor.read_positions();
    engine.tick(prev);

    sensor
        .push_script("e2 We4. e7 Be5. f1 Wc4. b8 Bc6. d1 Wh5. g8 Bf6. f7 h5 Wf7.")
        .expect("valid script");
    sensor
        .drain(|p| {
            prev = p;
            engine.tick(p);
        })
        .expect("valid sensor state");

    let curr = sensor.read_positions();
    let fb = compute_feedback(engine.position(), prev, curr);

    assert_eq!(fb.get(Square::E8), Some(SquareFeedback::Check));
    assert_eq!(fb.get(Square::F7), Some(SquareFeedback::Victory));
    assert_eq!(fb.get(Square::A1), Some(SquareFeedback::Victory));
    assert_eq!(fb.get(Square::A8), Some(SquareFeedback::Check));
}

#[test]
fn stalemate_shows_both_kings() {
    let position = position_from_fen("k7/8/1QK5/8/8/8/8/8 b - - 0 1");
    let sensors = sensors_from_position(&position);

    let fb = compute_feedback(&position, sensors, sensors);

    assert_eq!(fb.get(Square::A8), Some(SquareFeedback::Stalemate));
    assert_eq!(fb.get(Square::C6), Some(SquareFeedback::Stalemate));
    assert_eq!(fb.get(Square::A1), Some(SquareFeedback::Stalemate));
    assert_eq!(fb.get(Square::H8), Some(SquareFeedback::Stalemate));
}

// ---------------------------------------------------------------
// Opponent-move recovery
// ---------------------------------------------------------------

#[test]
fn opponent_move_recovery_shows_diff() {
    // After opponent's e7→e5 applied logically, physical board still has pawn on e7
    let mut position =
        position_from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
    let stale_sensors = sensors_from_position(&position);

    let mv = Move::Normal {
        role: Role::Pawn,
        from: Square::E7,
        capture: None,
        to: Square::E5,
        promotion: None,
    };
    position.play_unchecked(mv);

    let fb = compute_feedback(&position, stale_sensors, stale_sensors);

    assert_eq!(fb.get(Square::E7), Some(SquareFeedback::Capture));
    assert_eq!(fb.get(Square::E5), Some(SquareFeedback::Destination));
}

#[test]
fn opponent_move_recovery_clears_after_physical_matches() {
    let mut position =
        position_from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
    let mv = Move::Normal {
        role: Role::Pawn,
        from: Square::E7,
        capture: None,
        to: Square::E5,
        promotion: None,
    };
    position.play_unchecked(mv);
    let matching_sensors = sensors_from_position(&position);

    let fb = compute_feedback(&position, matching_sensors, matching_sensors);

    assert!(fb.is_empty());
}

#[test]
fn opponent_capture_shows_diff() {
    // Black knight captures white pawn on e4
    let mut position =
        position_from_fen("rnbqkb1r/pppppppp/5n2/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 2 1");
    let stale_sensors = sensors_from_position(&position);

    let mv = Move::Normal {
        role: Role::Knight,
        from: Square::F6,
        capture: Some(Role::Pawn),
        to: Square::E4,
        promotion: None,
    };
    position.play_unchecked(mv);

    let fb = compute_feedback(&position, stale_sensors, stale_sensors);

    assert_eq!(fb.get(Square::F6), Some(SquareFeedback::Capture));
    assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Capture));
}

#[test]
fn opponent_castle_shows_diff() {
    let mut position =
        position_from_fen("rnbqk2r/ppppppbp/5np1/8/4P3/5N2/PPPPBPPP/RNBQK2R b KQkq - 4 3");
    let stale_sensors = sensors_from_position(&position);

    let mv = Move::Castle {
        king: Square::E8,
        rook: Square::H8,
    };
    position.play_unchecked(mv);

    let fb = compute_feedback(&position, stale_sensors, stale_sensors);

    assert_eq!(fb.get(Square::E8), Some(SquareFeedback::Capture));
    assert_eq!(fb.get(Square::H8), Some(SquareFeedback::Capture));
    assert_eq!(fb.get(Square::G8), Some(SquareFeedback::Destination));
    assert_eq!(fb.get(Square::F8), Some(SquareFeedback::Destination));
}

#[test]
fn opponent_en_passant_shows_diff() {
    let mut position =
        position_from_fen("rnbqkbnr/pp1ppppp/8/8/2Pp4/8/PP1PPPPP/RNBQKBNR b KQkq c3 0 1");
    let stale_sensors = sensors_from_position(&position);

    let mv = Move::EnPassant {
        from: Square::D4,
        to: Square::C3,
    };
    position.play_unchecked(mv);

    let fb = compute_feedback(&position, stale_sensors, stale_sensors);

    assert_eq!(fb.get(Square::C4), Some(SquareFeedback::Capture));
    assert_eq!(fb.get(Square::D4), Some(SquareFeedback::Capture));
    assert_eq!(fb.get(Square::C3), Some(SquareFeedback::Destination));
}
