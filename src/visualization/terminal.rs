use std::io::{self, Write};

use crate::game_logic::{GameEngine, GameEvent, GamePhase};
use crate::hardware::MockPieceSensor;
use shakmaty::{Bitboard, Chess, File, Position, Rank, Role, Square};

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
    draw_interface(&mut sensor, &engine, &[]);

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
                            let events = engine.tick(bb);
                            clear_screen();
                            draw_interface(&mut sensor, &engine, &events);
                        }
                        Err(e) => println!("Invalid square: {}", e),
                    }
                }
            }
            "r" => {
                sensor = MockPieceSensor::new();
                engine = GameEngine::new();
                clear_screen();
                draw_interface(&mut sensor, &engine, &[]);
                println!("\n🔄 Reset to initial state");
            }
            "p" => {
                clear_screen();
                draw_interface(&mut sensor, &engine, &[]);
            }
            "q" => break,
            _ => println!("Unknown command"),
        }
    }
}

/// Draws the complete interface: help text, phase, events, and dual boards.
fn draw_interface(sensor: &mut MockPieceSensor, engine: &GameEngine, events: &[GameEvent]) {
    println!("♟️  Chess Board Sensor Simulator");
    println!();

    // Show game phase
    let phase_str = match engine.phase() {
        GamePhase::Setup { .. } => "⏳ Waiting for Setup",
        GamePhase::Playing => "🎮 Game in Progress",
        GamePhase::GameOver => "🏁 Game Over",
    };
    println!("Phase: {}", phase_str);

    // Show recent events
    if !events.is_empty() {
        println!();
        println!("📢 Events:");
        for event in events {
            println!("   {}", format_event(event));
        }
    }

    println!();
    println!("Commands: t <square> | r (reset) | p (refresh) | q (quit)");
    println!();

    draw_dual_boards(sensor, engine);
}

/// Format a game event for display.
fn format_event(event: &GameEvent) -> String {
    match event {
        GameEvent::PieceLifted { from } => format!("🖐️  Piece lifted from {}", from),
        GameEvent::MoveDetected { from, to } => format!("♟️  Move: {} → {}", from, to),
        GameEvent::IllegalMove { from, to } => format!("❌ Illegal: {} → {}", from, to),
        GameEvent::InvalidBoardState => "⚠️  Invalid board state".to_string(),
        GameEvent::GameOver { result } => format!("🏁 Game over: {}", result),
    }
}

/// Draws both boards side-by-side: raw sensors (left) and game state (right).
fn draw_dual_boards(sensor: &mut MockPieceSensor, engine: &GameEngine) {
    let sensor_bb = sensor.read_positions();
    let lifted_from = engine.lifted_from();

    // Get missing/extra from phase if in setup
    let (missing, extra) = match engine.phase() {
        GamePhase::Setup { missing, extra } => (missing, extra),
        _ => (Bitboard::EMPTY, Bitboard::EMPTY),
    };

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
            let symbol =
                get_game_state_symbol(square, sensor_bb, missing, extra, engine, lifted_from);
            print!("{}", symbol);
        }

        println!(" ║");
    }

    println!("╠═══╬═════════════════════════╬═══╬═════════════════════════╣");
    println!("║   ║ a  b  c  d  e  f  g  h  ║   ║ a  b  c  d  e  f  g  h  ║");
    println!("╚═══╩═════════════════════════╩═══╩═════════════════════════╝");

    let expected_bb = get_expected_bitboard(engine);
    println!(
        "Sensor:   {:#018X} | Pieces: {:02}",
        sensor_bb,
        sensor_bb.count()
    );
    println!(
        "Expected: {:#018X} | Pieces: {:02}",
        expected_bb,
        expected_bb.count()
    );

    // Show legend
    match engine.phase() {
        GamePhase::Setup { .. } => {
            println!("\nLegend (Game State): ♟ = correct, · = empty, ○ = missing, ⚠ = extra");
        }
        GamePhase::Playing => {
            println!("\nLegend (Game State): PNBRQK = pieces, · = empty, ○ = missing, ⚠ = extra, 🖐 = lifted");
        }
        GamePhase::GameOver => {
            println!("\nLegend (Game State): PNBRQK = pieces");
        }
    }
}

/// Get the display symbol for a square on the game state board.
fn get_game_state_symbol(
    square: Square,
    sensor_bb: Bitboard,
    missing: Bitboard,
    extra: Bitboard,
    engine: &GameEngine,
    lifted_from: Option<Square>,
) -> &'static str {
    // Check if this is the lifted square
    if Some(square) == lifted_from {
        return " 🖐 ";
    }

    // Check for missing/extra pieces (in Setup phase)
    if missing.contains(square) {
        return " ○ ";
    }
    if extra.contains(square) {
        return " ⚠ ";
    }

    let has_sensor = sensor_bb.contains(square);

    match engine.phase() {
        GamePhase::Setup { .. } => {
            // In setup, just show correct or empty
            if has_sensor {
                " ♟ "
            } else {
                " · "
            }
        }
        GamePhase::Playing | GamePhase::GameOver => {
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
    }
}

/// Get the expected bitboard for the current engine state
fn get_expected_bitboard(engine: &GameEngine) -> Bitboard {
    match engine.phase() {
        GamePhase::Setup { .. } => Chess::default().board().occupied(),
        GamePhase::Playing => {
            let mut bb = Chess::default().board().occupied();
            // Would need to track actual position, but for now use default
            // In real implementation, we'd use engine.position().board().occupied()
            // but we don't expose position(), so this is a visualization-only helper
            if let Some(from) = engine.lifted_from() {
                bb ^= Bitboard::from_square(from);
            }
            bb
        }
        GamePhase::GameOver => Chess::default().board().occupied(),
    }
}
