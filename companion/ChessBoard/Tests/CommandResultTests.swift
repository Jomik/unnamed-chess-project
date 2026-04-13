import Foundation
import Testing

@testable import ChessBoard

@Suite struct CommandResultTests {
    @Test func decodeStartGameSuccess() {
        let result = CommandResult.decode(Data([0x00, 0x00, 0x00]))
        #expect(
            result == CommandResult(ok: true, source: .startGame, error: nil)
        )
    }

    @Test func decodeMatchControlSuccess() {
        let result = CommandResult.decode(Data([0x00, 0x01, 0x00]))
        #expect(
            result == CommandResult(ok: true, source: .matchControl, error: nil)
        )
    }

    @Test func decodeSubmitMoveSuccess() {
        let result = CommandResult.decode(Data([0x00, 0x02, 0x00]))
        #expect(
            result == CommandResult(ok: true, source: .submitMove, error: nil)
        )
    }

    @Test func decodeStartGameErrorGameAlreadyInProgress() {
        let result = CommandResult.decode(Data([0x01, 0x00, 0x00]))
        #expect(
            result
                == CommandResult(
                    ok: false,
                    source: .startGame,
                    error: .gameAlreadyInProgress
                )
        )
    }

    @Test func decodeMatchControlErrorNoGameInProgress() {
        let result = CommandResult.decode(Data([0x01, 0x01, 0x01]))
        #expect(
            result
                == CommandResult(
                    ok: false,
                    source: .matchControl,
                    error: .noGameInProgress
                )
        )
    }

    @Test func decodeSubmitMoveErrorIllegalMove() {
        let result = CommandResult.decode(Data([0x01, 0x02, 0x03]))
        #expect(
            result
                == CommandResult(
                    ok: false,
                    source: .submitMove,
                    error: .illegalMove
                )
        )
    }

    @Test func decodeSubmitMoveErrorNotYourTurn() {
        let result = CommandResult.decode(Data([0x01, 0x02, 0x02]))
        #expect(
            result
                == CommandResult(
                    ok: false,
                    source: .submitMove,
                    error: .notYourTurn
                )
        )
    }

    @Test func decodeStartGameErrorInvalidCommand() {
        let result = CommandResult.decode(Data([0x01, 0x00, 0x05]))
        #expect(
            result
                == CommandResult(
                    ok: false,
                    source: .startGame,
                    error: .invalidCommand
                )
        )
    }

    @Test func decodeErrorUnknownBoardError() {
        // Unknown error code → BoardError is nil but result is still valid
        let result = CommandResult.decode(Data([0x01, 0x00, 0xFF]))
        #expect(
            result == CommandResult(ok: false, source: .startGame, error: nil)
        )
    }

    @Test func decodeCannotResignForRemotePlayer() {
        let result = CommandResult.decode(Data([0x01, 0x01, 0x04]))
        #expect(
            result
                == CommandResult(
                    ok: false,
                    source: .matchControl,
                    error: .cannotResignForRemotePlayer
                )
        )
    }

    @Test func decodeTooShort() {
        #expect(CommandResult.decode(Data([0x00, 0x00])) == nil)
        #expect(CommandResult.decode(Data([0x00])) == nil)
        #expect(CommandResult.decode(Data()) == nil)
    }

    @Test func decodeUnknownCommandSource() {
        #expect(CommandResult.decode(Data([0x00, 0xFF, 0x00])) == nil)
    }
}
