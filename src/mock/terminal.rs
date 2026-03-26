use std::io::{self, Write};

use super::ScriptedSensor;
use crate::feedback::{BoardFeedback, SquareFeedback};
use crate::player::{EmbeddedEngine, HumanPlayer};
use crate::session::{GameSession, TickResult};
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
    let initial = sensor.read_positions();
    let mut session = GameSession::new(
        Box::new(HumanPlayer::new(initial)),
        Box::new(EmbeddedEngine::new(42)),
    );
    let mut last_result = session.tick(initial);

    clear_screen();
    draw_interface(&sensor, &session, &last_result);

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
                                let positions = sensor.read_positions();
                                session = GameSession::from_position(
                                    chess,
                                    Box::new(HumanPlayer::new(positions)),
                                    Box::new(EmbeddedEngine::new(42)),
                                );
                                last_result = session.tick(positions);
                                clear_screen();
                                draw_interface(&sensor, &session, &last_result);
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
                let initial = sensor.read_positions();
                session = GameSession::new(
                    Box::new(HumanPlayer::new(initial)),
                    Box::new(EmbeddedEngine::new(42)),
                );
                last_result = session.tick(initial);
                clear_screen();
                draw_interface(&sensor, &session, &last_result);
                println!("\n🔄 Reset to initial state");
            }
            "p" => {
                clear_screen();
                draw_interface(&sensor, &session, &last_result);
            }
            "q" => break,
            _ => {
                // Treat input as BoardScript
                let result = sensor.push_script(&input).and_then(|()| {
                    sensor.drain(|p| {
                        last_result = session.tick(p);
                    })
                });
                match result {
                    Ok(()) => {
                        clear_screen();
                        draw_interface(&sensor, &session, &last_result);
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
fn draw_interface(sensor: &ScriptedSensor, session: &GameSession, result: &TickResult) {
    println!("♟️  Chess Board Sensor Simulator");
    println!();
    println!("Commands: <script> | load <fen> | r (reset) | p (refresh) | q (quit)");
    println!("Script format: e2e4. (toggle squares, '.' to tick)");
    println!();

    draw_dual_boards(sensor, session, result);
}

/// Draws both boards side-by-side: raw sensors (left) and game state (right).
fn draw_dual_boards(sensor: &ScriptedSensor, session: &GameSession, result: &TickResult) {
    let sensor_positions = sensor.read_positions();
    let sensor_bb = sensor_positions.white | sensor_positions.black;

    println!("╔═════════════════════════════╦═════════════════════════════╗");
    println!("║       Raw Sensors           ║       Game State            ║");
    println!("╠═══╦═════════════════════════╬═══╦═════════════════════════╣");

    for rank in Rank::ALL.iter().rev() {
        // Left board: Raw sensor state
        print!("║ {} ║", rank.char());
        for file in File::ALL {
            let square = Square::from_coords(file, *rank);
            let cell = if sensor_positions.white.contains(square) {
                "\x1b[97m●\x1b[0m" // white piece present (bright)
            } else if sensor_positions.black.contains(square) {
                "\x1b[90m●\x1b[0m" // black piece present (gray)
            } else {
                "·"
            };

            print!(" {} ", cell);
        }

        print!(" ║");

        // Right board: Game state with piece types and feedback highlights
        print!(" {} ║", rank.char());
        for file in File::ALL {
            let square = Square::from_coords(file, *rank);
            let symbol = get_game_state_symbol(square, sensor_bb, session, &result.feedback);
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

    // Show move info from feedback
    let origin_count = result
        .feedback
        .squares()
        .filter(|(_, t)| matches!(t, SquareFeedback::Origin))
        .count();
    let dest_count = result
        .feedback
        .squares()
        .filter(|(_, t)| matches!(t, SquareFeedback::Destination | SquareFeedback::Capture))
        .count();
    if origin_count > 0 || dest_count > 0 {
        println!("\nOrigins: {origin_count}  Destinations: {dest_count}");
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
    session: &GameSession,
    feedback: &BoardFeedback,
) -> String {
    let has_sensor = sensor_bb.contains(square);

    let symbol = match (session.position().board().piece_at(square), has_sensor) {
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
        Some(SquareFeedback::Victory) => format!("\x1b[102m {} \x1b[0m", symbol),    // Bright green
        Some(SquareFeedback::Stalemate) => format!("\x1b[103m {} \x1b[0m", symbol), // Bright yellow
        None => format!(" {} ", symbol),
    }
}
