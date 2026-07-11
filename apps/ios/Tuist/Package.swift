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
    ],
    // MarkdownUI at -Onone even in Release: the optimizer emits a symbolic-
    // reference mangled name for Markdown's opaque `Body` witness that the
    // iOS 27 (beta) Swift runtime fails to demangle — opening any issue with
    // comments aborted with "failed to demangle witness for associated type
    // 'Body' in conformance 'MarkdownUI.Markdown: View'". Debug (-Onone)
    // metadata resolves fine, so pin this one package to -Onone.
    targetSettings: [
        "MarkdownUI": ["SWIFT_OPTIMIZATION_LEVEL": "-Onone"],
    ]
)
#endif

let package = Package(
    name: "Exponential",
    dependencies: [
        .package(url: "https://github.com/groue/GRDB.swift.git", from: "7.0.0"),
        .package(url: "https://github.com/gonzalezreal/swift-markdown-ui.git", from: "2.4.0"),
        .package(url: "https://github.com/firebase/firebase-ios-sdk.git", from: "11.0.0"),
    ]
)
