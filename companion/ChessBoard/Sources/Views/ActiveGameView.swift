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

            if board.gameState.status == .inProgress {
                Text(turnText)
                    .font(.title2)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            if board.gameState.status == .inProgress,
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

            if board.gameState.isTerminal {
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
        switch board.gameState.status {
        case .idle: return "No Game"
        case .awaitingPieces: return "Set Up Starting Position"
        case .inProgress: return "In Progress"
        case .checkmate: return "Checkmate"
        case .stalemate: return "Stalemate"
        case .resignation: return "Resigned"
        case .draw: return "Draw"
        }
    }

    private var turnText: String {
        board.gameState.turn == .white ? "White to move" : "Black to move"
    }

    private var statusIcon: String {
        switch board.gameState.status {
        case .idle: return "square.dashed"
        case .awaitingPieces: return "checkerboard.rectangle"
        case .inProgress: return "play.fill"
        case .checkmate: return "crown.fill"
        case .stalemate: return "equal.circle.fill"
        case .resignation: return "flag.fill"
        case .draw: return "handshake.fill"
        }
    }

    private var statusColor: Color {
        switch board.gameState.status {
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
                    gameState: GameState(status: .awaitingPieces, turn: .white)
                )
            )
    }
    #Preview("In Progress") {
        NavigationStack { ActiveGameView() }
            .environment(
                BoardConnection(
                    gameState: GameState(status: .inProgress, turn: .white)
                )
            )
    }
    #Preview("Checkmate") {
        NavigationStack { ActiveGameView() }
            .environment(
                BoardConnection(
                    gameState: GameState(status: .checkmate, turn: .black)
                )
            )
    }
    #Preview("Resignation") {
        NavigationStack { ActiveGameView() }
            .environment(
                BoardConnection(
                    gameState: GameState(status: .resignation, turn: .white)
                )
            )
    }
    #Preview("Stalemate") {
        NavigationStack { ActiveGameView() }
            .environment(
                BoardConnection(
                    gameState: GameState(status: .stalemate, turn: .white)
                )
            )
    }
#endif
