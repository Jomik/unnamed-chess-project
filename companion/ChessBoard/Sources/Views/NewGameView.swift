import SwiftUI

struct NewGameView: View {
    @Environment(BoardConnection.self) private var board
    @Environment(\.dismiss) private var dismiss

    @State private var whiteType: PlayerType = .human
    @State private var blackType: PlayerType = .remote
    @State private var error: String?

    @State private var hasLoaded = false
    @State private var isStarting = false

    var body: some View {
        Form {
            playerSection("White", type: $whiteType)
            playerSection("Black", type: $blackType)

            if let error {
                Section {
                    Label(
                        error,
                        systemImage: "exclamationmark.triangle"
                    )
                    .foregroundStyle(.red)
                    Button("Retry") {
                        startGame()
                    }
                }
            }

            Section {
                Button {
                    startGame()
                } label: {
                    HStack {
                        Text("Start Game")
                        if isStarting {
                            Spacer()
                            ProgressView()
                        }
                    }
                }
                .disabled(isStarting)
            }

            #if DEBUG
                Section {
                    Button("Reset Saved Data", role: .destructive) {
                        resetSavedData()
                    }
                }
            #endif
        }
        .navigationTitle("New Game")
        .onAppear {
            guard !hasLoaded else { return }
            hasLoaded = true
            loadPreferences()
        }
        .onChange(of: board.lastCommandResult) {
            guard let result = board.lastCommandResult,
                result.source == .startGame
            else { return }
            isStarting = false
            if !result.ok {
                error = result.error.map { "\($0)" } ?? "Unknown error"
            } else {
                dismiss()
            }
        }
        .onChange(of: board.connectionState) {
            if board.connectionState != .ready {
                isStarting = false
            }
        }
    }

    private func playerSection(
        _ title: String,
        type: Binding<PlayerType>
    ) -> some View {
        Section(title) {
            Picker("Player", selection: type) {
                Text("Human").tag(PlayerType.human)
                Text("Remote").tag(PlayerType.remote)
            }
            .pickerStyle(.segmented)
        }
    }

    private func startGame() {
        error = nil
        savePreferences()
        guard board.connectionState == .ready else {
            error = "Board disconnected"
            return
        }
        isStarting = true
        board.configureAndStart(white: whiteType, black: blackType)
    }

    private func loadPreferences() {
        let defaults = UserDefaults.standard
        if let raw = defaults.object(forKey: "chess_white_player") as? Int,
            let u8 = UInt8(exactly: raw),
            let type = PlayerType(rawValue: u8)
        {
            whiteType = type
        }
        if let raw = defaults.object(forKey: "chess_black_player") as? Int,
            let u8 = UInt8(exactly: raw),
            let type = PlayerType(rawValue: u8)
        {
            blackType = type
        }
    }

    private func savePreferences() {
        let defaults = UserDefaults.standard
        defaults.set(Int(whiteType.rawValue), forKey: "chess_white_player")
        defaults.set(Int(blackType.rawValue), forKey: "chess_black_player")
    }

    #if DEBUG
        private func resetSavedData() {
            let defaults = UserDefaults.standard
            defaults.removeObject(forKey: "chess_white_player")
            defaults.removeObject(forKey: "chess_black_player")
            whiteType = .human
            blackType = .remote
            error = nil
        }
    #endif
}

#if DEBUG
    #Preview("Idle") {
        NavigationStack { NewGameView() }
            .environment(BoardConnection(connectionState: .ready))
    }
#endif
