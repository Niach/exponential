// swift-tools-version: 5.9
// #VAPP-268 (E6c): the iOS runner's glue, `RunnerKit`, on the Engine from the
// workspace (`apps/desktop/crates/eengine-ffi`, its Package.swift; build its
// xcframework with `build-ios.sh` first). The SwiftUI app around it (App/)
// is not a SwiftPM target: `make-project.sh` emits the Xcode project that
// links RunnerKit, ExponentialUI and ONLY the declared capability modules.
//   swift test --package-path apps/runner/ios     (macOS: the engine's macOS slice)
import PackageDescription

let package = Package(
    name: "ExponentialRunner",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "RunnerKit", targets: ["RunnerKit"]),
    ],
    dependencies: [
        .package(name: "ExponentialEngine", path: "../../desktop/crates/eengine-ffi"),
    ],
    targets: [
        .target(name: "RunnerKit", dependencies: [.product(name: "ExponentialEngine", package: "ExponentialEngine")]),
        .testTarget(name: "RunnerKitTests", dependencies: ["RunnerKit"]),
    ]
)
