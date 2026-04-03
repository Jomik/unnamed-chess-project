import Foundation

enum CommandSource: UInt8, Equatable {
    case startGame = 0x00
    case matchControl = 0x01
}

/// Wire format: `[ok: u8, command: u8, msg_len: u8, msg_bytes...]`
/// - ok: `0x00` = success, `0x01` = error
/// - command: which command produced this result
struct CommandResult: Equatable {
    let ok: Bool
    let source: CommandSource
    let message: String

    static func decode(_ data: Data) -> CommandResult? {
        guard data.count >= 3 else { return nil }
        let ok = data[0] == 0x00
        guard let source = CommandSource(rawValue: data[1]) else { return nil }
        let msgLen = Int(data[2])
        guard data.count >= 3 + msgLen else { return nil }
        let message =
            String(data: data[3..<(3 + msgLen)], encoding: .utf8) ?? ""
        return CommandResult(ok: ok, source: source, message: message)
    }
}
