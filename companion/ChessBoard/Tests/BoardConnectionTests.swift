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
        let board = BoardConnection(
            connectionState: .ready,
            gameStatus: .inProgress,
            currentPosition:
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
            whitePlayerType: .human,
            blackPlayerType: .human
        )
        XCTAssertEqual(board.resignColor, .black)
    }

    func testResignColorHumanVsHumanNoPosition() {
        let board = BoardConnection(
            connectionState: .ready,
            gameStatus: .inProgress,
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

    func testCancelGameClearsLastCommandResult() {
        let board = BoardConnection(
            connectionState: .ready,
            lastCommandResult: CommandResult(
                ok: true,
                source: .startGame,
                error: nil
            )
        )
        board.cancelGame()
        XCTAssertNil(board.lastCommandResult)
    }

    func testSubmitMoveClearsLastCommandResult() {
        let board = BoardConnection(
            connectionState: .ready,
            lastCommandResult: CommandResult(
                ok: false,
                source: .startGame,
                error: nil
            )
        )
        board.submitMove("e2e4")
        XCTAssertNil(board.lastCommandResult)
    }

    func testSubmitMoveTooLongIsIgnored() {
        let board = BoardConnection(
            connectionState: .ready,
            lastCommandResult: CommandResult(
                ok: true,
                source: .startGame,
                error: nil
            )
        )
        // A string longer than 255 bytes must not clear lastCommandResult
        let longMove = String(repeating: "a", count: 256)
        board.submitMove(longMove)
        XCTAssertNotNil(board.lastCommandResult)
    }

    func testConfigureAndStartWritesCorrectBytes() {
        let transport = MockTransport()
        let board = BoardConnection(transport: transport)
        board.connectionState = .ready

        board.configureAndStart(white: .human, black: .remote)

        XCTAssertEqual(transport.writeCallCount, 1)
        XCTAssertEqual(transport.writeArgs[0].data, Data([0x00, 0x01]))
        XCTAssertEqual(transport.writeArgs[0].characteristic, GATT.startGame)
    }
}
