import SwiftUI

struct NewGameView: View {
    @Environment(BoardConnection.self) private var board
    @Environment(\.dismiss) private var dismiss

    @State private var whiteType: PlayerType = .human
    @State private var blackType: PlayerType = .embedded
    @State private var lichessLevel: Int = 4
    @State private var error: String?

    @State private var wifiSsid = ""
    @State private var wifiPassword = ""
    @State private var wifiAuthMode: WifiAuthMode = .wpa2

    @State private var lichessToken = ""
    @State private var hasLoaded = false
    @State private var isStarting = false
    @State private var lichessTimeoutTask: Task<Void, Never>?
    @State private var showResetConfirmation = false

    private var needsLichess: Bool {
        whiteType == .lichessAi || blackType == .lichessAi
    }

    private var canStart: Bool {
        !needsLichess
            || (board.wifiStatus.state == .connected
                && board.lichessStatus.state == .connected)
    }

    private var startBlockedReason: String? {
        guard needsLichess else { return nil }
        let wifiReady = board.wifiStatus.state == .connected
        let lichessReady = board.lichessStatus.state == .connected
        switch (wifiReady, lichessReady) {
        case (false, _):
            return "Connect to WiFi first"
        case (true, false):
            return "Validate Lichess token first"
        case (true, true):
            return nil
        }
    }

    var body: some View {
        Form {
            playerSection("White", type: $whiteType)
            playerSection("Black", type: $blackType)

            if needsLichess {
                lichessLevelSection
                wifiSection
                lichessSection
            }

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
                .disabled(isStarting || !canStart)
                if let reason = startBlockedReason {
                    Text(reason)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }

            #if DEBUG
                Section {
                    Button("Reset Saved Data", role: .destructive) {
                        showResetConfirmation = true
                    }
                }
            #endif
        }
        .navigationTitle("New Game")
        .animation(.default, value: needsLichess)
        .onAppear {
            guard !hasLoaded else { return }
            hasLoaded = true
            loadPreferences()
        }
        .onChange(of: needsLichess) {
            autoConnectIfNeeded()
        }
        .onChange(of: board.lastCommandResult) {
            guard let result = board.lastCommandResult,
                result.source == .startGame
            else { return }
            isStarting = false
            if !result.ok {
                error = result.message
            } else {
                dismiss()
            }
        }
        .onChange(of: board.connectionState) {
            if board.connectionState != .ready {
                isStarting = false
            }
        }
        .onChange(of: board.wifiStatus.state) {
            if board.wifiStatus.state == .connected {
                let creds = WifiCredentials(
                    ssid: wifiSsid,
                    password: wifiPassword,
                    authMode: wifiAuthMode
                )
                if let data = try? JSONEncoder().encode(creds) {
                    KeychainStore.save(
                        key: "wifi_credentials",
                        data: data
                    )
                }
            }
        }
        .onChange(of: board.lichessStatus.state) {
            // Timeout management
            lichessTimeoutTask?.cancel()
            lichessTimeoutTask = nil
            if board.lichessStatus.state == .validating {
                lichessTimeoutTask = Task {
                    try? await Task.sleep(for: .seconds(15))
                    guard !Task.isCancelled else { return }
                    board.lichessStatus = LichessStatus(
                        state: .failed,
                        message: "Validation timed out"
                    )
                }
            }

            // Save token on success
            if board.lichessStatus.state == .connected {
                if let data = lichessToken.data(using: .utf8) {
                    KeychainStore.save(key: "lichess_token", data: data)
                }
            }
        }
        .onDisappear {
            lichessTimeoutTask?.cancel()
        }
        .alert(
            "Reset Saved Data?",
            isPresented: $showResetConfirmation
        ) {
            Button("Reset", role: .destructive) {
                resetSavedData()
            }
        } message: {
            Text(
                "This will clear all saved WiFi credentials, Lichess token, and player preferences."
            )
        }
    }

