// swift-tools-version: 5.10
// A fresh SwiftUI app on the Exponential UI SDK. Published, the dependency is
//   .package(url: "https://github.com/Niach/exponential-ui-swift", from: "0.1.0")
// with `package: "exponential-ui-swift"` below; until then it is the
// monorepo's package by path (its Binaries/ExponentialUIFFI.xcframework comes
// from `bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh`).
import PackageDescription

let package = Package(
    name: "GuideApp",
    platforms: [.iOS(.v17), .macOS(.v14)],
    dependencies: [
        .package(path: "../../../../packages/exponential-ui-swift"),
    ],
    targets: [
        .executableTarget(
            name: "GuideApp",
            dependencies: [.product(name: "ExponentialUI", package: "exponential-ui-swift")]
        ),
    ]
)
