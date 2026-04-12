import Foundation

// MARK: - Event types

enum LichessGameEvent {
    case gameFull(id: String, initialMoves: String, aiLevel: Int)
    case gameState(moves: String, status: String, winner: String?)
}

// MARK: - Errors

enum LichessAPIError: Error {
    case badStatus(Int)
    case invalidResponse
    case missingGameId
}

// MARK: - Protocol for testability

protocol LichessAPIProtocol: Sendable {
    func challengeAI(level: Int, color: String) async throws -> String
    func streamGame(id: String) -> AsyncThrowingStream<LichessGameEvent, Error>
    func makeMove(gameId: String, uci: String) async throws
    func resignGame(gameId: String) async throws
}

// MARK: - LichessAPI

/// Low-level Lichess API client.
///
/// All stored properties are immutable constants, so all methods are
/// nonisolated and the type is Sendable.
final class LichessAPI: LichessAPIProtocol, @unchecked Sendable {
    private let token: String
    private let baseURL: URL
    private let session: URLSession

    init(
        token: String,
        baseURL: URL = URL(string: "https://lichess.org/api")!,
        session: URLSession = .shared
    ) {
        self.token = token
        self.baseURL = baseURL
        self.session = session
    }

    /// Challenge the Lichess AI. Returns the game ID.
    func challengeAI(level: Int, color: String) async throws -> String {
        var request = URLRequest(
            url: baseURL.appendingPathComponent("challenge/ai")
        )
        request.httpMethod = "POST"
        request.setValue(
            "Bearer \(token)",
            forHTTPHeaderField: "Authorization"
        )
        request.setValue(
            "application/x-www-form-urlencoded",
            forHTTPHeaderField: "Content-Type"
        )
        let body = "level=\(level)&color=\(color)"
        request.httpBody = body.data(using: .utf8)

        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw LichessAPIError.invalidResponse
        }
        guard (200..<300).contains(http.statusCode) else {
            throw LichessAPIError.badStatus(http.statusCode)
        }

        guard
            let json = try? JSONSerialization.jsonObject(
                with: data
            ) as? [String: Any],
            let gameId = json["id"] as? String
        else {
            throw LichessAPIError.missingGameId
        }
        return gameId
    }

    /// Open an NDJSON stream for the given game.
    /// Yields game events (GameFull, GameState) as they arrive.
    func streamGame(id: String) -> AsyncThrowingStream<LichessGameEvent, Error>
    {
        var request = URLRequest(
            url: baseURL.appendingPathComponent("board/game/stream/\(id)")
        )
        request.setValue(
            "Bearer \(token)",
            forHTTPHeaderField: "Authorization"
        )

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let (bytes, response) = try await self.session.bytes(
                        for: request
                    )
                    guard let http = response as? HTTPURLResponse else {
                        continuation.finish(
                            throwing: LichessAPIError.invalidResponse
                        )
                        return
                    }
                    guard (200..<300).contains(http.statusCode) else {
                        continuation.finish(
                            throwing: LichessAPIError.badStatus(
                                http.statusCode
                            )
                        )
                        return
                    }

                    var lineBuffer = ""
                    for try await byte in bytes {
                        let char = Character(UnicodeScalar(byte))
                        if char == "\n" {
                            let line = lineBuffer.trimmingCharacters(
                                in: .whitespaces
                            )
                            lineBuffer = ""
                            guard !line.isEmpty else { continue }
                            if let event = Self.parseLine(line) {
                                continuation.yield(event)
                            }
                        } else {
                            lineBuffer.append(char)
                        }
                    }
                    // Flush any remaining data
                    let remaining = lineBuffer.trimmingCharacters(
                        in: .whitespaces
                    )
                    if !remaining.isEmpty,
                        let event = Self.parseLine(remaining)
                    {
                        continuation.yield(event)
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    /// Submit a move to an ongoing game.
    func makeMove(gameId: String, uci: String) async throws {
        var request = URLRequest(
            url: baseURL.appendingPathComponent(
                "board/game/\(gameId)/move/\(uci)"
            )
        )
        request.httpMethod = "POST"
        request.setValue(
            "Bearer \(token)",
            forHTTPHeaderField: "Authorization"
        )

        let (_, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw LichessAPIError.invalidResponse
        }
        guard (200..<300).contains(http.statusCode) else {
            throw LichessAPIError.badStatus(http.statusCode)
        }
    }

    /// Resign an ongoing game.
    func resignGame(gameId: String) async throws {
        var request = URLRequest(
            url: baseURL.appendingPathComponent(
                "board/game/\(gameId)/resign"
            )
        )
        request.httpMethod = "POST"
        request.setValue(
            "Bearer \(token)",
            forHTTPHeaderField: "Authorization"
        )

        let (_, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw LichessAPIError.invalidResponse
        }
        guard (200..<300).contains(http.statusCode) else {
            throw LichessAPIError.badStatus(http.statusCode)
        }
    }

    // MARK: - NDJSON parsing

    private static func parseLine(_ line: String) -> LichessGameEvent? {
        LichessEventParser.parse(line: line)
    }
}

// MARK: - LichessEventParser

/// Internal NDJSON event parser. Extracted for testability.
enum LichessEventParser {
    static func parse(line: String) -> LichessGameEvent? {
        guard
            let data = line.data(using: .utf8),
            let json = try? JSONSerialization.jsonObject(with: data)
                as? [String: Any],
            let type = json["type"] as? String
        else { return nil }

        switch type {
        case "gameFull":
            guard let id = json["id"] as? String else { return nil }
            let initialMoves: String
            if let state = json["state"] as? [String: Any],
                let moves = state["moves"] as? String
            {
                initialMoves = moves
            } else {
                initialMoves = ""
            }
            let aiLevel: Int
            if let opponent = json["opponent"] as? [String: Any],
                let level = opponent["aiLevel"] as? Int
            {
                aiLevel = level
            } else {
                aiLevel = 1
            }
            return .gameFull(
                id: id,
                initialMoves: initialMoves,
                aiLevel: aiLevel
            )

        case "gameState":
            guard let moves = json["moves"] as? String,
                let status = json["status"] as? String
            else { return nil }
            let winner = json["winner"] as? String
            return .gameState(moves: moves, status: status, winner: winner)

        default:
            return nil
        }
    }
}
