import XCTest

@testable import ChessBoard

final class CommandResultTests: XCTestCase {
    func testDecodeStartGameSuccess() {
        let result = CommandResult.decode(Data([0x00, 0x00, 0x00]))
        XCTAssertEqual(
            result,
            CommandResult(ok: true, source: .startGame, error: nil)
        )
    }

    func testDecodeMatchControlSuccess() {
        let result = CommandResult.decode(Data([0x00, 0x01, 0x00]))
        XCTAssertEqual(
            result,
            CommandResult(ok: true, source: .matchControl, error: nil)
        )
    }

    func testDecodeSubmitMoveSuccess() {
        let result = CommandResult.decode(Data([0x00, 0x02, 0x00]))
        XCTAssertEqual(
            result,
            CommandResult(ok: true, source: .submitMove, error: nil)
        )
    }

    func testDecodeStartGameErrorGameAlreadyInProgress() {
        let result = CommandResult.decode(Data([0x01, 0x00, 0x00]))
        XCTAssertEqual(
            result,
            CommandResult(
                ok: false,
                source: .startGame,
                error: .gameAlreadyInProgress
            )
        )
    }

    func testDecodeMatchControlErrorNoGameInProgress() {
        let result = CommandResult.decode(Data([0x01, 0x01, 0x01]))
        XCTAssertEqual(
            result,
            CommandResult(
                ok: false,
                source: .matchControl,
                error: .noGameInProgress
            )
        )
    }

    func testDecodeSubmitMoveErrorIllegalMove() {
        let result = CommandResult.decode(Data([0x01, 0x02, 0x03]))
        XCTAssertEqual(
            result,
            CommandResult(ok: false, source: .submitMove, error: .illegalMove)
        )
    }

    func testDecodeSubmitMoveErrorNotYourTurn() {
        let result = CommandResult.decode(Data([0x01, 0x02, 0x02]))
        XCTAssertEqual(
            result,
            CommandResult(ok: false, source: .submitMove, error: .notYourTurn)
        )
    }

    func testDecodeStartGameErrorInvalidCommand() {
        let result = CommandResult.decode(Data([0x01, 0x00, 0x05]))
        XCTAssertEqual(
            result,
            CommandResult(ok: false, source: .startGame, error: .invalidCommand)
        )
    }

    func testDecodeErrorUnknownBoardError() {
        // Unknown error code → BoardError is nil but result is still valid
        let result = CommandResult.decode(Data([0x01, 0x00, 0xFF]))
        XCTAssertEqual(
            result,
            CommandResult(ok: false, source: .startGame, error: nil)
        )
    }

    func testDecodeCannotResignForRemotePlayer() {
        let result = CommandResult.decode(Data([0x01, 0x01, 0x04]))
        XCTAssertEqual(
            result,
            CommandResult(
                ok: false,
                source: .matchControl,
                error: .cannotResignForRemotePlayer
            )
        )
    }

    func testDecodeTooShort() {
        XCTAssertNil(CommandResult.decode(Data([0x00, 0x00])))
        XCTAssertNil(CommandResult.decode(Data([0x00])))
        XCTAssertNil(CommandResult.decode(Data()))
    }

    func testDecodeUnknownCommandSource() {
        XCTAssertNil(CommandResult.decode(Data([0x00, 0xFF, 0x00])))
    }
}
