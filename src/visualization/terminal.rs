use crate::hardware::{mock::MockChessBoard, traits::*, Rgb};
use std::io::{self, Write};

pub fn run_interactive_terminal(mut board: MockChessBoard) {
    println!("♟️  Interactive Chess Board Simulator");
    println!("Commands:");
    println!("  t <square>  - Toggle piece (e.g., 't e2')");
    println!("  l <square> <r> <g> <b>  - Set LED color (e.g., 'l e2 255 0 0')");
    println!("  c  - Clear all LEDs");
    println!("  p  - Print board state");
    println!("  q  - Quit");
    println!();

    print_board_state(&mut board);

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
                    println!("Usage: t <square> (e.g., t e2)");
                    continue;
                }
                if let Some(square) = parse_square(parts[1]) {
                    toggle_piece(&mut board, square);
                    print_board_state(&mut board);
                } else {
                    println!("Invalid square: {}", parts[1]);
                }
            }
            "l" => {
                if parts.len() < 5 {
                    println!("Usage: l <square> <r> <g> <b> (e.g., l e2 255 0 0)");
                    continue;
                }
                if let Some(square) = parse_square(parts[1]) {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[2].parse::<u8>(),
                        parts[3].parse::<u8>(),
                        parts[4].parse::<u8>(),
                    ) {
                        let color = Rgb::new(r, g, b);
                        board.leds().set_square(square, [color; 4]).unwrap();
                        board.leds().show().unwrap();
                        print_board_state(&mut board);
                    } else {
                        println!("Invalid RGB values");
                    }
                }
            }
            "c" => {
                let black = [Rgb::new(0, 0, 0); NUM_LEDS];
                board.leds().set_all(black).unwrap();
                board.leds().show().unwrap();
                print_board_state(&mut board);
            }
            "p" => {
                print_board_state(&mut board);
            }
            "q" => {
                println!("Goodbye!");
                break;
            }
            _ => {
                println!("Unknown command: {}", parts[0]);
            }
        }
    }
}

fn parse_square(s: &str) -> Option<u8> {
    if s.len() != 2 {
        return None;
    }

    let mut chars = s.chars();
    let file = chars.next()?.to_ascii_lowercase();
    let rank = chars.next()?.to_digit(10)?;

    if !('a'..='h').contains(&file) || !(1..=8).contains(&rank) {
        return None;
    }

    let file_idx = (file as u8) - b'a';
    let rank_idx = (rank as u8) - 1;

    Some(rank_idx * 8 + file_idx)
}

fn toggle_piece(board: &mut MockChessBoard, square: u8) {
    let current = board.sensors().read_all().unwrap();
    let mask = 1u64 << square;
    let new_state = current ^ mask;
    board.sensors.set_bitboard(new_state);

    println!("Toggled piece on {}", square_to_notation(square));
}

fn square_to_notation(square: u8) -> String {
    let file = (b'a' + (square % 8)) as char;
    let rank = (square / 8) + 1;
    format!("{}{}", file, rank)
}

fn print_board_state(board: &mut MockChessBoard) {
    let bitboard = board.sensors.read_all().unwrap();

    println!("\n╔═════════════════════════════╗");
    println!("║      Sensors & LEDs         ║");
    println!("╠═══╦═════════════════════════╣");
    for rank in (0..8).rev() {
        print!("║ {} ║", rank + 1);
        for file in 0..8 {
            let square = rank * 8 + file;
            let has_piece = (bitboard & (1u64 << square)) != 0;

            // Get average LED color for this square
            let led_indices = crate::hardware::traits::square_leds(square);
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            for &idx in &led_indices {
                if let Some(color) = board.leds.get_pixel(idx) {
                    r += color.r as u32;
                    g += color.g as u32;
                    b += color.b as u32;
                }
            }
            r /= 4;
            g /= 4;
            b /= 4;

            // Print colored square with piece indicator
            if r == 0 && g == 0 && b == 0 {
                print!("{}", if has_piece { " ♟ " } else { " · " });
            } else {
                let piece_char = if has_piece { "♟" } else { " " };
                print!("\x1b[48;2;{};{};{}m {} \x1b[0m", r, g, b, piece_char);
            }
        }
        println!(" ║");
    }
    println!("╠═══╬═════════════════════════╣");
    println!("║   ║ a  b  c  d  e  f  g  h  ║");
    println!("╚═══╩═════════════════════════╝");
    println!(
        "\nBitboard: 0x{:016X} | Pieces: {}\n",
        bitboard,
        bitboard.count_ones()
    );
}
