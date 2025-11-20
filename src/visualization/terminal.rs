use std::io::{self, Write};

use crate::game_logic::PieceSensor;
use crate::hardware::MockPieceSensor;
use shakmaty::{File, Rank, Square};

/// Clears the screen and moves cursor to top-left.
fn clear_screen() {
    print!("\x1B[2J\x1B[H");
}

/// Runs an interactive terminal interface for simulating the chess board sensor.
///
/// Allows toggling pieces on squares and visualizing the current board state.
pub fn run_interactive_terminal(mut sensor: MockPieceSensor) {
    clear_screen();
    draw_interface(&mut sensor);

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
                            clear_screen();
                            draw_interface(&mut sensor);
                        }
                        Err(e) => println!("Invalid square: {}", e),
                    }
                }
            }
            "p" => {
                clear_screen();
                draw_interface(&mut sensor);
            }
            "q" => break,
            _ => println!("Unknown command"),
        }
    }
}

/// Draws the complete interface: help text and board.
fn draw_interface(sensor: &mut MockPieceSensor) {
    println!("♟️  Chess Board Sensor Simulator");
    println!();
    println!("Commands: t <square> | p (refresh) | q (quit)");
    println!();
    draw_board(sensor);
}

/// Draws the board display.
fn draw_board(sensor: &mut MockPieceSensor) {
    let bb = sensor.read_positions();

    println!("╔═════════════════════════════╗");
    println!("║      Piece Positions        ║");
    println!("╠═══╦═════════════════════════╣");

    for rank in Rank::ALL.iter().rev() {
        print!("║ {} ║", rank.char());
        for file in File::ALL {
            let square = Square::from_coords(file, *rank);
            print!("{}", if bb.contains(square) { " ♟ " } else { " · " });
        }
        println!(" ║");
    }

    println!("╠═══╬═════════════════════════╣");
    println!("║   ║ a  b  c  d  e  f  g  h  ║");
    println!("╚═══╩═════════════════════════╝");
    println!("Bitboard: {:#018X} | Pieces: {:02}", bb, bb.count())
}
