use std::io::{self, Write};

use super::ScriptedSensor;
use crate::feedback::{self, FeedbackSource, SquareFeedback};
use crate::game_logic::GameEngine;
use shakmaty::Piece;
use shakmaty::{
    Bitboard, CastlingMode, Chess, Color, File, Position, Rank, Role, Square, fen::Fen,
};

/// Clears the screen and moves cursor to top-left.
#[inline]
fn clear_screen() {
    print!("\x1B[2J\x1B[H");
}

/// Runs an interactive terminal interface for simulating the chess board sensor.
///
/// Displays raw sensor state alongside interpreted game state.
pub fn run_interactive_terminal() {
    let mut sensor = ScriptedSensor::new();
    let mut engine = GameEngine::new();
    let positions = sensor.read_positions();
    let mut last_state = engine.tick(positions);

    clear_screen();
    draw_interface(&sensor, &engine, &last_state);

    loop {
        print!("> ");
        if let Err(e) = io::stdout().flush() {
            eprintln!("Failed to flush stdout: {}", e);
            break;
        }

        let mut input = String::new();
        if let Err(e) = io::stdin().read_line(&mut input) {
            eprintln!("Failed to read input: {}", e);
            break;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "load" => {
                if parts.len() < 2 {
                    println!("Usage: load <fen> | load startpos");
                } else {
                    let fen_str = if parts[1] == "startpos" {
                        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
                    } else {
                        // Rejoin the rest of the parts as FEN might contain spaces
                        input[5..].trim()
                    };

                    match fen_str.parse::<Fen>() {
                        Ok(fen) => {
                            if let Ok(chess) = fen.into_position::<Chess>(CastlingMode::Standard) {
                                let board = chess.board();
                                sensor
                                    .load_bitboards(
                                        board.by_color(Color::White),
                                        board.by_color(Color::Black),
                                    )
                                    .expect("board positions cannot overlap");
                                engine = GameEngine::from_position(chess);
                                let positions = sensor.read_positions();
                                last_state = engine.tick(positions);
                                clear_screen();
                                draw_interface(&sensor, &engine, &last_state);
                                println!("\n✅ Position loaded from FEN");
                            } else {
                                println!("❌ Invalid FEN setup");
                            }
                        }
                        Err(e) => println!("❌ Invalid FEN: {}", e),
                    }
                }
            }
            "r" => {
                sensor = ScriptedSensor::new();
                engine = GameEngine::new();
                let positions = sensor.read_positions();
                last_state = engine.tick(positions);
                clear_screen();
                draw_interface(&sensor, &engine, &last_state);
                println!("\n🔄 Reset to initial state");
            }
            "p" => {
                clear_screen();
                draw_interface(&sensor, &engine, &last_state);
            }
            "q" => break,
            _ => {
                // Treat input as BoardScript
                let result = sensor.push_script(&input).and_then(|()| {
                    sensor.drain(|p| {
                        last_state = engine.tick(p);
                    })
                });
                match result {
                    Ok(()) => {
                        clear_screen();
                        draw_interface(&sensor, &engine, &last_state);
                    }
                    Err(e) => {
                        println!("❌ {}", e);
                    }
                }
            }
        }
    }
}

/// Draws the complete interface: help text and dual boards.
fn draw_interface(sensor: &ScriptedSensor, engine: &GameEngine, state: &impl FeedbackSource) {
    println!("♟️  Chess Board Sensor Simulator");
    println!();
    println!("Commands: <script> | load <fen> | r (reset) | p (refresh) | q (quit)");
    println!("Script format: e2e4. (toggle squares, '.' to tick)");
    println!();

    draw_dual_boards(sensor, engine, state);
}

