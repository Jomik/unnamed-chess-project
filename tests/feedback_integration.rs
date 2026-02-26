#![cfg(not(target_os = "espidf"))]

use shakmaty::{CastlingMode, Chess, Color, Position, Role, Square, fen::Fen};
use unnamed_chess_project::feedback::{SquareFeedback, compute_feedback};
use unnamed_chess_project::game_logic::GameEngine;
use unnamed_chess_project::mock::ScriptedSensor;

/// Helper: create engine + sensor from the default starting position.
fn setup() -> (GameEngine, ScriptedSensor) {
    let engine = GameEngine::new();
    let sensor = ScriptedSensor::new();
    (engine, sensor)
}

/// Helper: create engine + sensor from a FEN string.
fn setup_fen(fen: &str) -> (GameEngine, ScriptedSensor) {
    let pos: Chess = fen
        .parse::<Fen>()
        .expect("valid FEN")
        .into_position(CastlingMode::Standard)
        .expect("valid position");
    let board = pos.board();
    let sensor =
        ScriptedSensor::from_bitboards(board.by_color(Color::White), board.by_color(Color::Black))
            .expect("no overlapping squares");
    let engine = GameEngine::from_position(pos);
    (engine, sensor)
}

/// Helper: advance sensor one tick and feed into engine.
fn tick(
    engine: &mut GameEngine,
    sensor: &mut ScriptedSensor,
) -> unnamed_chess_project::game_logic::GameState {
    let positions = sensor
        .tick()
        .expect("valid sensor state")
        .expect("pending batch");
    engine.tick(positions)
}

// ---------------------------------------------------------------
// Simple move: lift → destinations shown → place → cleared
// ---------------------------------------------------------------

#[test]
fn lift_pawn_shows_destinations() {
    let (mut engine, mut sensor) = setup();
    sensor.push_script("e2.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    assert_eq!(
        fb.get(Square::E2),
        Some(SquareFeedback::Origin),
        "lifted square should be Origin"
    );
    assert_eq!(fb.get(Square::E3), Some(SquareFeedback::Destination),);
    assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Destination),);
    // Unrelated square has no feedback
    assert_eq!(fb.get(Square::A1), None);
}

#[test]
fn completed_move_clears_feedback() {
    let (mut engine, mut sensor) = setup();
    sensor.push_script("e2 We4.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    assert!(
        fb.is_empty(),
        "feedback should be empty after completing a move"
    );
}

#[test]
fn two_step_move_shows_destinations_then_clears() {
    let (mut engine, mut sensor) = setup();
    sensor.push_script("e2. We4.").expect("valid script");

    // Step 1: lift
    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);
    assert_eq!(fb.get(Square::E2), Some(SquareFeedback::Origin));
    assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Destination));

    // Step 2: place
    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);
    assert!(fb.is_empty(), "feedback clear after placement");
}

// ---------------------------------------------------------------
// Capture flow
// ---------------------------------------------------------------

