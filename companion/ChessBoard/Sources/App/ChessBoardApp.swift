import SwiftUI

@main
struct ChessBoardApp: App {
    @State private var board = BoardConnection(transport: BLETransport())

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(board)
        }
    }
}
