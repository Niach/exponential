// swift-tools-version:6.0
import PackageDescription

// #EXP-1543: ExpCore as a plain SwiftPM package, ONLY so CI (ios-core.yml:
// informational on PRs, a blocker in release-suite.yml) replays its suite on the macOS host with
// `swift test` (no tuist, no simulator). The app builds ExpCore from
// Project.swift; keep the two in step (Swift 6, MemberImportVisibility, GRDB
// pinned EXACTLY to Tuist/Package.resolved: the gate checks it).

// #EXP-2067: the chat-express.json cases that compile (segment / stream)
// replay over the REAL eui SDK when its xcframework is built (`bash
// apps/desktop/crates/exponential-ui-ffi/build-ios.sh`, a macOS slice; never
// the mobile build's header-only link stub): the tests then link the SDK's
// Swift package; without it they skip and the pure cases still run.
import Foundation

let euiPackage = "../../../packages/exponential-ui-swift"
let euiFramework = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    .appendingPathComponent("\(euiPackage)/Binaries/ExponentialUIFFI.xcframework").standardizedFileURL.path
let hasEui = !FileManager.default.fileExists(atPath: "\(euiFramework)/LINK_STUB")
    && ((try? FileManager.default.contentsOfDirectory(atPath: euiFramework)) ?? []).contains { $0.hasPrefix("macos") }

let package = Package(
    name: "ExpCore",
    platforms: [.macOS("14.4"), .iOS("17.4")],
    products: [.library(name: "ExpCore", targets: ["ExpCore"])],
    dependencies: [.package(url: "https://github.com/groue/GRDB.swift.git", exact: "7.12.0")]
        + (hasEui ? [.package(path: euiPackage)] : []),
    targets: [
        .target(
            name: "ExpCore",
            dependencies: [.product(name: "GRDB", package: "GRDB.swift")],
            path: "Sources",
            swiftSettings: [.enableUpcomingFeature("MemberImportVisibility")]
        ),
        .testTarget(
            name: "ExpCoreTests",
            dependencies: ["ExpCore"] + (hasEui ? [.product(name: "ExponentialUI", package: "exponential-ui-swift")] : []),
            path: "Tests",
            swiftSettings: hasEui ? [.define("EXPCORE_EUI")] : []
        ),
    ]
)
