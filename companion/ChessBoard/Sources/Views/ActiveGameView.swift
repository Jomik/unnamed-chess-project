import SwiftUI

struct ActiveGameView: View {
    @Environment(BoardConnection.self) private var board

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
                    board.resign(color: color)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
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
