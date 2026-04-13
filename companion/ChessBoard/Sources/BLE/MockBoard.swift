#if DEBUG
    import SwiftUI

    extension View {
        /// Creates a MockTransport-backed BoardConnection and injects it into
        /// the environment. This is a convenience method for SwiftUI Previews.
        func mockBoard(
            connectionState: ConnectionState = .ready,
            gameStatus: GameStatus = .idle,
            currentPosition: String? = nil,
            lastCommandResult: CommandResult? = nil,
            whitePlayerType: PlayerType? = .human,
            blackPlayerType: PlayerType? = .remote
        ) -> some View {
            let board = BoardConnection(transport: MockTransport())
            board.connectionState = connectionState
            board.gameStatus = gameStatus
            board.currentPosition = currentPosition
            board.lastCommandResult = lastCommandResult
            board.whitePlayerType = whitePlayerType
            board.blackPlayerType = blackPlayerType
            return self.environment(board)
        }
    }
#endif
