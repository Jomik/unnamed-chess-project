import Foundation

enum Turn: UInt8, Equatable {
    case white = 0x00
    case black = 0x01
}

enum GameStatus: Equatable {
    case idle
    case awaitingPieces
    case inProgress
    case checkmate(loser: Turn)
    case stalemate
    case resigned(color: Turn)

    var isTerminal: Bool {
        switch self {
        case .checkmate, .stalemate, .resigned: return true
        case .idle, .awaitingPieces, .inProgress: return false
        }
    }

    static func decode(_ data: Data) -> GameStatus? {
        guard let tag = data.first else { return nil }
        switch tag {
        case 0x00: return .idle
        case 0x01: return .awaitingPieces
        case 0x02: return .inProgress
        case 0x03:
            guard data.count >= 2, let turn = Turn(rawValue: data[1]) else {
                return nil
            }
            return .checkmate(loser: turn)
        case 0x04: return .stalemate
        case 0x05:
            guard data.count >= 2, let turn = Turn(rawValue: data[1]) else {
                return nil
            }
            return .resigned(color: turn)
        default: return nil
        }
    }
}
