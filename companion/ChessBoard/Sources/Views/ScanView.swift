import SwiftUI

struct ScanView: View {
    @Environment(BoardConnection.self) private var board

    private let connectionTimeout: Duration = .seconds(20)

    var body: some View {
        VStack(spacing: 24) {
            Spacer()

            if board.connectionState.isFailure {
                Image(systemName: failureIcon)
                    .font(.system(size: 48))
                    .foregroundStyle(.secondary)

                Text(failureMessage)
                    .font(.headline)
                    .foregroundStyle(.secondary)

                Button("Retry") {
                    board.restartScanning()
                }
                .buttonStyle(.borderedProminent)
            } else {
                ProgressView()
                    .controlSize(.large)

                Text(statusText)
                    .font(.headline)
                    .foregroundStyle(.secondary)
            }

            Spacer()
        }
        .navigationTitle("ChessBoard")
        .task(id: board.connectionState) {
            guard board.connectionState.isTimeoutable else { return }
            do {
                try await Task.sleep(for: connectionTimeout)
                board.connectionTimedOut()
            } catch {
                // Task cancelled — state changed before timeout, nothing to do.
            }
        }
    }

    private var failureIcon: String {
        switch board.connectionState {
        case .notFound:
            return "exclamationmark.triangle"
        case .connectionFailed:
            return "bolt.horizontal.circle"
        case .setupFailed:
            return "gear.badge.xmark"
        default:
            return "exclamationmark.triangle"
        }
    }

    private var failureMessage: String {
        switch board.connectionState {
        case .notFound:
            return "Board not found"
        case .connectionFailed:
            return "Connection failed"
        case .setupFailed:
            return "Setup failed"
        default:
            return "Something went wrong"
        }
    }

    private var statusText: String {
        switch board.connectionState {
        case .poweredOff:
            return "Turn on Bluetooth"
        case .scanning:
            return "Searching for board…"
        case .notFound:
            return "Board not found"
        case .connecting:
            return "Connecting…"
        case .connectionFailed:
            return "Connection failed"
        case .discoveringServices:
            return "Setting up…"
        case .setupFailed:
            return "Setup failed"
        case .ready:
            return "Connected"
        }
    }
}

#if DEBUG
    #Preview("Scanning") {
        NavigationStack { ScanView() }
            .environment(BoardConnection(connectionState: .scanning))
    }
    #Preview("Not Found") {
        NavigationStack { ScanView() }
            .environment(BoardConnection(connectionState: .notFound))
    }
    #Preview("Connection Failed") {
        NavigationStack { ScanView() }
            .environment(BoardConnection(connectionState: .connectionFailed))
    }
    #Preview("Setup Failed") {
        NavigationStack { ScanView() }
            .environment(BoardConnection(connectionState: .setupFailed))
    }
    #Preview("Powered Off") {
        NavigationStack { ScanView() }
            .environment(BoardConnection(connectionState: .poweredOff))
    }
#endif
