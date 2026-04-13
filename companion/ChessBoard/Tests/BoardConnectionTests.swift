import Foundation
import Testing

@testable import ChessBoard

@MainActor @Suite struct BoardConnectionTests {
    @Test func initialState() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        #expect(board.connectionState == .ready)
        #expect(board.gameStatus == .idle)
        #expect(board.lastCommandResult == nil)
    }

    @Test func configureAndStartSetsPlayerTypes() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.configureAndStart(white: .human, black: .remote)
        #expect(board.whitePlayerType == .human)
        #expect(board.blackPlayerType == .remote)
    }

    @Test func configureAndStartGuardsNotReady() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .scanning
        board.configureAndStart(white: .human, black: .remote)
        #expect(board.whitePlayerType == nil)
    }

    @Test func resignClearsLastCommandResult() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.lastCommandResult = CommandResult(
            ok: true,
            source: .startGame,
            error: nil
        )
        board.resign(color: .white)
        #expect(board.lastCommandResult == nil)
    }

    @Test func humanColor() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.whitePlayerType = .human
        board.blackPlayerType = .remote
        #expect(board.humanColor == .white)
    }

    @Test func humanColorBothHuman() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.whitePlayerType = .human
        board.blackPlayerType = .human
        #expect(board.humanColor == nil)
    }

    @Test func connectionTimedOutFromScanning() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .scanning
        board.connectionTimedOut()
        #expect(board.connectionState == .notFound)
    }

    @Test func connectionTimedOutFromConnecting() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .connecting
        board.connectionTimedOut()
        #expect(board.connectionState == .connectionFailed)
    }

    @Test func connectionTimedOutFromDiscovering() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .discoveringServices
        board.connectionTimedOut()
        #expect(board.connectionState == .setupFailed)
    }

    @Test func resignColorAfterReconnect() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.gameStatus = .inProgress
        #expect(board.resignColor == nil)

        board.whitePlayerType = .human
        board.blackPlayerType = .remote
        #expect(board.resignColor == .white)
    }

    @Test func resignColorHumanVsHuman() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.gameStatus = .inProgress
        board.currentPosition =
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
        board.whitePlayerType = .human
        board.blackPlayerType = .human
        #expect(board.resignColor == .black)
    }

    @Test func resignColorHumanVsHumanNoPosition() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.gameStatus = .inProgress
        board.whitePlayerType = .human
        board.blackPlayerType = .human
        #expect(board.resignColor == nil)
    }

    @Test func resignColorNilWhenPlayersUnset() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.gameStatus = .inProgress
        #expect(board.resignColor == nil)
    }

    @Test func cancelGameClearsLastCommandResult() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.lastCommandResult = CommandResult(
            ok: true,
            source: .startGame,
            error: nil
        )
        board.cancelGame()
        #expect(board.lastCommandResult == nil)
    }

    @Test func submitMoveClearsLastCommandResult() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.lastCommandResult = CommandResult(
            ok: false,
            source: .startGame,
            error: nil
        )
        board.submitMove("e2e4")
        #expect(board.lastCommandResult == nil)
    }

    @Test func submitMoveTooLongIsIgnored() {
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        board.lastCommandResult = CommandResult(
            ok: true,
            source: .startGame,
            error: nil
        )
        let longMove = String(repeating: "a", count: 256)
        board.submitMove(longMove)
        #expect(board.lastCommandResult != nil)
    }

    @Test func configureAndStartWritesCorrectBytes() {
        let transport = MockTransport()
        let board = BoardConnection(transport: transport)
        board.connectionState = .ready

        board.configureAndStart(white: .human, black: .remote)

        #expect(transport.writeCallCount == 1)
        #expect(transport.writeArgs[0].data == Data([0x00, 0x01]))
        #expect(transport.writeArgs[0].characteristic == GATT.startGame)
    }
}
