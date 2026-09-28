import ProjectDescription

// VAPP-3 spike: PeerHello, a throwaway iOS app driving the Rust peer core (PeerFFI.xcframework,
// built by ../build-xcframework.sh which also copies the Swift bindings into Sources/Generated).
let project = Project(
    name: "PeerHello",
    targets: [
        .target(
            name: "PeerHello",
            destinations: [.iPhone, .iPad],
            product: .app,
            bundleId: "at.exponential.peerhello",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: [
                "UILaunchScreen": [:],
                "CFBundleDisplayName": "PeerHello",
                "NSLocalNetworkUsageDescription": "PeerHello talks to the VAPP-3 spike relay, TURN server and daemon on your LAN.",
                "NSAppTransportSecurity": ["NSAllowsArbitraryLoads": true, "NSAllowsLocalNetworking": true],
            ]),
            sources: ["Sources/**"],
            dependencies: [
                .xcframework(path: "../out/PeerFFI.xcframework"),
                .sdk(name: "CryptoKit", type: .framework),
                .sdk(name: "SystemConfiguration", type: .framework),
                .sdk(name: "resolv", type: .library),
            ],
            settings: .settings(base: [
                "SWIFT_VERSION": "5.0",
                "DEVELOPMENT_TEAM": "V6W7BVCSM8",
                "CODE_SIGN_STYLE": "Automatic",
                "ARCHS": "arm64",
                "ONLY_ACTIVE_ARCH": "YES",
            ])
        ),
    ]
)
