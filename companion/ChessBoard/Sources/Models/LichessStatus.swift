import Foundation

/// Lichess connection state. Mirrors firmware `LichessState` repr(u8).
enum LichessState: UInt8, Equatable {
    case idle = 0x00
    case validating = 0x01
    case connected = 0x02
    case failed = 0x03
}

/// Lichess status received from the board.
///
/// Wire format: `[state: u8, msg_len: u8, msg_bytes...]`
struct LichessStatus: Equatable {
    let state: LichessState
    let message: String

    static let idle = LichessStatus(state: .idle, message: "")

    static func decode(_ data: Data) -> LichessStatus? {
        guard data.count >= 2,
            let state = LichessState(rawValue: data[0])
        else {
            return nil
        }
        let msgLen = Int(data[1])
        guard data.count >= 2 + msgLen else { return nil }
        let message =
            String(data: data[2..<(2 + msgLen)], encoding: .utf8) ?? ""
        return LichessStatus(state: state, message: message)
    }
}
