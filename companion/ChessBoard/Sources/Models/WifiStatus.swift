import Foundation

/// WiFi connection state. Mirrors firmware `WifiState` repr(u8).
enum WifiState: UInt8, Equatable {
    case disconnected = 0x00
    case connecting = 0x01
    case connected = 0x02
    case failed = 0x03
}

/// WiFi status received from the board.
///
/// Wire format: `[state: u8, msg_len: u8, msg_bytes...]`
struct WifiStatus: Equatable {
    let state: WifiState
    let message: String

    static let disconnected = WifiStatus(
        state: .disconnected,
        message: ""
    )

    static func decode(_ data: Data) -> WifiStatus? {
        guard data.count >= 2,
            let state = WifiState(rawValue: data[0])
        else {
            return nil
        }
        let msgLen = Int(data[1])
        guard data.count >= 2 + msgLen else { return nil }
        let message =
            String(data: data[2..<(2 + msgLen)], encoding: .utf8) ?? ""
        return WifiStatus(state: state, message: message)
    }
}
