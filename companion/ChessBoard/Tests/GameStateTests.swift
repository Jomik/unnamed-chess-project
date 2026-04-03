import XCTest

@testable import ChessBoard

final class GameStateTests: XCTestCase {
    func testDecodeIdle() {
        let state = GameState.decode(Data([0x00, 0x00]))
        XCTAssertEqual(state, GameState(status: .idle, turn: .white))
    }

    func testDecodeAwaitingPieces() {
        let state = GameState.decode(Data([0x01, 0x00]))
        XCTAssertEqual(state, GameState(status: .awaitingPieces, turn: .white))
    }

    func testDecodeInProgressBlackTurn() {
        let state = GameState.decode(Data([0x02, 0x01]))
        XCTAssertEqual(state, GameState(status: .inProgress, turn: .black))
    }

    func testDecodeCheckmate() {
        let state = GameState.decode(Data([0x03, 0x00]))
        XCTAssertEqual(state, GameState(status: .checkmate, turn: .white))
    }

    func testDecodeResignation() {
        let state = GameState.decode(Data([0x05, 0x01]))
        XCTAssertEqual(state, GameState(status: .resignation, turn: .black))
    }

    func testDecodeInvalidStatus() {
        XCTAssertNil(GameState.decode(Data([0xFF, 0x00])))
    }

    func testDecodeInvalidTurn() {
        XCTAssertNil(GameState.decode(Data([0x02, 0x02])))
    }

    func testDecodeTooShort() {
        XCTAssertNil(GameState.decode(Data([0x02])))
        XCTAssertNil(GameState.decode(Data()))
    }

    func testIsTerminal() {
        XCTAssertFalse(GameState(status: .idle, turn: .white).isTerminal)
        XCTAssertFalse(
            GameState(status: .awaitingPieces, turn: .white).isTerminal
        )
        XCTAssertFalse(GameState(status: .inProgress, turn: .white).isTerminal)
        XCTAssertTrue(GameState(status: .checkmate, turn: .white).isTerminal)
        XCTAssertTrue(GameState(status: .stalemate, turn: .white).isTerminal)
        XCTAssertTrue(GameState(status: .resignation, turn: .white).isTerminal)
        XCTAssertTrue(GameState(status: .draw, turn: .white).isTerminal)
    }

    func testInitialState() {
        XCTAssertEqual(
            GameState.initial,
            GameState(status: .idle, turn: .white)
        )
    }
}