/// Draws both boards side-by-side: raw sensors (left) and game state (right).
fn draw_dual_boards(sensor: &ScriptedSensor, engine: &GameEngine, state: &impl FeedbackSource) {
    let sensor_positions = sensor.read_positions();
    let sensor_bb = sensor_positions.white | sensor_positions.black;
    let feedback = feedback::compute_feedback(state);

    println!("╔═════════════════════════════╦═════════════════════════════╗");
    println!("║       Raw Sensors           ║       Game State            ║");
    println!("╠═══╦═════════════════════════╬═══╦═════════════════════════╣");

    for rank in Rank::ALL.iter().rev() {
        // Left board: Raw sensor state
        print!("║ {} ║", rank.char());
        for file in File::ALL {
            let square = Square::from_coords(file, *rank);
            let cell = match engine.piece_at(square).map(|p| p.color) {
                Some(Color::White) => "\x1b[97m●\x1b[0m", // white piece present (bright)
                Some(Color::Black) => "\x1b[90m●\x1b[0m", // black piece present (gray)
                None => "·",
            };

            print!(" {} ", cell);
        }

        print!(" ║");

        // Right board: Game state with piece types and feedback highlights
        print!(" {} ║", rank.char());
        for file in File::ALL {
            let square = Square::from_coords(file, *rank);
            let symbol = get_game_state_symbol(square, sensor_bb, engine, &feedback);
            print!("{}", symbol);
        }

        println!(" ║");
    }

    println!("╠═══╬═════════════════════════╬═══╬═════════════════════════╣");
    println!("║   ║ a  b  c  d  e  f  g  h  ║   ║ a  b  c  d  e  f  g  h  ║");
    println!("╚═══╩═════════════════════════╩═══╩═════════════════════════╝");

    println!(
        "Sensor:   {:#018X} | Pieces: {:02}",
        sensor_bb,
        sensor_bb.count()
    );

    // Show lifted piece info
    if let Some(lifted_square) = state.lifted_piece() {
        println!("\n🔵 Lifted: {}", lifted_square);
        println!(
            "   Legal destinations: {}",
            feedback
                .squares()
                .filter(|(_, t)| matches!(t, SquareFeedback::Destination | SquareFeedback::Capture))
                .count()
        );
    }
    if let Some(captured_square) = state.captured_piece() {
        println!("\n🔴 Captured: {}", captured_square);
        println!(
            "   Legal captors: {}",
            feedback
                .squares()
                .filter(|(_, t)| matches!(t, SquareFeedback::Origin))
                .count()
        );
    }
}

fn piece_symbol(piece: Piece) -> String {
    let glyph = match piece.role {
        Role::Pawn => "♟",
        Role::Knight => "♞",
        Role::Bishop => "♝",
        Role::Rook => "♜",
        Role::Queen => "♛",
        Role::King => "♚",
    };
    let fg_code = match piece.color {
        Color::White => 97, // bright white
        Color::Black => 90, // dark gray
    };
    format!("\x1b[{}m{}\x1b[39m", fg_code, glyph) // \x1b[39m resets fg only
}

fn get_game_state_symbol(
    square: Square,
    sensor_bb: Bitboard,
    engine: &GameEngine,
    feedback: &feedback::BoardFeedback,
) -> String {
    let has_sensor = sensor_bb.contains(square);

    let symbol = match (engine.piece_at(square), has_sensor) {
        (Some(piece), true) => piece_symbol(piece), // Correct
        (Some(_), false) => "○".into(),             // Missing
        (None, true) => "⚠".into(),                 // Extra
        (None, false) => "·".into(),                // Empty
    };

    match feedback.get(square) {
        Some(SquareFeedback::Destination) => format!("\x1b[44m {} \x1b[0m", symbol), // Blue
        Some(SquareFeedback::Capture) => format!("\x1b[41m {} \x1b[0m", symbol),     // Red
        Some(SquareFeedback::Origin) => format!("\x1b[42m {} \x1b[0m", symbol),      // Green
        Some(SquareFeedback::Check) => format!("\x1b[45m {} \x1b[0m", symbol),       // Magenta
        Some(SquareFeedback::Checker) => format!("\x1b[43m {} \x1b[0m", symbol),     // Yellow
        None => format!(" {} ", symbol),
    }
}
