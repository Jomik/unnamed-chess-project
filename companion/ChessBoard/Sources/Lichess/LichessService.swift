import Foundation
import Observation

// MARK: - Terminal status set

private let terminalStatuses: Set<String> = [
    "mate", "resign", "stalemate", "timeout", "draw", "outoftime", "cheat",
    "noStart", "unknownFinish", "variantEnd", "aborted",
]

// MARK: - User-facing messages for terminal statuses

private func terminalMessage(for status: String) -> String? {
    switch status {
    case "mate": return nil  // Board detects checkmate from position
    case "resign": return "Opponent resigned"
    case "timeout", "outoftime": return "Game ended: timeout"
    case "aborted", "noStart": return "Game aborted"
    case "draw", "stalemate": return "Game ended in a draw"
    default: return "Game over"
    }
}

// MARK: - LichessService

/// Orchestrates a Lichess game, bridging between the board and Lichess API.
///
/// Lifecycle:
/// 1. `start()` → challenges AI, opens stream
/// 2. Receives board's `MovePlayed` → filters by humanColor → sends to Lichess
///    via `makeMove`
/// 3. Receives Lichess AI move → sends to board via `board.submitMove()`
/// 4. Lichess game-over event → calls `board.cancelGame()` to sync board state
/// 5. Board game ends → `stop()`
@MainActor
@Observable
class LichessService {
    private let api: any LichessAPIProtocol
    private let board: BoardConnection
    /// Which color is the human on the physical board.
    private let humanColor: Turn

    var gameId: String?
    var isActive: Bool = false
    var error: String?

    private var streamTask: Task<Void, Never>?

    /// Number of half-moves (plies) seen in the last stream event.
    /// Used to detect new AI moves without re-processing already-seen ones.
    private var lastMoveCount: Int = 0

    init(
        token: String,
        board: BoardConnection,
        humanColor: Turn
    ) {
        self.api = LichessAPI(token: token)
        self.board = board
        self.humanColor = humanColor
    }

    /// Designated initializer for testing — accepts any LichessAPIProtocol.
    init(
        api: any LichessAPIProtocol,
        board: BoardConnection,
        humanColor: Turn
    ) {
        self.api = api
        self.board = board
        self.humanColor = humanColor
    }

    func start(level: Int) async {
        guard !isActive else { return }
        error = nil
        isActive = true
        lastMoveCount = 0

        let colorParam = humanColor == .white ? "white" : "black"

        do {
            let id = try await api.challengeAI(level: level, color: colorParam)
            gameId = id
            startStream(gameId: id)
        } catch {
            self.error = error.localizedDescription
            isActive = false
            board.cancelGame()
        }
    }

    /// Called when the board emits a MovePlayed event.
    /// Only forwards to Lichess if the move is from the human player.
    func boardMovePlayed(color: Turn, uci: String) {
        // Echo suppression: only forward moves from the human side.
        // When LichessService submits an AI move via board.submitMove(),
        // the board echoes it back as a MovePlayed event. We must ignore
        // those echoes.
        guard color == humanColor else { return }
        guard let id = gameId, isActive else { return }
        Task { @MainActor in
            // Re-check that the service is still active and the game ID hasn't changed
            guard isActive, let currentGameId = gameId, currentGameId == id
            else { return }
            do {
                try await api.makeMove(gameId: id, uci: uci)
            } catch {
                // Retry once before surfacing the error
                do {
                    try await api.makeMove(gameId: id, uci: uci)
                } catch {
                    self.error =
                        "Move submission failed: \(error.localizedDescription)"
                }
            }
        }
    }

    func stop() {
        streamTask?.cancel()
        streamTask = nil
        isActive = false
        if let id = gameId {
            gameId = nil
            Task { try? await api.resignGame(gameId: id) }
        }
    }

    // MARK: - Private

    private func startStream(gameId: String) {
        streamTask?.cancel()
        streamTask = Task { [weak self] in
            guard let self else { return }
            do {
                try await self.runStream(gameId: gameId)
            } catch {
                // First attempt failed — try once more if not cancelled
                guard !Task.isCancelled else { return }
                do {
                    try await self.runStream(gameId: gameId)
                } catch {
                    guard !Task.isCancelled else { return }
                    self.error = error.localizedDescription
                    self.isActive = false
                }
            }
        }
    }

    /// Runs the game stream until completion or error.
    private func runStream(gameId: String) async throws {
        let stream = api.streamGame(id: gameId)
        for try await event in stream {
            handleEvent(event)
        }
        // Normal stream completion (server closed the connection without a
        // terminal event) — mark service as inactive.
        if isActive {
            isActive = false
        }
    }

    private func handleEvent(_ event: LichessGameEvent) {
        switch event {
        case .gameFull(_, let initialMoves, _):
            let plies = movesCount(initialMoves)
            // If the AI moved first (human is black), submit that initial move.
            // On stream reconnect, only submit if there are genuinely new moves
            // beyond what we've already processed (use lastMoveCount as baseline).
            if plies > lastMoveCount {
                submitLatestAIMove(
                    from: initialMoves,
                    previousCount: lastMoveCount
                )
            }
            lastMoveCount = plies

        case .gameState(let moves, let status, _):
            if terminalStatuses.contains(status) {
                // For mate, the board detects checkmate from position itself.
                // For all other terminal states, force the board out of the
                // current game and surface a human-readable message.
                if status != "mate" {
                    board.cancelGame()
                    error = terminalMessage(for: status)
                }
                streamTask?.cancel()
                streamTask = nil
                isActive = false
                return
            }
            let plies = movesCount(moves)
            if plies > lastMoveCount {
                submitLatestAIMove(
                    from: moves,
                    previousCount: lastMoveCount
                )
                lastMoveCount = plies
            }
        }
    }

    /// Extracts and submits the latest AI move from the Lichess move list.
    ///
    /// The AI plays the opposite color from humanColor. In a space-separated
    /// UCI move list, white plays on even indices (0, 2, 4...) and black on
    /// odd indices (1, 3, 5...). We find the last move that belongs to the AI.
    private func submitLatestAIMove(
        from moves: String,
        previousCount: Int
    ) {
        let allMoves = moves.split(separator: " ").map(String.init)
        guard !allMoves.isEmpty else { return }

        // The AI color is opposite of the human.
        // White moves are at even indices (0-based), black at odd.
        let aiIsBlack = humanColor == .white

        // Look for the most recent AI move among the new moves
        for idx in stride(
            from: allMoves.count - 1,
            through: previousCount,
            by: -1
        ) {
            let moveIsBlack = (idx % 2) == 1
            if moveIsBlack == aiIsBlack {
                board.submitMove(allMoves[idx])
                return
            }
        }
    }

    /// Count the number of half-moves (plies) in a space-separated UCI string.
    private func movesCount(_ moves: String) -> Int {
        moves.isEmpty ? 0 : moves.split(separator: " ").count
    }
}
