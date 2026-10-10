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
// `EXPONENTIAL_UI_FFI` names one, absolute or relative to this directory), so a checkout that only needs the
// primitives (the app's Tuist graph, CI) resolves without a Rust toolchain.
// Publishing (VAPP-91) switches the binary target to `url:` + `checksum`.
import Foundation
import PackageDescription

/// SwiftPM takes a binary target's `path:` only RELATIVE to the package root
/// (`../` is fine): an absolute `EXPONENTIAL_UI_FFI` is rewritten relative.
/// SwiftPM joins that path LEXICALLY onto `Context.packageDirectory` as it
/// was given (a symlinked checkout stays unresolved), so the rewrite is
/// lexical on both sides too (`..` collapsed, no symlink resolved): it
/// holds through symlinks of any depth, and the existence check joins the
/// same way.
func standardized(_ path: String) -> [String] {
    URL(fileURLWithPath: path).standardizedFileURL.pathComponents
}

func relativeToPackage(_ path: String) -> String {
    guard path.hasPrefix("/") else { return path }
    let base = standardized(Context.packageDirectory)
    let target = standardized(path)
    var common = 0
    while common < min(base.count, target.count), base[common] == target[common] { common += 1 }
    return (Array(repeating: "..", count: base.count - common) + target[common...]).joined(separator: "/")
}

let ffiPath = relativeToPackage(ProcessInfo.processInfo.environment["EXPONENTIAL_UI_FFI"] ?? "Binaries/ExponentialUIFFI.xcframework")
let hasFFI = FileManager.default.fileExists(atPath: URL(fileURLWithPath: Context.packageDirectory).appendingPathComponent(ffiPath).standardizedFileURL.path)

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
        // AVKit: Video plays in its `VideoPlayer` (linked explicitly; autolink
        // alone leaves the test bundle without AVPlayerView's metadata).
        .target(name: "ExponentialUI", dependencies: ["ExponentialUICore", "ExponentialUIPrimitives"], linkerSettings: [.linkedFramework("AVKit")]),
        .testTarget(name: "ExponentialUITests", dependencies: ["ExponentialUI"]),
    ]
}

let package = Package(
    name: "ExponentialUI",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: products,
    targets: targets
)
