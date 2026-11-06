// swift-tools-version: 5.9
// #VAPP-268: the Engine for Swift FROM THE WORKSPACE, the layout of the
// published `ExponentialEngine` package (release/swift-package.sh writes the
// same targets): the binary target = out/ExponentialEngineFFI.xcframework
// (`bash build-ios.sh`; `EENGINE_FFI_XCFRAMEWORK` names another, absolute or
// relative to this directory), `ExponentialEngine` = the committed UniFFI
// binding, and the capabilities as ONE MODULE EACH, so an iOS app links a
// privacy framework only when it declares the capability (ITMS-90683):
//
// | product | links |
// |---|---|
// | `ExponentialEnginePhotos` | Photos, PhotosUI |
// | `ExponentialEngineFiles` | UniformTypeIdentifiers (the document picker) |
// | `ExponentialEngineShare` | LinkPresentation (the share sheet) |
// | `ExponentialEnginePrint` | UIKit printing |
// | `ExponentialEngineNotifications` | UserNotifications |
// | `ExponentialEngineCapabilities` | all five (`EngineCapabilities.ios()` / `.macOS()`) |
//
// Each sits on `ExponentialEngineCapabilityCore` (arguments, MCP results,
// threading, the presenter: Foundation + UIKit only). The iOS runner
// (apps/runner/ios) consumes this package by path; nothing is published from
// here (`eengine-v*` tags publish release/swift-package.sh's output).
import Foundation
import PackageDescription

/// SwiftPM takes a binary target's `path:` only relative to the package root.
func relativeToPackage(_ path: String) -> String {
    guard path.hasPrefix("/") else { return path }
    let base = URL(fileURLWithPath: Context.packageDirectory).standardizedFileURL.pathComponents
    let target = URL(fileURLWithPath: path).standardizedFileURL.pathComponents
    var common = 0
    while common < min(base.count, target.count), base[common] == target[common] { common += 1 }
    return (Array(repeating: "..", count: base.count - common) + target[common...]).joined(separator: "/")
}

let xcframework = relativeToPackage(ProcessInfo.processInfo.environment["EENGINE_FFI_XCFRAMEWORK"] ?? "out/ExponentialEngineFFI.xcframework")

let capabilities: [(product: String, path: String)] = [
    ("ExponentialEnginePhotos", "swift/ExponentialEnginePhotos"),
    ("ExponentialEngineFiles", "swift/ExponentialEngineFiles"),
    ("ExponentialEngineShare", "swift/ExponentialEngineShare"),
    ("ExponentialEnginePrint", "swift/ExponentialEnginePrint"),
    ("ExponentialEngineNotifications", "swift/ExponentialEngineNotifications"),
]

let package = Package(
    name: "ExponentialEngine",
    platforms: [.iOS(.v15), .macOS(.v12)],
    products: [
        .library(name: "ExponentialEngine", targets: ["ExponentialEngine"]),
        .library(name: "ExponentialEngineCapabilities", targets: ["ExponentialEngineCapabilities"]),
    ] + capabilities.map { .library(name: $0.product, targets: [$0.product]) },
    targets: [
        .binaryTarget(name: "ExponentialEngineFFI", path: xcframework),
        .target(name: "ExponentialEngine", dependencies: ["ExponentialEngineFFI"], path: "bindings/swift",
                exclude: ["ExponentialEngineFFIFFI.h", "module.modulemap"], sources: ["ExponentialEngineFFI.swift"]),
        .target(name: "ExponentialEngineCapabilityCore", dependencies: ["ExponentialEngine"], path: "swift/ExponentialEngineCapabilityCore"),
        .target(name: "ExponentialEngineCapabilities", dependencies: ["ExponentialEngine"] + capabilities.map { .target(name: $0.product) },
                path: "swift/ExponentialEngineCapabilities"),
    ] + capabilities.map {
        .target(name: $0.product, dependencies: ["ExponentialEngine", "ExponentialEngineCapabilityCore"], path: $0.path)
    }
)