    private func playerSection(
        _ title: String,
        type: Binding<PlayerType>
    ) -> some View {
        Section(title) {
            Picker("Player", selection: type) {
                Text("Human").tag(PlayerType.human)
                Text("Engine").tag(PlayerType.embedded)
                Text("Lichess AI").tag(PlayerType.lichessAi)
            }
            .pickerStyle(.segmented)
        }
    }

    private var lichessLevelSection: some View {
        Section("Lichess AI Level") {
            Picker("Level", selection: $lichessLevel) {
                ForEach(1...8, id: \.self) { level in
                    Text("\(level)").tag(level)
                }
            }
            .pickerStyle(.segmented)
        }
    }

    @ViewBuilder
    private var wifiSection: some View {
        Section("WiFi") {
            switch board.wifiStatus.state {
            case .connected:
                Label(
                    "Connected to \(wifiSsid)",
                    systemImage: "wifi"
                )
                .foregroundStyle(.green)
            case .connecting:
                HStack {
                    ProgressView()
                    Text("Connecting…")
                        .foregroundStyle(.secondary)
                }
            case .failed:
                Label(
                    board.wifiStatus.message.isEmpty
                        ? "Connection failed"
                        : board.wifiStatus.message,
                    systemImage: "wifi.exclamationmark"
                )
                .foregroundStyle(.red)
                Button("Retry") {
                    board.configureWifi(
                        ssid: wifiSsid,
                        password: wifiPassword,
                        authMode: wifiAuthMode
                    )
                }
                .disabled(wifiSsid.isEmpty)
                wifiFields
            case .disconnected:
                wifiFields
            }
        }
    }

    @ViewBuilder
    private var wifiFields: some View {
        TextField("SSID", text: $wifiSsid)
            .textContentType(.none)
            .autocorrectionDisabled()
            .textInputAutocapitalization(.never)
        if wifiAuthMode != .open {
            SecureField("Password", text: $wifiPassword)
                .textContentType(.none)
        }
        Picker("Security", selection: $wifiAuthMode) {
            Text("Open").tag(WifiAuthMode.open)
            Text("WPA2").tag(WifiAuthMode.wpa2)
            Text("WPA3").tag(WifiAuthMode.wpa3)
        }
        Button("Connect") {
            board.configureWifi(
                ssid: wifiSsid,
                password: wifiPassword,
                authMode: wifiAuthMode
            )
        }
        .disabled(wifiSsid.isEmpty)
    }

    @ViewBuilder
    private var lichessSection: some View {
        Section("Lichess") {
            switch board.lichessStatus.state {
            case .connected:
                Label(
                    "Lichess connected",
                    systemImage: "checkmark.circle"
                )
                .foregroundStyle(.green)
            case .validating:
                HStack {
                    ProgressView()
                    Text("Validating token…")
                        .foregroundStyle(.secondary)
                }
            case .failed:
                Label(
                    board.lichessStatus.message
                        .isEmpty
                        ? "Validation failed"
                        : board.lichessStatus
                            .message,
                    systemImage: "exclamationmark.triangle"
                )
                .foregroundStyle(.red)
                Button("Retry") {
                    board.setLichessToken(lichessToken)
                }
                .disabled(lichessToken.isEmpty)
                lichessFields
            case .idle:
                lichessFields
            }
        }
    }

    @ViewBuilder
    private var lichessFields: some View {
        SecureField("API Token", text: $lichessToken)
        Button("Validate") {
            board.setLichessToken(lichessToken)
        }
        .disabled(lichessToken.isEmpty)
    }

    private func startGame() {
        error = nil
        savePreferences()
        guard board.connectionState == .ready else {
            error = "Board disconnected"
            return
        }
        isStarting = true
        let wl =
            whiteType == .lichessAi ? lichessLevel : 0
        let bl =
            blackType == .lichessAi ? lichessLevel : 0
        board.configureAndStart(
            white: whiteType,
            whiteLevel: wl,
            black: blackType,
            blackLevel: bl
        )
    }

