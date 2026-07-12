// swift-tools-version: 6.0
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
    ]
)
#endif

let package = Package(
    name: "Exponential",
    dependencies: [
        // swift-markdown-ui was removed on purpose (2026-07-11): its one
        // usage was the read-only comment display, and its optimized opaque-
        // Body metadata hard-crashed the iOS 27 runtime. Comments render via
        // the in-house block editor (read-only mode) instead.
        .package(url: "https://github.com/groue/GRDB.swift.git", from: "7.0.0"),
        // cmark-gfm / cmark-gfm-extensions used to arrive transitively via
        // swift-markdown-ui; the block editor's GFM parser links them directly,
        // so declare the source package explicitly now that markdown-ui is gone.
        .package(url: "https://github.com/swiftlang/swift-cmark", from: "0.7.1"),
        .package(url: "https://github.com/firebase/firebase-ios-sdk.git", from: "11.0.0"),
    ]
)
