import CoreBluetooth

/// GATT UUID constants matching the firmware's `ble_protocol::uuids` module.
///
/// All UUIDs share the base `3d6343a2-xxxx-44ea-8fc2-3568d7216866`.
enum GATT {
    static let gameService = CBUUID(
        string: "3d6343a2-1010-44ea-8fc2-3568d7216866"
    )
    static let whitePlayer = CBUUID(
        string: "3d6343a2-1011-44ea-8fc2-3568d7216866"
    )
    static let blackPlayer = CBUUID(
        string: "3d6343a2-1012-44ea-8fc2-3568d7216866"
    )
    static let startGame = CBUUID(
        string: "3d6343a2-1013-44ea-8fc2-3568d7216866"
    )
    static let matchControl = CBUUID(
        string: "3d6343a2-1014-44ea-8fc2-3568d7216866"
    )
    static let gameStatus = CBUUID(
        string: "3d6343a2-1015-44ea-8fc2-3568d7216866"
    )
    static let commandResult = CBUUID(
        string: "3d6343a2-1016-44ea-8fc2-3568d7216866"
    )
    static let submitMove = CBUUID(
        string: "3d6343a2-1017-44ea-8fc2-3568d7216866"
    )
    static let position = CBUUID(string: "3d6343a2-1018-44ea-8fc2-3568d7216866")
    static let lastMove = CBUUID(string: "3d6343a2-1019-44ea-8fc2-3568d7216866")
    static let movePlayed = CBUUID(
        string: "3d6343a2-101a-44ea-8fc2-3568d7216866"
    )

    static let allServices = [gameService]

    static let gameCharacteristics: [CBUUID] = [
        whitePlayer, blackPlayer, startGame, matchControl,
        gameStatus, commandResult, submitMove, position, lastMove, movePlayed,
    ]
}
