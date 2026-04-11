use shakmaty::{Bitboard, ByColor, Chess, Move};
use std::sync::mpsc;

use super::Player;

/// A player whose moves are delivered from an external source (e.g. BLE SubmitMove).
///
/// Moves are pushed into the sender half; `poll_move` drains the receiver.
/// The move is a shakmaty `Move` — validation and UCI parsing happen upstream.
#[derive(Debug)]
pub struct RemotePlayer {
    rx: mpsc::Receiver<Move>,
}

impl RemotePlayer {
    pub fn new(rx: mpsc::Receiver<Move>) -> Self {
        Self { rx }
    }
}

impl Player for RemotePlayer {
    fn poll_move(&mut self, _position: &Chess, _sensors: ByColor<Bitboard>) -> Option<Move> {
        self.rx.try_recv().ok()
    }

    fn is_interactive(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::{Move, Role, Square};

    fn make_normal_move() -> Move {
        Move::Normal {
            role: Role::Pawn,
            from: Square::E2,
            to: Square::E4,
            capture: None,
            promotion: None,
        }
    }

    #[test]
    fn poll_returns_none_when_no_move() {
        let (_tx, rx) = mpsc::channel();
        let mut player = RemotePlayer::new(rx);
        let pos = Chess::default();
        let sensors = ByColor {
            white: Bitboard::EMPTY,
            black: Bitboard::EMPTY,
        };
        assert!(player.poll_move(&pos, sensors).is_none());
    }

    #[test]
    fn poll_returns_move_after_send() {
        let (tx, rx) = mpsc::channel();
        let mut player = RemotePlayer::new(rx);
        let pos = Chess::default();
        let sensors = ByColor {
            white: Bitboard::EMPTY,
            black: Bitboard::EMPTY,
        };

        let mv = make_normal_move();
        tx.send(mv).unwrap();

        assert_eq!(player.poll_move(&pos, sensors), Some(mv));
    }

    #[test]
    fn poll_returns_none_after_consuming_move() {
        let (tx, rx) = mpsc::channel();
        let mut player = RemotePlayer::new(rx);
        let pos = Chess::default();
        let sensors = ByColor {
            white: Bitboard::EMPTY,
            black: Bitboard::EMPTY,
        };

        tx.send(make_normal_move()).unwrap();
        player.poll_move(&pos, sensors); // consume
        assert!(player.poll_move(&pos, sensors).is_none());
    }

    #[test]
    fn is_not_interactive() {
        let (_tx, rx) = mpsc::channel();
        let player = RemotePlayer::new(rx);
        assert!(!player.is_interactive());
    }

    #[test]
    fn poll_returns_none_when_sender_dropped() {
        let (tx, rx) = mpsc::channel::<Move>();
        let mut player = RemotePlayer::new(rx);
        drop(tx);
        let pos = Chess::default();
        let sensors = ByColor {
            white: Bitboard::EMPTY,
            black: Bitboard::EMPTY,
        };
        assert!(player.poll_move(&pos, sensors).is_none());
    }
}
