import XCTest

@testable import ChessBoard

final class CommandResultTests: XCTestCase {
    func testDecodeStartGameSuccess() {
        let result = CommandResult.decode(Data([0x00, 0x00, 0x00]))
        XCTAssertEqual(
            result,
            CommandResult(ok: true, source: .startGame, message: "")
        )
    }

    func testDecodeMatchControlSuccess() {
        let result = CommandResult.decode(Data([0x00, 0x01, 0x00]))
        XCTAssertEqual(
            result,
            CommandResult(ok: true, source: .matchControl, message: "")
        )
    }

    func testDecodeStartGameErrorWithMessage() {
        // "oops" = [0x6F, 0x6F, 0x70, 0x73]
        let data = Data([0x01, 0x00, 0x04]) + "oops".data(using: .utf8)!
        let result = CommandResult.decode(data)
        XCTAssertEqual(
            result,
            CommandResult(ok: false, source: .startGame, message: "oops")
        )
    }

    func testDecodeMatchControlErrorWithMessage() {
        let data = Data([0x01, 0x01, 0x04]) + "oops".data(using: .utf8)!
        let result = CommandResult.decode(data)
        XCTAssertEqual(
            result,
            CommandResult(ok: false, source: .matchControl, message: "oops")
        )
    }

    func testDecodeErrorEmptyMessage() {
        let result = CommandResult.decode(Data([0x01, 0x00, 0x00]))
        XCTAssertEqual(
            result,
            CommandResult(ok: false, source: .startGame, message: "")
        )
    }

    func testDecodeTooShort() {
        XCTAssertNil(CommandResult.decode(Data([0x00, 0x00])))
        XCTAssertNil(CommandResult.decode(Data([0x00])))
        XCTAssertNil(CommandResult.decode(Data()))
    }

    func testDecodeUnknownCommandSource() {
        // command byte 0xFF is unknown
        XCTAssertNil(CommandResult.decode(Data([0x00, 0xFF, 0x00])))
    }

    func testDecodeTruncatedMessage() {
        // msg_len says 4 but only 2 bytes follow (after ok + source + msg_len = 3 header bytes)
        XCTAssertNil(
            CommandResult.decode(Data([0x01, 0x00, 0x04, 0x68, 0x69]))
        )
    }
}
