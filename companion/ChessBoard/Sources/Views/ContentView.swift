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
    #Preview("Ready - New Game") {
        ContentView()
            .environment(BoardConnection(connectionState: .ready))
    }
    #Preview("Scanning") {
        ContentView()
            .environment(BoardConnection(connectionState: .scanning))
    }
    #Preview("In Progress") {
        ContentView()
            .environment(
                BoardConnection(
                    gameStatus: .inProgress
                )
            )
    }
#endif
