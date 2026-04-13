import XCTest

@testable import ChessBoard

@MainActor
final class BoardConnectionTests: XCTestCase {
    func testInitialState() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        XCTAssertEqual(board.connectionState, .ready)
        XCTAssertEqual(board.gameStatus, .idle)
        XCTAssertNil(board.lastCommandResult)
    }

    func testConfigureAndStartSetsPlayerTypes() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.configureAndStart(white: .human, black: .remote)
        XCTAssertEqual(board.whitePlayerType, .human)
        XCTAssertEqual(board.blackPlayerType, .remote)
    }

    func testConfigureAndStartGuardsNotReady() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .scanning
        board.configureAndStart(white: .human, black: .remote)
        XCTAssertNil(board.whitePlayerType)
    }

    func testResignClearsLastCommandResult() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.lastCommandResult = CommandResult(
            ok: true,
            source: .startGame,
            error: nil
        )
        board.resign(color: .white)
        XCTAssertNil(board.lastCommandResult)
    }

    func testHumanColor() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.whitePlayerType = .human
        board.blackPlayerType = .remote
        XCTAssertEqual(board.humanColor, .white)
    }

    func testHumanColorBothHuman() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.whitePlayerType = .human
        board.blackPlayerType = .human
        XCTAssertNil(board.humanColor)
    }

    func testConnectionTimedOutFromScanning() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .scanning
        board.connectionTimedOut()
        XCTAssertEqual(board.connectionState, .notFound)
    }

    func testConnectionTimedOutFromConnecting() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .connecting
        board.connectionTimedOut()
        XCTAssertEqual(board.connectionState, .connectionFailed)
    }

    func testConnectionTimedOutFromDiscovering() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .discoveringServices
        board.connectionTimedOut()
        XCTAssertEqual(board.connectionState, .setupFailed)
    }

    func testResignColorAfterReconnect() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.gameStatus = .inProgress
        XCTAssertNil(board.resignColor)

        board.whitePlayerType = .human
        board.blackPlayerType = .remote
        XCTAssertEqual(board.resignColor, .white)
    }

    func testResignColorHumanVsHuman() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.gameStatus = .inProgress
        board.currentPosition =
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
        board.whitePlayerType = .human
        board.blackPlayerType = .human
        XCTAssertEqual(board.resignColor, .black)
    }

    func testResignColorHumanVsHumanNoPosition() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.gameStatus = .inProgress
        board.whitePlayerType = .human
        board.blackPlayerType = .human
        XCTAssertNil(board.resignColor)
    }

    func testResignColorNilWhenPlayersUnset() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.gameStatus = .inProgress
        XCTAssertNil(board.resignColor)
    }

    func testCancelGameClearsLastCommandResult() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.lastCommandResult = CommandResult(
            ok: true,
            source: .startGame,
            error: nil
        )
        board.cancelGame()
        XCTAssertNil(board.lastCommandResult)
    }

    func testSubmitMoveClearsLastCommandResult() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.lastCommandResult = CommandResult(
            ok: false,
            source: .startGame,
            error: nil
        )
        board.submitMove("e2e4")
        XCTAssertNil(board.lastCommandResult)
    }

    func testSubmitMoveTooLongIsIgnored() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.lastCommandResult = CommandResult(
            ok: true,
            source: .startGame,
            error: nil
        )
        let longMove = String(repeating: "a", count: 256)
        board.submitMove(longMove)
        XCTAssertNotNil(board.lastCommandResult)
    }
}