    private func autoConnectIfNeeded() {
        guard needsLichess else { return }

        // Reconnect automatically so the user doesn't have to re-tap "Connect"
        if board.wifiStatus.state == .disconnected
            && !wifiSsid.isEmpty
        {
            board.configureWifi(
                ssid: wifiSsid,
                password: wifiPassword,
                authMode: wifiAuthMode
            )
        }

        if board.lichessStatus.state == .idle
            && !lichessToken.isEmpty
        {
            board.setLichessToken(lichessToken)
        }
    }

    private func loadPreferences() {
        if let data = KeychainStore.load(
            key: "wifi_credentials"
        ),
            let creds = try? JSONDecoder().decode(
                WifiCredentials.self,
                from: data
            )
        {
            wifiSsid = creds.ssid
            wifiPassword = creds.password
            wifiAuthMode = creds.authMode
        }

        if let data = KeychainStore.load(
            key: "lichess_token"
        ),
            let token = String(data: data, encoding: .utf8)
        {
            lichessToken = token
        }

        let defaults = UserDefaults.standard
        if let raw = defaults.object(
            forKey: "chess_white_player"
        ) as? Int,
            let u8 = UInt8(exactly: raw),
            let type = PlayerType(rawValue: u8)
        {
            whiteType = type
        }
        if let raw = defaults.object(
            forKey: "chess_black_player"
        ) as? Int,
            let u8 = UInt8(exactly: raw),
            let type = PlayerType(rawValue: u8)
        {
            blackType = type
        }
        let level = defaults.integer(
            forKey: "chess_lichess_level"
        )
        if level >= 1 && level <= 8 {
            lichessLevel = level
        }
    }

    private func savePreferences() {
        let defaults = UserDefaults.standard
        defaults.set(
            Int(whiteType.rawValue),
            forKey: "chess_white_player"
        )
        defaults.set(
            Int(blackType.rawValue),
            forKey: "chess_black_player"
        )
        defaults.set(
            lichessLevel,
            forKey: "chess_lichess_level"
        )
    }

    #if DEBUG
        private func resetSavedData() {
            KeychainStore.delete(key: "wifi_credentials")
            KeychainStore.delete(key: "lichess_token")
            let defaults = UserDefaults.standard
            defaults.removeObject(forKey: "chess_white_player")
            defaults.removeObject(forKey: "chess_black_player")
            defaults.removeObject(forKey: "chess_lichess_level")
            // Reset local state
            wifiSsid = ""
            wifiPassword = ""
            wifiAuthMode = .wpa2
            lichessToken = ""
            whiteType = .human
            blackType = .embedded
            lichessLevel = 4
            error = nil
        }
    #endif
}

#if DEBUG
    #Preview("Idle") {
        NavigationStack { NewGameView() }
            .environment(BoardConnection(connectionState: .ready))
    }
    #Preview("WiFi Connecting") {
        NavigationStack {
            NewGameView()
        }
        .environment(
            BoardConnection(
                wifiStatus: WifiStatus(state: .connecting, message: "")
            )
        )
    }
    #Preview("WiFi Connected") {
        NavigationStack {
            NewGameView()
        }
        .environment(
            BoardConnection(
                wifiStatus: WifiStatus(state: .connected, message: "")
            )
        )
    }
    #Preview("WiFi Failed") {
        NavigationStack {
            NewGameView()
        }
        .environment(
            BoardConnection(
                wifiStatus: WifiStatus(
                    state: .failed,
                    message: "Connection timed out"
                )
            )
        )
    }
    #Preview("Lichess Validating") {
        NavigationStack {
            NewGameView()
        }
        .environment(
            BoardConnection(
                wifiStatus: WifiStatus(state: .connected, message: ""),
                lichessStatus: LichessStatus(state: .validating, message: "")
            )
        )
    }
    #Preview("Lichess Connected") {
        NavigationStack {
            NewGameView()
        }
        .environment(
            BoardConnection(
                wifiStatus: WifiStatus(state: .connected, message: ""),
                lichessStatus: LichessStatus(state: .connected, message: "")
            )
        )
    }
#endif
