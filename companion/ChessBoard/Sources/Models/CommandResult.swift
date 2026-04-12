import Foundation

enum CommandSource: UInt8 {
    case startGame = 0x00
    case matchControl = 0x01
    case submitMove = 0x02
}

enum BoardError: UInt8, Equatable {
    case gameAlreadyInProgress = 0x00
    case noGameInProgress = 0x01
    case notYourTurn = 0x02
    case illegalMove = 0x03
    case cannotResignForRemotePlayer = 0x04
    case invalidCommand = 0x05
}

struct CommandResult: Equatable {
    let ok: Bool
    let source: CommandSource
    let error: BoardError?

    static func decode(_ data: Data) -> CommandResult? {
        guard data.count >= 3,
            let source = CommandSource(rawValue: data[1])
        else { return nil }
        let ok = data[0] == 0x00
        let error = ok ? nil : BoardError(rawValue: data[2])
        return CommandResult(ok: ok, source: source, error: error)
    }
}
