import SwiftUI

struct ContentView: View {
    @Environment(BoardConnection.self) private var board

    var body: some View {
        NavigationStack {
            Group {
                if board.connectionState != .ready {
                    ScanView()
                } else if board.gameStatus == .idle {
                    NewGameView()
                } else {
                    ActiveGameView()
                }
            }
        }
    }
}

#if DEBUG
    #Preview("Ready - New Game", traits: .modifier(MockBoard())) {
        ContentView()
    }
    #Preview(
        "Scanning",
        traits: .modifier(MockBoard(connectionState: .scanning))
    ) {
        ContentView()
    }
    #Preview(
        "In Progress",
        traits: .modifier(MockBoard(gameStatus: .inProgress))
    ) {
        ContentView()
    }
#endif
