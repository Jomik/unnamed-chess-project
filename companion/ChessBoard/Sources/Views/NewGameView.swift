import Security
import SwiftUI

struct NewGameView: View {
    @Environment(BoardConnection.self) private var board
    @Environment(\.dismiss) private var dismiss

    @State private var whiteType: PlayerType = .human
    @State private var blackType: PlayerType = .remote
    @State private var error: String?

    @State private var hasLoaded = false
    @State private var isStarting = false

    // Lichess configuration
    @State private var lichessToken: String = ""
    @State private var lichessLevel: Int = 3

    var body: some View {
        Form {
            playerSection("White", type: $whiteType)
            playerSection("Black", type: $blackType)

            if isLichessGame {
                lichessConfigSection
            }

            if let error {
                Section {
                    Label(
                        error,
                        systemImage: "exclamationmark.triangle"
                    )
                    .foregroundStyle(.red)
                }
            }

            Section {
                Button {
                    Task { await startGame() }
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

    // MARK: - Derived helpers

    /// True when at least one side is Remote (Lichess AI).
    private var isLichessGame: Bool {
        whiteType == .remote || blackType == .remote
    }

    /// The color that is NOT remote (nil when both are remote or neither).
    private var humanColor: Turn? {
        switch (whiteType, blackType) {
        case (.human, .remote): return .white
        case (.remote, .human): return .black
        default: return nil
        }
    }

    // MARK: - Sub-views

    private func playerSection(
        _ title: String,
        type: Binding<PlayerType>
    ) -> some View {
        Section(title) {
            Picker("Player", selection: type) {
                Text("Human").tag(PlayerType.human)
                Text("Lichess AI").tag(PlayerType.remote)
            }
            .pickerStyle(.segmented)
        }
    }

    @ViewBuilder
    private var lichessConfigSection: some View {
        Section("Lichess AI") {
            SecureField("API Token", text: $lichessToken)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()

            Picker("AI Level", selection: $lichessLevel) {
                ForEach(1...8, id: \.self) { level in
                    Text("Level \(level)").tag(level)
                }
            }
        }
    }

    // MARK: - Actions

    private func startGame() async {
        error = nil
        board.lichessError = nil  // Clear any previous Lichess errors
        savePreferences()
        guard board.connectionState == .ready else {
            error = "Board disconnected"
            return
        }

        // Set up Lichess bridge when one side is remote
        if isLichessGame, let color = humanColor {
            let trimmedToken = lichessToken.trimmingCharacters(in: .whitespaces)
            guard !trimmedToken.isEmpty else {
                error = "Lichess API token is required"
                return
            }

            // Validate token before starting the board game
            isStarting = true
            let api = LichessAPI(token: trimmedToken)
            do {
                try await api.validateToken()
            } catch {
                isStarting = false
                self.error = error.localizedDescription
                return
            }

            let service = LichessService(
                token: trimmedToken,
                board: board,
                humanColor: color
            )
            board.lichessService = service
            board.pendingLichessLevel = lichessLevel
            board.onMovePlayed = { [weak service] turn, uci in
                service?.boardMovePlayed(color: turn, uci: uci)
            }
        } else if isLichessGame {
            // Both sides are remote — not a supported configuration
            error = "At least one side must be human for a Lichess game"
            return
        } else {
            board.lichessService = nil
            board.pendingLichessLevel = nil
            board.onMovePlayed = nil
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
        // Load Lichess token from Keychain
        if let token = KeychainStore.retrieve(forKey: "lichess_token") {
            lichessToken = token
        }
        // Load Lichess level from UserDefaults
        if let savedLevel = defaults.object(forKey: "lichess_level") as? Int {
            lichessLevel = savedLevel
        }
    }

    private func savePreferences() {
        let defaults = UserDefaults.standard
        defaults.set(Int(whiteType.rawValue), forKey: "chess_white_player")
        defaults.set(Int(blackType.rawValue), forKey: "chess_black_player")
        // Save Lichess level to UserDefaults
        defaults.set(lichessLevel, forKey: "lichess_level")
        // Save Lichess token to Keychain
        let trimmedToken = lichessToken.trimmingCharacters(in: .whitespaces)
        if !trimmedToken.isEmpty {
            do {
                try KeychainStore.save(trimmedToken, forKey: "lichess_token")
            } catch {
                // Silently fail on Keychain save errors (non-critical)
                print(
                    "Warning: Failed to save Lichess token to Keychain: \(error)"
                )
            }
        } else {
            do {
                try KeychainStore.delete(forKey: "lichess_token")
            } catch {
                // Silently fail on Keychain delete errors (non-critical)
                print(
                    "Warning: Failed to delete Lichess token from Keychain: \(error)"
                )
            }
        }
    }

    #if DEBUG
        private func resetSavedData() {
            let defaults = UserDefaults.standard
            defaults.removeObject(forKey: "chess_white_player")
            defaults.removeObject(forKey: "chess_black_player")
            defaults.removeObject(forKey: "lichess_level")
            do {
                try KeychainStore.delete(forKey: "lichess_token")
            } catch {
                print(
                    "Warning: Failed to delete Lichess token from Keychain: \(error)"
                )
            }
            whiteType = .human
            blackType = .remote
            lichessToken = ""
            lichessLevel = 3
            error = nil
        }
    #endif
}

#if DEBUG
    #Preview("Idle", traits: .modifier(MockBoard())) {
        NavigationStack { NewGameView() }
    }
    #Preview("Lichess Remote", traits: .modifier(MockBoard())) {
        NavigationStack { NewGameView() }
    }
#endif
