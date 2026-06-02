import ProjectDescription

let sharedSources: SourceFilesList = ["Exponential/**"]
let sharedResources: ResourceFileElements = [
    "Exponential/Assets.xcassets",
    "Exponential/GoogleService-Info.plist",
    "Exponential/Resources/**",
]
let sharedDependencies: [TargetDependency] = [
    .external(name: "GRDB"),
    .external(name: "MarkdownUI"),
    .external(name: "FirebaseCore"),
    .external(name: "FirebaseMessaging"),
    .external(name: "cmark-gfm"),
    .external(name: "cmark-gfm-extensions"),
]

// ExpCore: platform-neutral data/sync/domain layer shared with macOS later.
// Foundation/GRDB/Security/CryptoKit/os only — NO cmark/MarkdownUI/Firebase/SwiftUI.
let expCoreSources: SourceFilesList = ["ExpCore/Sources/**"]
let expCoreDependencies: [TargetDependency] = [.external(name: "GRDB")]

// ExpUI: cross-platform SwiftUI layer (theme, glass modifiers, status/priority
// colors, WorkspaceAvatar, CrossPlatform shims) shared by the iOS and macOS apps.
// SwiftUI only; depends on ExpCore for the domain enums/entities it renders.
let expUiSources: SourceFilesList = ["ExpUI/Sources/**"]
let expUiDependencies: [TargetDependency] = [.target(name: "ExpCore")]

// Foundation-only files reused by the Share Extension. Compiled into the
// extension's own module (no `public` needed, no shared framework). Verified to
// import only Foundation/Security/CryptoKit — no GRDB/Firebase/SwiftUI drag-in.
// These now live in ExpCore/Sources but the extension still compiles its own
// curated copy (it does NOT link ExpCore, so it stays GRDB-free).
let shareExtensionSources: SourceFilesList = [
    "ExpCore/Sources/AppConstants.swift",
    "ExpCore/Sources/Shared/**",
    "ExpCore/Sources/Auth/KeychainStore.swift",
    "ExpCore/Sources/Auth/AccountStore.swift",
    "ExpCore/Sources/Auth/ServerAccount.swift",
    "ExpCore/Sources/Auth/AuthRepository.swift",
    "ExpCore/Sources/API/HTTPClient.swift",
    "ExpCore/Sources/API/TrpcClient.swift",
    "ExpCore/Sources/API/IssuesApi.swift",
    "ExpCore/Sources/API/IssueImagesApi.swift",
    "ShareExtension/**",
]

let shareExtensionInfoPlist: [String: Plist.Value] = [
    "NSExtension": .dictionary([
        "NSExtensionPointIdentifier": .string("com.apple.share-services"),
        "NSExtensionPrincipalClass": .string("$(PRODUCT_MODULE_NAME).ShareViewController"),
        "NSExtensionAttributes": .dictionary([
            "NSExtensionActivationRule": .dictionary([
                "NSExtensionActivationSupportsImageWithMaxCount": .integer(20),
                "NSExtensionActivationSupportsText": .boolean(true),
                "NSExtensionActivationSupportsWebURLWithMaxCount": .integer(1),
            ]),
        ]),
    ]),
]

let sharedInfoPlist: [String: Plist.Value] = [
    "CFBundleShortVersionString": "0.3.0",
    "CFBundleVersion": "3",
    "UILaunchScreen": .dictionary([:]),
    "CFBundleURLTypes": .array([
        .dictionary([
            "CFBundleURLSchemes": .array([.string("exp")]),
            "CFBundleURLName": .string("com.straehhuber.exponential.oauth"),
        ]),
    ]),
    "UIBackgroundModes": .array([.string("remote-notification")]),
    "ITSAppUsesNonExemptEncryption": .boolean(false),
]
let baseSettings: SettingsDictionary = [
    "SWIFT_VERSION": "6.0",
    "SWIFT_STRICT_CONCURRENCY": "complete",
    "DEVELOPMENT_TEAM": "V6W7BVCSM8",
    "CODE_SIGN_STYLE": "Automatic",
]

