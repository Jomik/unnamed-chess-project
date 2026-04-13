import Foundation
import Testing

@testable import ChessBoard

@Suite struct GameStatusTests {
    @Test func decodeIdle() {
        #expect(GameStatus.decode(Data([0x00])) == .idle)
    }

    @Test func decodeAwaitingPieces() {
        #expect(GameStatus.decode(Data([0x01])) == .awaitingPieces)
    }

    @Test func decodeInProgress() {
        #expect(GameStatus.decode(Data([0x02])) == .inProgress)
    }

    @Test func decodeCheckmateWhiteLoser() {
        #expect(
            GameStatus.decode(Data([0x03, 0x00])) == .checkmate(loser: .white)
        )
    }

    @Test func decodeCheckmateBlackLoser() {
        #expect(
            GameStatus.decode(Data([0x03, 0x01])) == .checkmate(loser: .black)
        )
    }

    @Test func decodeCheckmateMissingLoser() {
        #expect(GameStatus.decode(Data([0x03])) == nil)
    }

    @Test func decodeCheckmateInvalidLoser() {
        #expect(GameStatus.decode(Data([0x03, 0xFF])) == nil)
    }

    @Test func decodeStalemate() {
        #expect(GameStatus.decode(Data([0x04])) == .stalemate)
    }

    @Test func decodeResignedWhite() {
        #expect(
            GameStatus.decode(Data([0x05, 0x00])) == .resigned(color: .white)
        )
    }

    @Test func decodeResignedBlack() {
        #expect(
            GameStatus.decode(Data([0x05, 0x01])) == .resigned(color: .black)
        )
    }

    @Test func decodeResignedMissingColor() {
        #expect(GameStatus.decode(Data([0x05])) == nil)
    }

    @Test func decodeResignedInvalidColor() {
        #expect(GameStatus.decode(Data([0x05, 0xFF])) == nil)
    }

    @Test func decodeInvalidTag() {
        #expect(GameStatus.decode(Data([0xFF])) == nil)
    }

    @Test func decodeEmpty() {
        #expect(GameStatus.decode(Data()) == nil)
    }

    @Test func isTerminalFalseForNonTerminal() {
        #expect(!GameStatus.idle.isTerminal)
        #expect(!GameStatus.awaitingPieces.isTerminal)
        #expect(!GameStatus.inProgress.isTerminal)
    }

    @Test func isTerminalTrueForTerminal() {
        #expect(GameStatus.checkmate(loser: .white).isTerminal)
        #expect(GameStatus.checkmate(loser: .black).isTerminal)
        #expect(GameStatus.stalemate.isTerminal)
        #expect(GameStatus.resigned(color: .white).isTerminal)
        #expect(GameStatus.resigned(color: .black).isTerminal)
    }
}
