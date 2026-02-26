use std::io::{self, Write};

use shakmaty::{File, Rank, Square};

use crate::BoardDisplay;
use crate::feedback::{BoardFeedback, SquareFeedback};

/// Terminal-based board display for development and testing.
///
/// Renders [`BoardFeedback`] as an 8×8 grid with ANSI color-coded
/// squares matching the feedback type (destinations, captures, etc.).
#[derive(Debug, Default)]
pub struct TerminalDisplay;

impl TerminalDisplay {
    /// Create a new terminal display.
    pub fn new() -> Self {
        Self
    }
}

/// Error type for terminal display operations.
#[derive(Debug, thiserror::Error)]
pub enum DisplayError {
    #[error("failed to write to terminal: {0}")]
    Io(#[from] io::Error),
}

impl BoardDisplay for TerminalDisplay {
    type Error = DisplayError;

    fn show(&mut self, feedback: &BoardFeedback) -> Result<(), Self::Error> {
        render_feedback(&mut io::stdout(), feedback)
    }
}

/// Render feedback to any writer. Extracted for testability.
fn render_feedback(w: &mut impl Write, feedback: &BoardFeedback) -> Result<(), DisplayError> {
    for rank in Rank::ALL.iter().rev() {
        write!(w, " {} ", rank.char())?;
        for file in File::ALL {
            let square = Square::from_coords(file, *rank);
            let cell = format_square(feedback.get(square));
            write!(w, "{cell}")?;
        }
        writeln!(w)?;
    }
    writeln!(w, "    a  b  c  d  e  f  g  h")?;
    w.flush()?;
    Ok(())
}

/// Map a feedback variant to an ANSI-colored cell string.
fn format_square(feedback: Option<SquareFeedback>) -> &'static str {
    match feedback {
        Some(SquareFeedback::Destination) => "\x1b[44m · \x1b[0m",
        Some(SquareFeedback::Capture) => "\x1b[41m · \x1b[0m",
        Some(SquareFeedback::Origin) => "\x1b[42m · \x1b[0m",
        Some(SquareFeedback::Check) => "\x1b[45m · \x1b[0m",
        Some(SquareFeedback::Checker) => "\x1b[43m · \x1b[0m",
        None => " · ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_string(feedback: &BoardFeedback) -> String {
        let mut buf = Vec::new();
        render_feedback(&mut buf, feedback).expect("rendering to buffer should succeed");
        String::from_utf8(buf).expect("output should be valid UTF-8")
    }

    #[test]
    fn show_empty_feedback_contains_all_ranks() {
        let output = render_to_string(&BoardFeedback::new());

        for rank in '1'..='8' {
            assert!(
                output.contains(rank),
                "output should contain rank label '{rank}'"
            );
        }
    }

    #[test]
    fn show_empty_feedback_contains_file_labels() {
        let output = render_to_string(&BoardFeedback::new());

        assert!(
            output.contains("a  b  c  d  e  f  g  h"),
            "output should contain file labels"
        );
    }

    #[test]
    fn show_destination_uses_blue_background() {
        let mut feedback = BoardFeedback::new();
        feedback.set(Square::E4, SquareFeedback::Destination);

        let output = render_to_string(&feedback);

        assert!(
            output.contains("\x1b[44m"),
            "destination should use blue ANSI background"
        );
    }

    #[test]
    fn show_capture_uses_red_background() {
        let mut feedback = BoardFeedback::new();
        feedback.set(Square::D5, SquareFeedback::Capture);

        let output = render_to_string(&feedback);

        assert!(
            output.contains("\x1b[41m"),
            "capture should use red ANSI background"
        );
    }

    #[test]
    fn show_origin_uses_green_background() {
        let mut feedback = BoardFeedback::new();
        feedback.set(Square::E2, SquareFeedback::Origin);

        let output = render_to_string(&feedback);

        assert!(
            output.contains("\x1b[42m"),
            "origin should use green ANSI background"
        );
    }

    #[test]
    fn show_check_uses_magenta_background() {
        let mut feedback = BoardFeedback::new();
        feedback.set(Square::E8, SquareFeedback::Check);

        let output = render_to_string(&feedback);

        assert!(
            output.contains("\x1b[45m"),
            "check should use magenta ANSI background"
        );
    }

    #[test]
    fn show_checker_uses_yellow_background() {
        let mut feedback = BoardFeedback::new();
        feedback.set(Square::H5, SquareFeedback::Checker);

        let output = render_to_string(&feedback);

        assert!(
            output.contains("\x1b[43m"),
            "checker should use yellow ANSI background"
        );
    }

    #[test]
    fn empty_feedback_has_no_ansi_codes() {
        let output = render_to_string(&BoardFeedback::new());

        assert!(
            !output.contains("\x1b[4"),
            "empty feedback should have no ANSI background codes"
        );
    }
}
