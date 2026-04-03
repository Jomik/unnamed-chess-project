import SwiftUI

struct ScanView: View {
    @Environment(BoardConnection.self) private var board

    var body: some View {
        VStack(spacing: 24) {
            Spacer()

            ProgressView()
                .controlSize(.large)

            Text(statusText)
                .font(.headline)
                .foregroundStyle(.secondary)

            Spacer()
        }
        .navigationTitle("ChessBoard")
    }

    private var statusText: String {
        switch board.connectionState {
        case .poweredOff:
            return "Turn on Bluetooth"
        case .scanning:
            return "Searching for board…"
        case .connecting:
            return "Connecting…"
        case .discoveringServices:
            return "Setting up…"
        case .ready:
            return "Connected"
        }
    }
}
