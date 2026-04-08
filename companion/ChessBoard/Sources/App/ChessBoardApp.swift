import SwiftUI

@main
struct ChessBoardApp: App {
    @State private var board: BoardConnection = {
        #if DEBUG && targetEnvironment(simulator)
            BoardConnection(connectionState: .ready)
        #else
            BoardConnection()
        #endif
    }()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(board)
        }
    }
}
