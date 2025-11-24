use std::io::{self, Write};

use crate::game_logic::GameEngine;
use crate::hardware::MockPieceSensor;
use shakmaty::{Bitboard, File, Rank, Role, Square};

/// Clears the screen and moves cursor to top-left.
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
    println!("Commands: t <square> | r (reset) | p (refresh) | q (quit)");
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
    // // Check for missing/extra pieces (in Setup phase)
    // if missing.contains(square) {
    //     return " ○ ";
    // }
    // if extra.contains(square) {
    //     return " ⚠ ";
    // }

    let has_sensor = sensor_bb.contains(square);

    // In playing, show piece types or detect discrepancies
    if let Some(piece) = engine.piece_at(square) {
        // Should have a piece here
        if has_sensor {
            // Correct - show piece type
            match piece.role {
                Role::Pawn => " P ",
                Role::Knight => " N ",
                Role::Bishop => " B ",
                Role::Rook => " R ",
                Role::Queen => " Q ",
                Role::King => " K ",
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
