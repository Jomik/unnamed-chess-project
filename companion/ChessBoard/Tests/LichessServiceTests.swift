import Foundation
import Testing

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

@MainActor @Suite(.serialized)
struct LichessServiceTests {
    // MARK: - Echo suppression

    @Test func boardMovePlayedForwardsHumanMoves() async {
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

        #expect(api.makeMoveCallCount == 1)
        #expect(api.makeMoveArgs.first?.uci == "e2e4")
    }

    @Test func boardMovePlayedIgnoresAIMoves() async {
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

        #expect(
            api.makeMoveCallCount == 0,
            "AI echo must not be forwarded to Lichess"
        )
    }

    @Test func boardMovePlayedIgnoresWhenNotActive() async {
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

        #expect(api.makeMoveCallCount == 0)
    }

    @Test func boardMovePlayedIgnoresWhenNoGameId() async {
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

        #expect(api.makeMoveCallCount == 0)
    }

    // MARK: - Terminal state handling

    @Test func terminalGameStateTriggersCancelGame() async {
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

        #expect(
            !service.isActive,
            "Service should be inactive after terminal gameState"
        )
        // board.cancelGame() should have been called — verify via isActive
        // (cancelGame writes through MockTransport; isActive is the primary signal)
    }

    @Test func resignGameStateTriggersCancelGame() async {
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

        #expect(!service.isActive)
    }

    @Test func stalemateGameStateTriggersCancelGame() async {
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

        #expect(!service.isActive)
    }

    // MARK: - start() lifecycle

    @Test func startSetsGameId() async {
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

        #expect(service.gameId == "new-game-42")
        #expect(service.isActive)
    }

    @Test func startSetsErrorOnAPIFailure() async {
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

        #expect(!service.isActive)
        #expect(service.error != nil)
    }

    // MARK: - stop()

    @Test func stopCancelsStreamAndResigns() async {
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
        #expect(service.isActive)
        let capturedGameId = service.gameId

        service.stop()

        #expect(!service.isActive)
        #expect(service.gameId == nil)

        // Allow the resign Task to run
        try? await Task.sleep(nanoseconds: 10_000_000)

        #expect(api.resignCallCount == 1)
        #expect(api.resignArgs.first == capturedGameId)
    }

    @Test func stopWhenNotActiveDoesNotResign() async {
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

        #expect(api.resignCallCount == 0)
    }

    // MARK: - Echo suppression with human as black

    @Test func echoSuppressionHumanIsBlack() async {
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

        #expect(
            api.makeMoveCallCount == 0,
            "White move must be suppressed when human is black"
        )

        // Black move — this is the human
        service.boardMovePlayed(color: .black, uci: "e7e5")

        try? await Task.sleep(nanoseconds: 10_000_000)

        #expect(api.makeMoveCallCount == 1)
        #expect(api.makeMoveArgs.first?.uci == "e7e5")
    }
}
