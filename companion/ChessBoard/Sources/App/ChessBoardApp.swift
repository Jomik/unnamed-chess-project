import SwiftUI

@main
struct ChessBoardApp: App {
    @State private var board = BoardConnection()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(board)
        }
    }
}