#[test]
fn capture_remove_opponent_shows_origins() {
    // Position: white pawn e4, black pawn d5, white to move
    let (mut engine, mut sensor) = setup_fen(
        "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR \
         w KQkq - 0 1",
    );
    // Remove opponent pawn from d5
    sensor.push_script("d5.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    // e4 pawn can capture on d5
    assert_eq!(
        fb.get(Square::E4),
        Some(SquareFeedback::Origin),
        "e4 pawn should be shown as captor"
    );
    assert_eq!(
        fb.get(Square::D5),
        Some(SquareFeedback::Destination),
        "d5 should be capture destination"
    );
}

#[test]
fn capture_lift_and_place_completes() {
    let (mut engine, mut sensor) = setup_fen(
        "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR \
         w KQkq - 0 1",
    );
    // Remove d5, lift e4, place on d5 — all at once
    sensor.push_script("d5 e4 Wd5.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    assert!(fb.is_empty(), "feedback clear after capture");
    assert_eq!(
        engine.piece_at(Square::D5),
        Some(shakmaty::Piece {
            role: Role::Pawn,
            color: Color::White,
        }),
        "white pawn should be on d5"
    );
}

#[test]
fn capture_two_step_shows_completion_square() {
    let (mut engine, mut sensor) = setup_fen(
        "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR \
         w KQkq - 0 1",
    );
    // Step 1: remove opponent piece, lift our piece
    sensor.push_script("d5 e4.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    assert_eq!(fb.get(Square::E4), Some(SquareFeedback::Origin),);
    assert_eq!(
        fb.get(Square::D5),
        Some(SquareFeedback::Destination),
        "capture completion square"
    );

    // Step 2: place
    sensor.push_script("Wd5.").expect("valid script");
    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    assert!(fb.is_empty(), "feedback clear after capture done");
}

// ---------------------------------------------------------------
// Check feedback
// ---------------------------------------------------------------

#[test]
fn check_feedback_shown_when_idle() {
    // Scholar's mate position: black king in check from Qh5
    let (mut engine, sensor) = setup_fen(
        "rnbqkbnr/pppp2pp/8/4pp1Q/4P3/8/PPPP1PPP/\
         RNB1KBNR b KQkq - 0 1",
    );

    // Tick with current positions (no changes)
    let state = engine.tick(sensor.read_positions());
    let fb = compute_feedback(&state);

    assert_eq!(
        fb.get(Square::E8),
        Some(SquareFeedback::Check),
        "king square should show Check"
    );
    assert_eq!(
        fb.get(Square::H5),
        Some(SquareFeedback::Checker),
        "queen should show Checker"
    );
}

#[test]
fn check_feedback_replaced_by_destinations_on_lift() {
    // Black king in check from Qh5, black lifts g8 knight
    let (mut engine, mut sensor) = setup_fen(
        "rnbqkbnr/pppp2pp/8/4pp1Q/4P3/8/PPPP1PPP/\
         RNB1KBNR b KQkq - 0 1",
    );

    sensor.push_script("g8.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    // Lifting shows move destinations, not check feedback
    assert_eq!(fb.get(Square::G8), Some(SquareFeedback::Origin),);
    assert_eq!(
        fb.get(Square::E8),
        None,
        "check highlight gone when piece lifted"
    );
}

// ---------------------------------------------------------------
// En passant
// ---------------------------------------------------------------

#[test]
fn en_passant_capture_feedback() {
    // White pawn on e5, black just played d7-d5
    let (mut engine, mut sensor) = setup_fen(
        "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/\
         RNBQKBNR w KQkq d6 0 1",
    );

    // Remove the en passant pawn (d5)
    sensor.push_script("d5.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    // e5 pawn can capture en passant to d6
    assert_eq!(fb.get(Square::E5), Some(SquareFeedback::Origin),);
    assert_eq!(
        fb.get(Square::D6),
        Some(SquareFeedback::Destination),
        "en passant destination d6"
    );
}

#[test]
fn en_passant_full_sequence() {
    let (mut engine, mut sensor) = setup_fen(
        "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/\
         RNBQKBNR w KQkq d6 0 1",
    );

    // Remove d5, lift e5, place on d6
    sensor.push_script("d5 e5 Wd6.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    assert!(fb.is_empty(), "feedback clear after en passant");
    assert_eq!(
        engine.piece_at(Square::D6),
        Some(shakmaty::Piece {
            role: Role::Pawn,
            color: Color::White,
        }),
    );
    assert_eq!(engine.piece_at(Square::D5), None, "captured pawn gone");
    assert_eq!(engine.piece_at(Square::E5), None, "origin empty");
}

// ---------------------------------------------------------------
// Castling
// ---------------------------------------------------------------

#[test]
fn kingside_castling_feedback() {
    // White can castle kingside
    let (mut engine, mut sensor) = setup_fen(
        "rnbqkbnr/pppppppp/8/8/8/5NP1/PPPPPPBP/\
         RNBQK2R w KQkq - 0 1",
    );

    // Lift king
    sensor.push_script("e1.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    assert_eq!(fb.get(Square::E1), Some(SquareFeedback::Origin),);
    // Castle move's to() is the rook origin (h1), so
    // feedback shows h1 as the destination, not g1.
    assert_eq!(
        fb.get(Square::H1),
        Some(SquareFeedback::Destination),
        "castling destination shows rook origin h1"
    );
}

#[test]
fn kingside_castling_completes() {
    let (mut engine, mut sensor) = setup_fen(
        "rnbqkbnr/pppppppp/8/8/8/5NP1/PPPPPPBP/\
         RNBQK2R w KQkq - 0 1",
    );

    // King to g1, rook to f1 (two-handed)
    sensor.push_script("e1 h1. Wg1 Wf1.").expect("valid script");

    tick(&mut engine, &mut sensor); // lift
    let state = tick(&mut engine, &mut sensor); // place
    let fb = compute_feedback(&state);

    assert!(fb.is_empty(), "feedback clear after castling");
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
    // White pawn on e7, black king on a8, white to move
    let (mut engine, mut sensor) = setup_fen("k7/4P3/8/8/8/8/8/4K3 w - - 0 1");

    sensor.push_script("e7 We8.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);
    // After promotion to queen on e8, the queen gives
    // check along rank 8 to the black king on a8.
    assert_eq!(
        fb.get(Square::A8),
        Some(SquareFeedback::Check),
        "black king in check after promotion"
    );
    assert_eq!(
        fb.get(Square::E8),
        Some(SquareFeedback::Checker),
        "promoted queen is checker"
    );
    assert_eq!(
        engine.piece_at(Square::E8),
        Some(shakmaty::Piece {
            role: Role::Queen,
            color: Color::White,
        }),
        "pawn promoted to queen"
    );
}

// ---------------------------------------------------------------
// Multi-move sequence: feedback correct across turns
// ---------------------------------------------------------------

#[test]
fn multi_move_sequence_alternates_feedback() {
    let (mut engine, mut sensor) = setup();

    // White: e2-e4 (two steps)
    sensor.push_script("e2. We4.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);
    assert_eq!(fb.get(Square::E2), Some(SquareFeedback::Origin));

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);
    assert!(fb.is_empty(), "clear after white moves");

    // Black: e7-e5 (two steps)
    sensor.push_script("e7. Be5.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);
    assert_eq!(
        fb.get(Square::E7),
        Some(SquareFeedback::Origin),
        "black pawn lifted"
    );

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);
    assert!(fb.is_empty(), "clear after black moves");
}

// ---------------------------------------------------------------
// Knight lift shows correct destinations
// ---------------------------------------------------------------

#[test]
fn knight_lift_shows_only_valid_destinations() {
    let (mut engine, mut sensor) = setup();
    sensor.push_script("b1.").expect("valid script");

    let state = tick(&mut engine, &mut sensor);
    let fb = compute_feedback(&state);

    assert_eq!(fb.get(Square::B1), Some(SquareFeedback::Origin),);
    assert_eq!(fb.get(Square::A3), Some(SquareFeedback::Destination),);
    assert_eq!(fb.get(Square::C3), Some(SquareFeedback::Destination),);
    // d2 is blocked by own pawn
    assert_eq!(fb.get(Square::D2), None);
}
