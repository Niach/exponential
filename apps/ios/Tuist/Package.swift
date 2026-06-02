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
        .package(url: "https://github.com/groue/GRDB.swift.git", from: "7.0.0"),
        .package(url: "https://github.com/gonzalezreal/swift-markdown-ui.git", from: "2.4.0"),
        .package(url: "https://github.com/firebase/firebase-ios-sdk.git", from: "11.0.0"),
    ]
)
