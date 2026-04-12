import SwiftUI

extension GameStatus {
    var displayText: String {
        switch self {
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

    var iconName: String {
        switch self {
        case .idle: return "square.dashed"
        case .awaitingPieces: return "checkerboard.rectangle"
        case .inProgress: return "play.fill"
        case .checkmate: return "crown.fill"
        case .stalemate: return "equal.circle.fill"
        case .resigned: return "flag.fill"
        }
    }

    var displayColor: Color {
        switch self {
        case .idle: return .secondary
        case .awaitingPieces: return .blue
        case .inProgress: return .green
        case .checkmate: return .orange
        default: return .secondary
        }
    }
}
