import XCTest

@testable import ChessBoard

final class GameStatusTests: XCTestCase {
    func testDecodeIdle() {
        XCTAssertEqual(GameStatus.decode(Data([0x00])), .idle)
    }

    func testDecodeAwaitingPieces() {
        XCTAssertEqual(GameStatus.decode(Data([0x01])), .awaitingPieces)
    }

    func testDecodeInProgress() {
        XCTAssertEqual(GameStatus.decode(Data([0x02])), .inProgress)
    }

    func testDecodeCheckmateWhiteLoser() {
        XCTAssertEqual(
            GameStatus.decode(Data([0x03, 0x00])),
            .checkmate(loser: .white)
        )
    }

    func testDecodeCheckmateBlackLoser() {
        XCTAssertEqual(
            GameStatus.decode(Data([0x03, 0x01])),
            .checkmate(loser: .black)
        )
    }

    func testDecodeCheckmateMissingLoser() {
        XCTAssertNil(GameStatus.decode(Data([0x03])))
    }

    func testDecodeCheckmateInvalidLoser() {
        XCTAssertNil(GameStatus.decode(Data([0x03, 0xFF])))
    }

    func testDecodeStalemate() {
        XCTAssertEqual(GameStatus.decode(Data([0x04])), .stalemate)
    }

    func testDecodeResignedWhite() {
        XCTAssertEqual(
            GameStatus.decode(Data([0x05, 0x00])),
            .resigned(color: .white)
        )
    }

    func testDecodeResignedBlack() {
        XCTAssertEqual(
            GameStatus.decode(Data([0x05, 0x01])),
            .resigned(color: .black)
        )
    }

    func testDecodeResignedMissingColor() {
        XCTAssertNil(GameStatus.decode(Data([0x05])))
    }

    func testDecodeResignedInvalidColor() {
        XCTAssertNil(GameStatus.decode(Data([0x05, 0xFF])))
    }

    func testDecodeInvalidTag() {
        XCTAssertNil(GameStatus.decode(Data([0xFF])))
    }

    func testDecodeEmpty() {
        XCTAssertNil(GameStatus.decode(Data()))
    }

    func testIsTerminalFalseForNonTerminal() {
        XCTAssertFalse(GameStatus.idle.isTerminal)
        XCTAssertFalse(GameStatus.awaitingPieces.isTerminal)
        XCTAssertFalse(GameStatus.inProgress.isTerminal)
    }

    func testIsTerminalTrueForTerminal() {
        XCTAssertTrue(GameStatus.checkmate(loser: .white).isTerminal)
        XCTAssertTrue(GameStatus.checkmate(loser: .black).isTerminal)
        XCTAssertTrue(GameStatus.stalemate.isTerminal)
        XCTAssertTrue(GameStatus.resigned(color: .white).isTerminal)
        XCTAssertTrue(GameStatus.resigned(color: .black).isTerminal)
    }
}
