import Foundation

/// Game status values. Mirrors firmware `GameStatus` repr(u8).
enum GameStatus: UInt8, Equatable {
    case idle = 0x00
    case awaitingPieces = 0x01
    case inProgress = 0x02
    case checkmate = 0x03
    case stalemate = 0x04
    case resignation = 0x05
    case draw = 0x06
}

enum Turn: UInt8, Equatable {
    case white = 0x00
    case black = 0x01
}

/// Wire format: `[status: u8, turn: u8]`
struct GameState: Equatable {
    let status: GameStatus
    let turn: Turn

    static let initial = GameState(status: .idle, turn: .white)

    static func decode(_ data: Data) -> GameState? {
        guard data.count >= 2,
            let status = GameStatus(rawValue: data[0]),
            let turn = Turn(rawValue: data[1])
        else {
            return nil
        }
        return GameState(status: status, turn: turn)
    }

    var isTerminal: Bool {
        switch status {
        case .checkmate, .stalemate, .resignation, .draw: return true
        case .idle, .awaitingPieces, .inProgress: return false
        }
    }
}
