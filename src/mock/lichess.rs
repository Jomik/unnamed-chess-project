use crate::lichess::{GameEvent, LichessClient, LichessGame, LichessStream};

#[derive(Debug)]
pub struct MockLichessClient;

pub struct MockLichessGame;

pub struct MockLichessStream;

impl LichessClient for MockLichessClient {
    type Error = String;
    type Game = MockLichessGame;

    fn challenge_ai(
        self,
        _level: u8,
        _clock_limit: u32,
        _clock_increment: u32,
    ) -> Result<MockLichessGame, String> {
        Err("Lichess HTTP not implemented on host".to_string())
    }
}

impl LichessGame for MockLichessGame {
    type Error = String;

    fn game_id(&self) -> &str {
        "mock"
    }

    fn into_stream(self) -> Result<Box<dyn LichessStream<Error = String>>, String> {
        Err("Lichess HTTP not implemented on host".to_string())
    }
}

impl LichessStream for MockLichessStream {
    type Error = String;

    fn next_event(&mut self) -> Option<Result<GameEvent, String>> {
        None
    }

    fn make_move(&mut self, _uci_move: &str) -> Result<(), String> {
        Err("Lichess HTTP not implemented on host".to_string())
    }
}
