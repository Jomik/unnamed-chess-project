import SwiftUI

struct ActiveGameView: View {
    @Environment(BoardConnection.self) private var board
    @State private var showResignConfirmation = false

    var body: some View {
        VStack(spacing: 32) {
            Spacer()

            Image(systemName: statusIcon)
                .font(.system(size: 48))
                .foregroundStyle(statusColor)

            Text(statusText)
                .font(.largeTitle.bold())

            if board.gameStatus == .inProgress {
                Text(turnText)
                    .font(.title2)
                    .foregroundStyle(.secondary)
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

    private var statusText: String {
        switch board.gameStatus {
        case .idle: return "No Game"
        case .awaitingPieces: return "Set Up Starting Position"
        case .inProgress: return "In Progress"
        case .checkmate(let loser):
            return "Checkmate – \(loser == .white ? "White" : "Black") loses"
        case .stalemate: return "Stalemate"
        case .resigned(let color):
            return "\(color == .white ? "White" : "Black") Resigned"
        }
    }

    private var statusIcon: String {
        switch board.gameStatus {
        case .idle: return "square.dashed"
        case .awaitingPieces: return "checkerboard.rectangle"
        case .inProgress: return "play.fill"
        case .checkmate: return "crown.fill"
        case .stalemate: return "equal.circle.fill"
        case .resigned: return "flag.fill"
        }
    }

    private var statusColor: Color {
        switch board.gameStatus {
        case .idle: return .secondary
        case .awaitingPieces: return .blue
        case .inProgress: return .green
        case .checkmate: return .orange
        default: return .secondary
        }
    }
}

#if DEBUG
    #Preview("Awaiting Pieces") {
        NavigationStack { ActiveGameView() }
            .environment(
                BoardConnection(
                    gameStatus: .awaitingPieces
                )
            )
    }
    #Preview("In Progress") {
        NavigationStack { ActiveGameView() }
            .environment(
                BoardConnection(
                    gameStatus: .inProgress,
                    currentPosition:
                        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
                )
            )
    }
    #Preview("Checkmate") {
        NavigationStack { ActiveGameView() }
            .environment(
                BoardConnection(
                    gameStatus: .checkmate(loser: .black)
                )
            )
    }
    #Preview("Resigned") {
        NavigationStack { ActiveGameView() }
            .environment(
                BoardConnection(
                    gameStatus: .resigned(color: .white)
                )
            )
    }
    #Preview("Stalemate") {
        NavigationStack { ActiveGameView() }
            .environment(
                BoardConnection(
                    gameStatus: .stalemate
                )
            )
    }
#endif
