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

            Spacer()

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
                    gameStatus: .inProgress
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
