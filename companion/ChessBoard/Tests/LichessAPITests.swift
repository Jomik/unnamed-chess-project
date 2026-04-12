import XCTest

@testable import ChessBoard

// MARK: - LichessAPITests

final class LichessAPITests: XCTestCase {
    // MARK: - parseLine tests (via streamGame parsing logic)
    // We test parsing indirectly by feeding sample Lichess NDJSON responses
    // through a mock URLSession.

    func testParseGameFullEvent() throws {
        let line = """
            {"type":"gameFull","id":"abc123","opponent":{"aiLevel":3},"state":{"moves":"e2e4 e7e5","status":"started"}}
            """
        let event = parseLineForTest(line)
        guard case .gameFull(let id, let initialMoves, let aiLevel) = event
        else {
            XCTFail("Expected gameFull, got \(String(describing: event))")
            return
        }
        XCTAssertEqual(id, "abc123")
        XCTAssertEqual(initialMoves, "e2e4 e7e5")
        XCTAssertEqual(aiLevel, 3)
    }

    func testParseGameFullEventNoMoves() throws {
        let line = """
            {"type":"gameFull","id":"xyz","opponent":{"aiLevel":1},"state":{"moves":"","status":"started"}}
            """
        let event = parseLineForTest(line)
        guard case .gameFull(let id, let initialMoves, let aiLevel) = event
        else {
            XCTFail("Expected gameFull, got \(String(describing: event))")
            return
        }
        XCTAssertEqual(id, "xyz")
        XCTAssertEqual(initialMoves, "")
        XCTAssertEqual(aiLevel, 1)
    }

    func testParseGameFullEventMissingId() {
        let line = """
            {"type":"gameFull","opponent":{"aiLevel":1},"state":{"moves":""}}
            """
        let event = parseLineForTest(line)
        XCTAssertNil(event, "Should return nil when id is missing")
    }

    func testParseGameFullEventMissingOpponent() {
        // When opponent key is absent, aiLevel defaults to 1
        let line = """
            {"type":"gameFull","id":"abc","state":{"moves":""}}
            """
        let event = parseLineForTest(line)
        guard case .gameFull(let id, _, let aiLevel) = event else {
            XCTFail("Expected gameFull, got \(String(describing: event))")
            return
        }
        XCTAssertEqual(id, "abc")
        XCTAssertEqual(aiLevel, 1)
    }

    func testParseGameStateEvent() {
        let line = """
            {"type":"gameState","moves":"e2e4 e7e5 g1f3","status":"started","winner":null}
            """
        let event = parseLineForTest(line)
        guard case .gameState(let moves, let status, let winner) = event else {
            XCTFail("Expected gameState, got \(String(describing: event))")
            return
        }
        XCTAssertEqual(moves, "e2e4 e7e5 g1f3")
        XCTAssertEqual(status, "started")
        XCTAssertNil(winner)
    }

    func testParseGameStateEventWithWinner() {
        let line = """
            {"type":"gameState","moves":"e2e4 e7e5","status":"mate","winner":"white"}
            """
        let event = parseLineForTest(line)
        guard case .gameState(let moves, let status, let winner) = event else {
            XCTFail("Expected gameState, got \(String(describing: event))")
            return
        }
        XCTAssertEqual(moves, "e2e4 e7e5")
        XCTAssertEqual(status, "mate")
        XCTAssertEqual(winner, "white")
    }

    func testParseGameStateMissingMoves() {
        let line = """
            {"type":"gameState","status":"started"}
            """
        let event = parseLineForTest(line)
        XCTAssertNil(event, "Should return nil when moves field is missing")
    }

    func testParseGameStateMissingStatus() {
        let line = """
            {"type":"gameState","moves":"e2e4"}
            """
        let event = parseLineForTest(line)
        XCTAssertNil(event, "Should return nil when status field is missing")
    }

    func testParseUnknownType() {
        let line = """
            {"type":"ping"}
            """
        let event = parseLineForTest(line)
        XCTAssertNil(event, "Should return nil for unknown event types")
    }

    func testParseEmptyString() {
        XCTAssertNil(parseLineForTest(""))
    }

    func testParseInvalidJSON() {
        XCTAssertNil(parseLineForTest("not json"))
    }

    func testParseGameFullMissingStateKey() {
        // State key absent → initialMoves defaults to ""
        let line = """
            {"type":"gameFull","id":"abc","opponent":{"aiLevel":2}}
            """
        let event = parseLineForTest(line)
        guard case .gameFull(_, let initialMoves, _) = event else {
            XCTFail("Expected gameFull")
            return
        }
        XCTAssertEqual(initialMoves, "")
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
