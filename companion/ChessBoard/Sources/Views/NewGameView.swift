import SwiftUI

struct NewGameView: View {
    @Environment(BoardConnection.self) private var board
    @Environment(\.dismiss) private var dismiss
    @State private var whiteType: PlayerType = .human
    @State private var blackType: PlayerType = .embedded
    @State private var error: String?

    var body: some View {
        Form {
            Section("White") {
                Picker("Player", selection: $whiteType) {
                    Text("Human").tag(PlayerType.human)
                    Text("Engine").tag(PlayerType.embedded)
                }
                .pickerStyle(.segmented)
            }

            Section("Black") {
                Picker("Player", selection: $blackType) {
                    Text("Human").tag(PlayerType.human)
                    Text("Engine").tag(PlayerType.embedded)
                }
                .pickerStyle(.segmented)
            }

            if let error {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.red)
                }
            }

            Section {
                Button("Start Game") {
                    error = nil
                    board.configureAndStart(white: whiteType, black: blackType)
                }
            }
        }
        .navigationTitle("New Game")
        .onChange(of: board.lastCommandResult) {
            guard let result = board.lastCommandResult,
                result.source == .startGame
            else { return }
            if !result.ok {
                error = result.message
            } else {
                dismiss()
            }
        }
    }
}
