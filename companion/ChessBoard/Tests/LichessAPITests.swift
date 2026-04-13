import Testing

@testable import ChessBoard

// MARK: - LichessAPITests

@Suite struct LichessAPITests {
    // MARK: - parseLine tests (via streamGame parsing logic)
    // We test parsing indirectly by feeding sample Lichess NDJSON responses
    // through a mock URLSession.

    @Test func parseGameFullEvent() throws {
        let line = """
            {"type":"gameFull","id":"abc123","opponent":{"aiLevel":3},"state":{"moves":"e2e4 e7e5","status":"started"}}
            """
        let event = try #require(parseLineForTest(line))
        guard case .gameFull(let id, let initialMoves, let aiLevel) = event
        else {
            Issue.record("Expected gameFull, got \(event)")
            return
        }
        #expect(id == "abc123")
        #expect(initialMoves == "e2e4 e7e5")
        #expect(aiLevel == 3)
    }

    @Test func parseGameFullEventNoMoves() throws {
        let line = """
            {"type":"gameFull","id":"xyz","opponent":{"aiLevel":1},"state":{"moves":"","status":"started"}}
            """
        let event = try #require(parseLineForTest(line))
        guard case .gameFull(let id, let initialMoves, let aiLevel) = event
        else {
            Issue.record("Expected gameFull, got \(event)")
            return
        }
        #expect(id == "xyz")
        #expect(initialMoves == "")
        #expect(aiLevel == 1)
    }

    @Test func parseGameFullEventMissingId() {
        let line = """
            {"type":"gameFull","opponent":{"aiLevel":1},"state":{"moves":""}}
            """
        let event = parseLineForTest(line)
        #expect(event == nil, "Should return nil when id is missing")
    }

    @Test func parseGameFullEventMissingOpponent() throws {
        // When opponent key is absent, aiLevel defaults to 1
        let line = """
            {"type":"gameFull","id":"abc","state":{"moves":""}}
            """
        let event = try #require(parseLineForTest(line))
        guard case .gameFull(let id, _, let aiLevel) = event else {
            Issue.record("Expected gameFull, got \(event)")
            return
        }
        #expect(id == "abc")
        #expect(aiLevel == 1)
    }

    @Test func parseGameStateEvent() throws {
        let line = """
            {"type":"gameState","moves":"e2e4 e7e5 g1f3","status":"started","winner":null}
            """
        let event = try #require(parseLineForTest(line))
        guard case .gameState(let moves, let status, let winner) = event else {
            Issue.record("Expected gameState, got \(event)")
            return
        }
        #expect(moves == "e2e4 e7e5 g1f3")
        #expect(status == "started")
        #expect(winner == nil)
    }

    @Test func parseGameStateEventWithWinner() throws {
        let line = """
            {"type":"gameState","moves":"e2e4 e7e5","status":"mate","winner":"white"}
            """
        let event = try #require(parseLineForTest(line))
        guard case .gameState(let moves, let status, let winner) = event else {
            Issue.record("Expected gameState, got \(event)")
            return
        }
        #expect(moves == "e2e4 e7e5")
        #expect(status == "mate")
        #expect(winner == "white")
    }

    @Test func parseGameStateMissingMoves() {
        let line = """
            {"type":"gameState","status":"started"}
            """
        let event = parseLineForTest(line)
        #expect(event == nil, "Should return nil when moves field is missing")
    }

    @Test func parseGameStateMissingStatus() {
        let line = """
            {"type":"gameState","moves":"e2e4"}
            """
        let event = parseLineForTest(line)
        #expect(event == nil, "Should return nil when status field is missing")
    }

    @Test func parseUnknownType() {
        let line = """
            {"type":"ping"}
            """
        let event = parseLineForTest(line)
        #expect(event == nil, "Should return nil for unknown event types")
    }

    @Test func parseEmptyString() {
        #expect(parseLineForTest("") == nil)
    }

    @Test func parseInvalidJSON() {
        #expect(parseLineForTest("not json") == nil)
    }

    @Test func parseGameFullMissingStateKey() throws {
        // State key absent → initialMoves defaults to ""
        let line = """
            {"type":"gameFull","id":"abc","opponent":{"aiLevel":2}}
            """
        let event = try #require(parseLineForTest(line))
        guard case .gameFull(_, let initialMoves, _) = event else {
            Issue.record("Expected gameFull")
            return
        }
        #expect(initialMoves == "")
    }

    // MARK: - Helpers

    /// Reaches into LichessAPI's parseLine via a mock stream.
    /// Since parseLine is private, we exercise it through a MockURLSession
    /// that serves a single-line NDJSON response.
    private func parseLineForTest(_ line: String) -> LichessGameEvent? {
        // We can't call parseLine directly (it's private/static).
        // Instead we use the testable LichessEventParser helper.
        LichessEventParser.parse(line: line)
    }
}
