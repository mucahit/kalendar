// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "macos-calendar-bridge",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "kalendar-eventkit", targets: ["KalendarBridge"]),
    ],
    targets: [
        .executableTarget(
            name: "KalendarBridge",
            exclude: ["Info.plist"],
            linkerSettings: [
                .unsafeFlags([
                    "-Xlinker", "-sectcreate",
                    "-Xlinker", "__TEXT",
                    "-Xlinker", "__info_plist",
                    "-Xlinker", "Sources/KalendarBridge/Info.plist",
                ])
            ]
        ),
    ],
    swiftLanguageModes: [.v5]
)
