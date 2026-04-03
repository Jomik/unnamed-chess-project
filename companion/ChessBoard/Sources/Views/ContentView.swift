import SwiftUI

struct ContentView: View {
    @Environment(BoardConnection.self) private var board

    var body: some View {
        NavigationStack {
            Group {
                if board.connectionState != .ready {
                    ScanView()
                } else if board.gameState.status == .idle {
                    NewGameView()
                } else {
                    ActiveGameView()
                }
            }
        }
    }
}
