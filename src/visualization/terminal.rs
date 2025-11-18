use crate::game_state::{PieceSensor, Square};
use crate::hardware::MockPieceSensor;
use std::io::{self, Write};

pub fn run_interactive_terminal(mut sensor: MockPieceSensor) {
    println!("♟️  Chess Board Sensor Simulator");
    println!("Commands:");
    println!("  t <square>  - Toggle piece (e.g., 't e2')");
    println!("  p  - Print board");
    println!("  q  - Quit");
    println!();

    print_board(&mut sensor);

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "t" => {
                if parts.len() < 2 {
                    println!("Usage: t <square>");
                    continue;
                }
                if let Some(square) = parse_square(parts[1]) {
                    sensor.toggle(square);
                    print_board(&mut sensor);
                } else {
                    println!("Invalid square");
                }
            }
            "p" => print_board(&mut sensor),
            "q" => break,
            _ => println!("Unknown command"),
        }
    }
}

fn parse_square(s: &str) -> Option<Square> {
    if s.len() != 2 {
        return None;
    }
    let file = s.chars().next()?.to_ascii_lowercase();
    let rank = s.chars().nth(1)?.to_digit(10)?;

    if !('a'..='h').contains(&file) || !(1..=8).contains(&rank) {
        return None;
    }

    let idx = ((rank as u8 - 1) * 8) + (file as u8 - b'a');
    Square::new(idx)
}

fn print_board(sensor: &mut MockPieceSensor) {
    let bb = sensor.read_positions();

    println!("\n╔═════════════════════════════╗");
    println!("║      Piece Positions        ║");
    println!("╠═══╦═════════════════════════╣");

    for rank in (0..8).rev() {
        print!("║ {} ║", rank + 1);
        for file in 0..8 {
            let square = Square::new(rank * 8 + file).unwrap();
            let bit = 1u64 << square.value();
            print!(
                "{}",
                if bb.value() & bit != 0 {
                    " ♟ "
                } else {
                    " · "
                }
            );
        }
        println!(" ║");
    }

    println!("╠═══╬═════════════════════════╣");
    println!("║   ║ a  b  c  d  e  f  g  h  ║");
    println!("╚═══╩═════════════════════════╝");
    println!(
        "\nBitboard: 0x{:016X} | Pieces: {}\n",
        bb.value(),
        bb.value().count_ones()
    );
}
