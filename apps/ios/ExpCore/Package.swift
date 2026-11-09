// swift-tools-version:6.0
import PackageDescription

// #EXP-1543: ExpCore as a plain SwiftPM package, ONLY so CI (ios-core.yml:
// informational on PRs, a blocker in release-suite.yml) replays its suite on the macOS host with
// `swift test` (no tuist, no simulator). The app builds ExpCore from
// Project.swift; keep the two in step (Swift 6, MemberImportVisibility, GRDB
// pinned EXACTLY to Tuist/Package.resolved: the gate checks it).

let package = Package(
    name: "ExpCore",
    platforms: [.macOS("14.4"), .iOS("17.4")],
    products: [.library(name: "ExpCore", targets: ["ExpCore"])],
    dependencies: [.package(url: "https://github.com/groue/GRDB.swift.git", exact: "7.12.0")],
    targets: [
        .target(
            name: "ExpCore",
            dependencies: [.product(name: "GRDB", package: "GRDB.swift")],
            path: "Sources",
            swiftSettings: [.enableUpcomingFeature("MemberImportVisibility")]
        ),
        .testTarget(name: "ExpCoreTests", dependencies: ["ExpCore"], path: "Tests"),
    ]
)
