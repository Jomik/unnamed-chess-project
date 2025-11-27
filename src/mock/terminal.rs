use std::io::{self, Write};

use super::MockPieceSensor;
use crate::game_logic::GameEngine;
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
    let mut sensor = MockPieceSensor::new();
    let mut engine = GameEngine::new();

    clear_screen();
    draw_interface(&mut sensor, &engine);

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
            "t" => {
                if parts.len() < 2 {
                    println!("Usage: t <square>");
                } else {
                    match parts[1].parse::<Square>() {
                        Ok(square) => {
                            sensor.toggle(square);
                            let bb = sensor.read_positions();
                            engine.tick(bb);
                            clear_screen();
                            draw_interface(&mut sensor, &engine);
                        }
                        Err(e) => println!("Invalid square: {}", e),
                    }
                }
            }
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
                                sensor.load_bitboard(chess.board().occupied());
                                engine = GameEngine::from_position(chess);
                                clear_screen();
                                draw_interface(&mut sensor, &engine);
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
                sensor = MockPieceSensor::new();
                engine = GameEngine::new();
                clear_screen();
                draw_interface(&mut sensor, &engine);
                println!("\n🔄 Reset to initial state");
            }
            "p" => {
                clear_screen();
                draw_interface(&mut sensor, &engine);
            }
            "q" => break,
            _ => println!("Unknown command"),
        }
    }
}

/// Draws the complete interface: help text, phase, events, and dual boards.
fn draw_interface(sensor: &mut MockPieceSensor, engine: &GameEngine) {
    println!("♟️  Chess Board Sensor Simulator");
    println!();

    println!();
    println!("Commands: t <square> | load <fen> | r (reset) | p (refresh) | q (quit)");
    println!();

    draw_dual_boards(sensor, engine);
}

/// Draws both boards side-by-side: raw sensors (left) and game state (right).
fn draw_dual_boards(sensor: &mut MockPieceSensor, engine: &GameEngine) {
    let sensor_bb = sensor.read_positions();

    println!("╔═════════════════════════════╦═════════════════════════════╗");
    println!("║       Raw Sensors           ║       Game State            ║");
    println!("╠═══╦═════════════════════════╬═══╦═════════════════════════╣");

    for rank in Rank::ALL.iter().rev() {
        // Left board: Raw sensor state
        print!("║ {} ║", rank.char());
        for file in File::ALL {
            let square = Square::from_coords(file, *rank);
            let has_sensor = sensor_bb.contains(square);
            print!("{}", if has_sensor { " ♟ " } else { " · " });
        }

        print!(" ║");

        // Right board: Game state with piece types and status
        print!(" {} ║", rank.char());
        for file in File::ALL {
            let square = Square::from_coords(file, *rank);
            let symbol = get_game_state_symbol(square, sensor_bb, engine);
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
}

/// Get the display symbol for a square on the game state board.
fn get_game_state_symbol(square: Square, sensor_bb: Bitboard, engine: &GameEngine) -> &'static str {
    let has_sensor = sensor_bb.contains(square);

    // In playing, show piece types or detect discrepancies
    if let Some(piece) = engine.piece_at(square) {
        // Should have a piece here
        if has_sensor {
            // Correct - show piece type
            match (piece.role, piece.color) {
                (Role::Pawn, Color::White) => " P ",
                (Role::Knight, Color::White) => " N ",
                (Role::Bishop, Color::White) => " B ",
                (Role::Rook, Color::White) => " R ",
                (Role::Queen, Color::White) => " Q ",
                (Role::King, Color::White) => " K ",
                (Role::Pawn, Color::Black) => " p ",
                (Role::Knight, Color::Black) => " n ",
                (Role::Bishop, Color::Black) => " b ",
                (Role::Rook, Color::Black) => " r ",
                (Role::Queen, Color::Black) => " q ",
                (Role::King, Color::Black) => " k ",
            }
        } else {
            // Missing piece
            " ○ "
        }
    } else {
        // Should be empty
        if has_sensor {
            // Extra piece
            " ⚠ "
        } else {
            // Correct empty
            " · "
        }
    }
}