let project = Project(
    name: "Exponential",
    options: .options(
        defaultKnownRegions: ["en"],
        developmentRegion: "en"
    ),
    targets: [
        .target(
            name: "ExpCore",
            destinations: [.iPhone, .iPad, .mac],
            product: .framework,
            bundleId: "com.straehhuber.exponential.core",
            deploymentTargets: .multiplatform(iOS: "17.4", macOS: "14.0"),
            sources: expCoreSources,
            dependencies: expCoreDependencies,
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "ExpUI",
            destinations: [.iPhone, .iPad, .mac],
            product: .framework,
            bundleId: "com.straehhuber.exponential.ui",
            deploymentTargets: .multiplatform(iOS: "17.4", macOS: "14.0"),
            sources: expUiSources,
            dependencies: expUiDependencies,
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "Exponential",
            destinations: [.iPhone, .iPad],
            product: .app,
            bundleId: "com.straehhuber.exponential",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: sharedInfoPlist.merging([
                "CFBundleDisplayName": "Exponential",
            ]) { _, new in new }),
            sources: sharedSources,
            resources: sharedResources,
            entitlements: "Exponential.entitlements",
            dependencies: sharedDependencies + [.target(name: "ExpCore"), .target(name: "ExpUI"), .target(name: "ShareExtension")],
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "Exponential-Staging",
            destinations: [.iPhone, .iPad],
            product: .app,
            bundleId: "com.straehhuber.exponential.staging",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: sharedInfoPlist.merging([
                "CFBundleDisplayName": "Exp Staging",
            ]) { _, new in new }),
            sources: sharedSources,
            resources: sharedResources,
            entitlements: "ExponentialStaging.entitlements",
            dependencies: sharedDependencies + [.target(name: "ExpCore"), .target(name: "ExpUI"), .target(name: "ShareExtension-Staging")],
            settings: .settings(base: baseSettings.merging([
                "SWIFT_ACTIVE_COMPILATION_CONDITIONS": "$(inherited) STAGING",
            ]) { _, new in new })
        ),
        .target(
            name: "ShareExtension",
            destinations: [.iPhone, .iPad],
            product: .appExtension,
            bundleId: "com.straehhuber.exponential.shareextension",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: shareExtensionInfoPlist.merging([
                "CFBundleDisplayName": "Exponential",
            ]) { _, new in new }),
            sources: shareExtensionSources,
            entitlements: "Exponential.entitlements",
            dependencies: [],
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "ShareExtension-Staging",
            destinations: [.iPhone, .iPad],
            product: .appExtension,
            bundleId: "com.straehhuber.exponential.staging.shareextension",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: shareExtensionInfoPlist.merging([
                "CFBundleDisplayName": "Exp Staging",
            ]) { _, new in new }),
            sources: shareExtensionSources,
            entitlements: "ExponentialStaging.entitlements",
            dependencies: [],
            settings: .settings(base: baseSettings.merging([
                "SWIFT_ACTIVE_COMPILATION_CONDITIONS": "$(inherited) STAGING",
            ]) { _, new in new })
        ),
    ],
    schemes: [
        .scheme(
            name: "ExpCore",
            buildAction: .buildAction(targets: ["ExpCore"])
        ),
        .scheme(
            name: "ExpUI",
            buildAction: .buildAction(targets: ["ExpUI"])
        ),
        .scheme(
            name: "Exponential",
            buildAction: .buildAction(targets: ["Exponential"]),
            runAction: .runAction(configuration: .debug),
            archiveAction: .archiveAction(configuration: .release)
        ),
        .scheme(
            name: "Exponential-Staging",
            buildAction: .buildAction(targets: ["Exponential-Staging"]),
            runAction: .runAction(configuration: .debug),
            archiveAction: .archiveAction(configuration: .release)
        ),
    ]
)
