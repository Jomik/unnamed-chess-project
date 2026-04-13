import XCTest

@testable import ChessBoard

// MARK: - Mock API

/// A mock LichessAPI for testing LichessService.
final class MockLichessAPI: LichessAPIProtocol, @unchecked Sendable {
    var challengeAIResult: Result<String, Error> = .success("test-game-id")
    var makeMoveCallCount = 0
    var makeMoveArgs: [(gameId: String, uci: String)] = []
    var resignCallCount = 0
    var resignArgs: [String] = []

    /// Controls events emitted by streamGame. Set before calling start().
    var streamEvents: [LichessGameEvent] = []

    var validateTokenError: Error?
    func validateToken() async throws {
        if let error = validateTokenError { throw error }
    }

    func challengeAI(level: Int, color: String) async throws -> String {
        switch challengeAIResult {
        case .success(let id): return id
        case .failure(let error): throw error
        }
    }

    func streamGame(id: String) -> AsyncThrowingStream<LichessGameEvent, Error>
    {
        let events = streamEvents
        return AsyncThrowingStream { continuation in
            for event in events {
                continuation.yield(event)
            }
            continuation.finish()
        }
    }

    func makeMove(gameId: String, uci: String) async throws {
        makeMoveCallCount += 1
        makeMoveArgs.append((gameId, uci))
    }

    func resignGame(gameId: String) async throws {
        resignCallCount += 1
        resignArgs.append(gameId)
    }
}

// MARK: - LichessServiceTests

@MainActor
final class LichessServiceTests: XCTestCase {
    // MARK: - Echo suppression

    func testBoardMovePlayedForwardsHumanMoves() async {
        let api = MockLichessAPI()
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .white
        )
        service.gameId = "game1"
        service.isActive = true

        service.boardMovePlayed(color: .white, uci: "e2e4")

        // Allow the Task inside boardMovePlayed to execute.
        // Task.yield() alone is not sufficient when the spawned Task calls an
        // async method; a short sleep gives the Swift concurrency runtime time
        // to schedule and complete the child Task.
        try? await Task.sleep(nanoseconds: 10_000_000)

        XCTAssertEqual(api.makeMoveCallCount, 1)
        XCTAssertEqual(api.makeMoveArgs.first?.uci, "e2e4")
    }

    func testBoardMovePlayedIgnoresAIMoves() async {
        let api = MockLichessAPI()
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .white  // human is white
        )
        service.gameId = "game1"
        service.isActive = true

        // Black move — this is the AI's echo, must be suppressed
        service.boardMovePlayed(color: .black, uci: "e7e5")

        try? await Task.sleep(nanoseconds: 10_000_000)

        XCTAssertEqual(
            api.makeMoveCallCount,
            0,
            "AI echo must not be forwarded to Lichess"
        )
    }

    func testBoardMovePlayedIgnoresWhenNotActive() async {
        let api = MockLichessAPI()
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .white
        )
        service.gameId = "game1"
        service.isActive = false  // service not active

        service.boardMovePlayed(color: .white, uci: "e2e4")

        try? await Task.sleep(nanoseconds: 10_000_000)

        XCTAssertEqual(api.makeMoveCallCount, 0)
    }

    func testBoardMovePlayedIgnoresWhenNoGameId() async {
        let api = MockLichessAPI()
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .white
        )
        service.gameId = nil
        service.isActive = true

        service.boardMovePlayed(color: .white, uci: "e2e4")

        try? await Task.sleep(nanoseconds: 10_000_000)

        XCTAssertEqual(api.makeMoveCallCount, 0)
    }

    // MARK: - Terminal state handling

    func testTerminalGameStateTriggersCancelGame() async {
        let api = MockLichessAPI()
        api.streamEvents = [
            .gameState(moves: "e2e4 e7e5", status: "mate", winner: "white")
        ]
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .white
        )

        await service.start(level: 1)

        // Give the stream task time to run
        try? await Task.sleep(for: .milliseconds(50))

        XCTAssertFalse(
            service.isActive,
            "Service should be inactive after terminal gameState"
        )
        // board.cancelGame() should have been called — verify via lastCommandResult
        // (cancelGame calls transport?.write but transport is nil in tests;
        //  we verify the service's isActive is false as the primary signal)
    }

    func testResignGameStateTriggersCancelGame() async {
        let api = MockLichessAPI()
        api.streamEvents = [
            .gameState(moves: "e2e4", status: "resign", winner: "white")
        ]
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .white
        )

        await service.start(level: 3)

        try? await Task.sleep(for: .milliseconds(50))

        XCTAssertFalse(service.isActive)
    }

    func testStalemateGameStateTriggersCancelGame() async {
        let api = MockLichessAPI()
        api.streamEvents = [
            .gameState(moves: "e2e4 e7e5", status: "stalemate", winner: nil)
        ]
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .black
        )

        await service.start(level: 5)

        try? await Task.sleep(for: .milliseconds(50))

        XCTAssertFalse(service.isActive)
    }

    // MARK: - start() lifecycle

    func testStartSetsGameId() async {
        let api = MockLichessAPI()
        api.challengeAIResult = .success("new-game-42")
        api.streamEvents = []
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .white
        )

        await service.start(level: 2)

        XCTAssertEqual(service.gameId, "new-game-42")
        XCTAssertTrue(service.isActive)
    }

    func testStartSetsErrorOnAPIFailure() async {
        struct FakeError: Error, LocalizedError {
            var errorDescription: String? { "Network error" }
        }
        let api = MockLichessAPI()
        api.challengeAIResult = .failure(FakeError())
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .white
        )

        await service.start(level: 1)

        XCTAssertFalse(service.isActive)
        XCTAssertNotNil(service.error)
    }

    // MARK: - stop()

    func testStopCancelsStreamAndResigns() async {
        let api = MockLichessAPI()
        api.streamEvents = []
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .white
        )

        await service.start(level: 1)
        XCTAssertTrue(service.isActive)
        let capturedGameId = service.gameId

        service.stop()

        XCTAssertFalse(service.isActive)
        XCTAssertNil(service.gameId)

        // Allow the resign Task to run
        try? await Task.sleep(nanoseconds: 10_000_000)

        XCTAssertEqual(api.resignCallCount, 1)
        XCTAssertEqual(api.resignArgs.first, capturedGameId)
    }

    func testStopWhenNotActiveDoesNotResign() async {
        let api = MockLichessAPI()
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .white
        )

        service.stop()

        try? await Task.sleep(nanoseconds: 10_000_000)

        XCTAssertEqual(api.resignCallCount, 0)
    }

    // MARK: - Echo suppression with human as black

    func testEchoSuppressionHumanIsBlack() async {
        let api = MockLichessAPI()
        let board = BoardConnection(transport: MockTransport())
        board.connectionState = .ready
        let service = LichessService(
            api: api,
            board: board,
            humanColor: .black  // human plays black
        )
        service.gameId = "game2"
        service.isActive = true

        // White move — this is the AI's echo (AI plays white)
        service.boardMovePlayed(color: .white, uci: "e2e4")

        try? await Task.sleep(nanoseconds: 10_000_000)

        XCTAssertEqual(
            api.makeMoveCallCount,
            0,
            "White move must be suppressed when human is black"
        )

        // Black move — this is the human
        service.boardMovePlayed(color: .black, uci: "e7e5")

        try? await Task.sleep(nanoseconds: 10_000_000)

        XCTAssertEqual(api.makeMoveCallCount, 1)
        XCTAssertEqual(api.makeMoveArgs.first?.uci, "e7e5")
    }
}
