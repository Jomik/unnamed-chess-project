import SwiftUI

struct ActiveGameView: View {
    @Environment(BoardConnection.self) private var board
    @State private var showResignConfirmation = false

    var body: some View {
        VStack(spacing: 32) {
            Spacer()

            Image(systemName: board.gameStatus.iconName)
                .font(.system(size: 48))
                .foregroundStyle(board.gameStatus.displayColor)

            Text(board.gameStatus.displayText)
                .font(.largeTitle.bold())

            if board.gameStatus == .inProgress {
                Text(turnText)
                    .font(.title2)
                    .foregroundStyle(.secondary)
            }

            if let lichessError = board.lichessError
                ?? board.lichessService?.error
            {
                Label(lichessError, systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.red)
                    .font(.callout)
                    .multilineTextAlignment(.center)
            }

            Spacer()

            if board.gameStatus == .awaitingPieces {
                Button("Cancel") { board.cancelGame() }
            }

            if board.gameStatus == .inProgress,
                let color = board.resignColor
            {
                Button("Resign", role: .destructive) {
                    showResignConfirmation = true
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .alert(
                    "Resign Game?",
                    isPresented: $showResignConfirmation
                ) {
                    Button("Resign", role: .destructive) {
                        board.resign(color: color)
                    }
                } message: {
                    Text("This will end the game as a loss.")
                }
            }

            if board.gameStatus.isTerminal {
                NavigationLink("New Game") {
                    NewGameView()
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
            }
        }
        .padding()
        .navigationTitle("Game")
    }

    private var turnText: String {
        guard let fen = board.currentPosition else { return "" }
        let components = fen.split(separator: " ")
        guard components.count >= 2 else { return "" }
        return components[1] == "w" ? "White to move" : "Black to move"
    }

}

#if DEBUG
    #Preview(
        "Awaiting Pieces",
        traits: .modifier(MockBoard(gameStatus: .awaitingPieces))
    ) {
        NavigationStack { ActiveGameView() }
    }
    #Preview(
        "In Progress",
        traits: .modifier(
            MockBoard(
                gameStatus: .inProgress,
                currentPosition:
                    "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
            )
        )
    ) {
        NavigationStack { ActiveGameView() }
    }
    #Preview(
        "Checkmate",
        traits: .modifier(MockBoard(gameStatus: .checkmate(loser: .black)))
    ) {
        NavigationStack { ActiveGameView() }
    }
    #Preview(
        "Resigned",
        traits: .modifier(MockBoard(gameStatus: .resigned(color: .white)))
    ) {
        NavigationStack { ActiveGameView() }
    }
    #Preview("Stalemate", traits: .modifier(MockBoard(gameStatus: .stalemate)))
    {
        NavigationStack { ActiveGameView() }
    }
#endif
