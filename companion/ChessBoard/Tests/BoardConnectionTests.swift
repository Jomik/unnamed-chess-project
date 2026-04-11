import XCTest

@testable import ChessBoard

@MainActor
final class BoardConnectionTests: XCTestCase {
    func testInitialState() {
        let board = BoardConnection(connectionState: .ready)
        XCTAssertEqual(board.connectionState, .ready)
        XCTAssertEqual(board.gameState, .initial)
        XCTAssertEqual(board.wifiStatus, .disconnected)
        XCTAssertEqual(board.lichessStatus, .idle)
        XCTAssertNil(board.lastCommandResult)
    }

    func testConfigureWifiSetsConnecting() {
        let board = BoardConnection(connectionState: .ready)
        board.configureWifi(ssid: "test", password: "pass", authMode: .wpa2)
        XCTAssertEqual(board.wifiStatus.state, .connecting)
    }

    func testConfigureWifiNoOpWithoutTransport() {
        let board = BoardConnection(connectionState: .ready)
        board.configureWifi(ssid: "test", password: "pass", authMode: .wpa2)
        // Local state update happens, but no BLE write (transport is nil)
        XCTAssertEqual(board.wifiStatus.state, .connecting)
    }

    func testSetLichessTokenSetsValidating() {
        let board = BoardConnection(connectionState: .ready)
        board.setLichessToken("lip_abc123")
        XCTAssertEqual(board.lichessStatus.state, .validating)
    }

    func testSetLichessTokenRejectsLongToken() {
        let board = BoardConnection(connectionState: .ready)
        let longToken = String(repeating: "a", count: 256)
        board.setLichessToken(longToken)
        // Should not change status because token is too long
        XCTAssertEqual(board.lichessStatus, .idle)
    }

    func testConfigureAndStartSetsPlayerTypes() {
        let board = BoardConnection(connectionState: .ready)
        board.configureAndStart(
            white: .human,
            whiteLevel: 0,
            black: .lichessAi,
            blackLevel: 4
        )
        XCTAssertEqual(board.whitePlayerType, .human)
        XCTAssertEqual(board.blackPlayerType, .lichessAi)
    }

    func testConfigureAndStartGuardsNotReady() {
        let board = BoardConnection(
            connectionState: .scanning,
            whitePlayerType: nil,
            blackPlayerType: nil
        )
        board.configureAndStart(
            white: .human,
            whiteLevel: 0,
            black: .embedded,
            blackLevel: 0
        )
        XCTAssertNil(board.whitePlayerType)
    }

    func testResignClearsLastCommandResult() {
        let board = BoardConnection(
            connectionState: .ready,
            lastCommandResult: CommandResult(
                ok: true,
                source: .startGame,
                message: ""
            )
        )
        board.resign(color: .white)
        XCTAssertNil(board.lastCommandResult)
    }

    func testHumanColor() {
        let board = BoardConnection(
            connectionState: .ready,
            whitePlayerType: .human,
            blackPlayerType: .embedded
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
            gameState: GameState(status: .inProgress, turn: .white),
            whitePlayerType: nil,
            blackPlayerType: nil
        )
        // Before player types are read from firmware: no resign available
        XCTAssertNil(board.resignColor)

        // Simulate BLE transport reading player types from firmware
        board.whitePlayerType = .human
        board.blackPlayerType = .embedded

        // Now resign should work — human is white
        XCTAssertEqual(board.resignColor, .white)
    }

    func testResignColorNilWhenPlayersUnset() {
        let board = BoardConnection(
            connectionState: .ready,
            gameState: GameState(status: .inProgress, turn: .white),
            whitePlayerType: nil,
            blackPlayerType: nil
        )
        XCTAssertNil(board.resignColor)
    }
}
