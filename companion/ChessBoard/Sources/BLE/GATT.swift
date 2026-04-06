import CoreBluetooth

/// GATT UUID constants matching the firmware's `ble_protocol::uuids` module.
///
/// All UUIDs share the base `3d6343a2-xxxx-44ea-8fc2-3568d7216866`.
enum GATT {
    static let gameService = CBUUID(
        string: "3d6343a2-1001-44ea-8fc2-3568d7216866"
    )
    static let whitePlayer = CBUUID(
        string: "3d6343a2-1002-44ea-8fc2-3568d7216866"
    )
    static let blackPlayer = CBUUID(
        string: "3d6343a2-1003-44ea-8fc2-3568d7216866"
    )
    static let startGame = CBUUID(
        string: "3d6343a2-1004-44ea-8fc2-3568d7216866"
    )
    static let matchControl = CBUUID(
        string: "3d6343a2-1005-44ea-8fc2-3568d7216866"
    )
    static let gameState = CBUUID(
        string: "3d6343a2-1006-44ea-8fc2-3568d7216866"
    )
    static let commandResult = CBUUID(
        string: "3d6343a2-1007-44ea-8fc2-3568d7216866"
    )

    static let wifiService = CBUUID(
        string: "3d6343a2-2001-44ea-8fc2-3568d7216866"
    )
    static let lichessService = CBUUID(
        string: "3d6343a2-3001-44ea-8fc2-3568d7216866"
    )

    static let wifiConfig = CBUUID(
        string: "3d6343a2-2002-44ea-8fc2-3568d7216866"
    )
    static let wifiStatus = CBUUID(
        string: "3d6343a2-2003-44ea-8fc2-3568d7216866"
    )

    static let lichessToken = CBUUID(
        string: "3d6343a2-3002-44ea-8fc2-3568d7216866"
    )
    static let lichessStatus = CBUUID(
        string: "3d6343a2-3003-44ea-8fc2-3568d7216866"
    )

    static let allServices = [gameService, wifiService, lichessService]

    static let gameCharacteristics: [CBUUID] = [
        whitePlayer, blackPlayer, startGame, matchControl, gameState,
        commandResult,
    ]

    static let wifiCharacteristics: [CBUUID] = [wifiConfig, wifiStatus]
    static let lichessCharacteristics: [CBUUID] = [
        lichessToken, lichessStatus,
    ]
}
