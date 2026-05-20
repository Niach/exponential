import ProjectDescription

let project = Project(
    name: "Exponential",
    options: .options(
        defaultKnownRegions: ["en"],
        developmentRegion: "en"
    ),
    targets: [
        .target(
            name: "Exponential",
            destinations: [.iPhone, .iPad],
            product: .app,
            bundleId: "com.straehhuber.exponential",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: [
                "CFBundleDisplayName": "Exponential",
                "CFBundleShortVersionString": "0.2.0",
                "CFBundleVersion": "2",
                "UILaunchScreen": .dictionary([:]),
                "CFBundleURLTypes": .array([
                    .dictionary([
                        "CFBundleURLSchemes": .array([.string("exp")]),
                        "CFBundleURLName": .string("com.straehhuber.exponential.oauth"),
                    ]),
                ]),
                "UIBackgroundModes": .array([.string("remote-notification")]),
                "ITSAppUsesNonExemptEncryption": .boolean(false),
            ]),
            sources: ["Exponential/**"],
            resources: [
                "Exponential/Assets.xcassets",
                "Exponential/GoogleService-Info.plist",
            ],
            dependencies: [
                .external(name: "GRDB"),
                .external(name: "MarkdownUI"),
                .external(name: "FirebaseCore"),
                .external(name: "FirebaseMessaging"),
            ],
            settings: .settings(
                base: [
                    "SWIFT_VERSION": "6.0",
                    "SWIFT_STRICT_CONCURRENCY": "complete",
                    "DEVELOPMENT_TEAM": "V6W7BVCSM8",
                    "CODE_SIGN_STYLE": "Automatic",
                ]
            )
        ),
    ]
)
