import XCTest

@testable import ChessBoard

@MainActor
final class BoardConnectionTests: XCTestCase {
    func testInitialState() {
        let board = BoardConnection(connectionState: .ready)
        XCTAssertEqual(board.connectionState, .ready)
        XCTAssertEqual(board.gameStatus, .idle)
        XCTAssertNil(board.lastCommandResult)
    }

    func testConfigureAndStartSetsPlayerTypes() {
        let board = BoardConnection(connectionState: .ready)
        board.configureAndStart(white: .human, black: .remote)
        XCTAssertEqual(board.whitePlayerType, .human)
        XCTAssertEqual(board.blackPlayerType, .remote)
    }

    func testConfigureAndStartGuardsNotReady() {
        let board = BoardConnection(
            connectionState: .scanning,
            whitePlayerType: nil,
            blackPlayerType: nil
        )
        board.configureAndStart(white: .human, black: .remote)
        XCTAssertNil(board.whitePlayerType)
    }

    func testResignClearsLastCommandResult() {
        let board = BoardConnection(
            connectionState: .ready,
            lastCommandResult: CommandResult(
                ok: true,
                source: .startGame,
                error: nil
            )
        )
        board.resign(color: .white)
        XCTAssertNil(board.lastCommandResult)
    }

    func testHumanColor() {
        let board = BoardConnection(
            connectionState: .ready,
            whitePlayerType: .human,
            blackPlayerType: .remote
        )
        XCTAssertEqual(board.humanColor, .white)
    }

    func testHumanColorBothHuman() {
        let board = BoardConnection(
            connectionState: .ready,
            whitePlayerType: .human,
            blackPlayerType: .human
        )
        XCTAssertNil(board.humanColor)
    }

    func testConnectionTimedOutFromScanning() {
        let board = BoardConnection(connectionState: .scanning)
        board.connectionTimedOut()
        XCTAssertEqual(board.connectionState, .notFound)
    }

    func testConnectionTimedOutFromConnecting() {
        let board = BoardConnection(connectionState: .connecting)
        board.connectionTimedOut()
        XCTAssertEqual(board.connectionState, .connectionFailed)
    }

    func testConnectionTimedOutFromDiscovering() {
        let board = BoardConnection(
            connectionState: .discoveringServices
        )
        board.connectionTimedOut()
        XCTAssertEqual(board.connectionState, .setupFailed)
    }

    func testResignColorAfterReconnect() {
        // Simulate fresh app start: player types are nil, game still in progress on firmware
        let board = BoardConnection(
            connectionState: .ready,
            gameStatus: .inProgress,
            whitePlayerType: nil,
            blackPlayerType: nil
        )
        // Before player types are read from firmware: no resign available
        XCTAssertNil(board.resignColor)

        // Simulate BLE transport reading player types from firmware
        board.whitePlayerType = .human
        board.blackPlayerType = .remote

        // Now resign should work — human is white
        XCTAssertEqual(board.resignColor, .white)
    }

    func testResignColorHumanVsHuman() {
        // In human-vs-human, resign is not available (no turn tracking)
        let board = BoardConnection(
            connectionState: .ready,
            whitePlayerType: .human,
            blackPlayerType: .human
        )
        XCTAssertNil(board.resignColor)
    }

    func testResignColorNilWhenPlayersUnset() {
        let board = BoardConnection(
            connectionState: .ready,
            gameStatus: .inProgress,
            whitePlayerType: nil,
            blackPlayerType: nil
        )
        XCTAssertNil(board.resignColor)
    }
}
