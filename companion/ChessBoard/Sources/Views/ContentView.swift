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
            .mockBoard()
    }
    #Preview("Scanning") {
        ContentView()
            .mockBoard(connectionState: .scanning)
    }
    #Preview("In Progress") {
        ContentView()
            .mockBoard(gameStatus: .inProgress)
    }
#endif
