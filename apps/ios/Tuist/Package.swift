// swift-tools-version: 6.0
import Foundation
import PackageDescription

#if TUIST
import struct ProjectDescription.PackageSettings

let packageSettings = PackageSettings(
    // GRDB is linked by both ExpCore (the shared framework) and the app targets,
    // which use GRDB APIs directly. A static product would be linked twice (two
    // module copies → incompatible types across the boundary), so vend it as a
    // dynamic framework: one shared copy linked by both.
    productTypes: [
        "GRDB": .framework,
        // SLOP-18 / VAPP-88: the Exponential UI SDK's pure-SwiftUI primitives
        // (pill, segmented control, field chrome, drawn switch, avatar, meter,
        // ring). ExpUI builds its glass views on them and ExpUITests reads
        // their style structs, so vend ONE dynamic copy rather than a static
        // library linked into both.
        "ExponentialUIPrimitives": .framework,
    ]
)
#endif

// #VAPP-272: the Engine (apps/desktop/crates/eengine-ffi) holds the apps'
// outbox. Declared only when its gitignored xcframework exists (`bash
// apps/desktop/crates/eengine-ffi/build-ios.sh`), like the eUI painter.
let engineDirectory = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appendingPathComponent("../../desktop/crates/eengine-ffi")
    .standardizedFileURL.path
let engine: [Package.Dependency] = FileManager.default.fileExists(atPath: "\(engineDirectory)/out/ExponentialEngineFFI.xcframework")
    ? [.package(path: "../../desktop/crates/eengine-ffi")]
    : []

let package = Package(
    name: "Exponential",
    dependencies: [
        // swift-markdown-ui was removed on purpose (2026-07-11): its one
        // usage was the read-only comment display, and its optimized opaque-
        // Body metadata hard-crashed the iOS 27 runtime. Comments render via
        // the in-house block editor (read-only mode) instead.
        .package(url: "https://github.com/groue/GRDB.swift.git", from: "7.12.0"),
        // cmark-gfm / cmark-gfm-extensions used to arrive transitively via
        // swift-markdown-ui; the block editor's GFM parser links them directly,
        // so declare the source package explicitly now that markdown-ui is gone.
        .package(url: "https://github.com/swiftlang/swift-cmark", from: "0.9.0"),
        // Capped below 13: Firebase 13 needs a newer Xcode than CI runs.
        .package(url: "https://github.com/firebase/firebase-ios-sdk.git", "12.19.0"..<"13.0.0"),
        // The Exponential UI SDK (packages/exponential-ui-swift), a LOCAL path
        // package: its `ExponentialUIPrimitives` product needs no Rust
        // toolchain; the painter `ExponentialUI` (EXP-1280: the plan card's
        // inline surface) is declared only when its xcframework exists, and
        // Project.swift links it only then.
        .package(path: "../../../packages/exponential-ui-swift"),
    ] + engine
)
