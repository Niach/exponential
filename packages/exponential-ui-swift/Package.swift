// swift-tools-version: 5.10
// VAPP-88: `ExponentialUI`, the SwiftUI painter of the Exponential UI SDK.
//
// Products
// - `ExponentialUI`: the painter. Paints an A2UI surface at the frames the
//   Rust core (`exponential-ui`, VAPP-86) computes, reached through the
//   UniFFI facade `exponential-ui-ffi` shipped as `ExponentialUIFFI.xcframework`
//   (iOS device + simulator + macOS slices; `bash
//   apps/desktop/crates/exponential-ui-ffi/build-ios.sh` writes it into
//   `Binaries/`). iOS 17 and macOS 14.
// - `ExponentialUIPrimitives`: the generic SwiftUI primitives the catalog's
//   natives are painted with (pill, segmented control, field chrome, toggle
//   style, avatar, meter track, ring, markdown view, empty state,
//   disclosure). Pure SwiftUI, no binary: the Exponential app's `ExpUI` builds
//   its specialised views on them (SLOP-18 convergence) without linking the
//   core.
//
// The binary target is declared only when the xcframework is present (or
// `EXPONENTIAL_UI_FFI` names one), so a checkout that only needs the
// primitives (the app's Tuist graph, CI) resolves without a Rust toolchain.
// Publishing (VAPP-91) switches the binary target to `url:` + `checksum`.
import Foundation
import PackageDescription

let ffiPath = ProcessInfo.processInfo.environment["EXPONENTIAL_UI_FFI"] ?? "Binaries/ExponentialUIFFI.xcframework"
let ffiAbsolute = ffiPath.hasPrefix("/") ? ffiPath : Context.packageDirectory + "/" + ffiPath
let hasFFI = FileManager.default.fileExists(atPath: ffiAbsolute)

var products: [Product] = [
    .library(name: "ExponentialUIPrimitives", targets: ["ExponentialUIPrimitives"]),
]
var targets: [Target] = [
    .target(name: "ExponentialUIPrimitives"),
    .testTarget(name: "ExponentialUIPrimitivesTests", dependencies: ["ExponentialUIPrimitives"]),
]

if hasFFI {
    products.append(.library(name: "ExponentialUI", targets: ["ExponentialUI"]))
    targets += [
        .binaryTarget(name: "ExponentialUIFFI", path: ffiPath),
        // The generated UniFFI Swift binding (a committed copy of
        // apps/desktop/crates/exponential-ui-ffi/bindings/swift, drift-gated).
        .target(name: "ExponentialUICore", dependencies: ["ExponentialUIFFI"]),
        .target(name: "ExponentialUI", dependencies: ["ExponentialUICore", "ExponentialUIPrimitives"]),
        .testTarget(name: "ExponentialUITests", dependencies: ["ExponentialUI"]),
    ]
}

let package = Package(
    name: "ExponentialUI",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: products,
    targets: targets
)
