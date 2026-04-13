#if DEBUG
    import SwiftUI

    /// A PreviewModifier that creates a MockTransport-backed BoardConnection
    /// and injects it into the environment.
    struct MockBoard: PreviewModifier {
        var connectionState: ConnectionState = .ready
        var gameStatus: GameStatus = .idle
        var currentPosition: String? = nil
        var lastCommandResult: CommandResult? = nil
        var whitePlayerType: PlayerType? = .human
        var blackPlayerType: PlayerType? = .remote

        func body(content: Content, context: Void) -> some View {
            let board = BoardConnection(transport: MockTransport())
            board.connectionState = connectionState
            board.gameStatus = gameStatus
            board.currentPosition = currentPosition
            board.lastCommandResult = lastCommandResult
            board.whitePlayerType = whitePlayerType
            board.blackPlayerType = blackPlayerType
            return content.environment(board)
        }
    }
#endif
