import ProjectDescription

let sharedSources: SourceFilesList = ["Exponential/**"]
// GoogleService-Info.plist is per-target now (prod and staging are separate
// Firebase apps: at.exponential vs at.exponential.staging). Each iOS app target
// bundles exactly one, copied in as `GoogleService-Info.plist` (the name the
// Firebase SDK loads). The staging copy lives outside Exponential/ so the two
// files of the same bundle name don't collide.
let prodResources: ResourceFileElements = [
    "Exponential/Assets.xcassets",
    "Exponential/Resources/**",
    "Exponential/GoogleService-Info.plist",
]
let stagingResources: ResourceFileElements = [
    "Exponential/Assets.xcassets",
    "Exponential/Resources/**",
    "Firebase-Staging/GoogleService-Info.plist",
]
let sharedDependencies: [TargetDependency] = [
    .external(name: "GRDB"),
    .external(name: "MarkdownUI"),
    .external(name: "FirebaseCore"),
    .external(name: "FirebaseMessaging"),
    .external(name: "cmark-gfm"),
    .external(name: "cmark-gfm-extensions"),
]

// ExpCore: iOS(+iPad) data/sync/domain layer. Foundation/GRDB/Security/
// CryptoKit/os only — NO cmark/MarkdownUI/Firebase/SwiftUI.
let expCoreSources: SourceFilesList = ["ExpCore/Sources/**"]
let expCoreDependencies: [TargetDependency] = [.external(name: "GRDB")]
// ExpCore unit tests — currently the annotation-geometry parity gate that locks
// AnnotationGeometry.swift to the TS source of truth (shapes.test.ts) for
// web + iOS parity.
let expCoreTestSources: SourceFilesList = ["ExpCore/Tests/**"]

// ExpUI: the iOS SwiftUI presentation layer (theme, glass modifiers,
// status/priority colors, WorkspaceAvatar, the block markdown editor core).
// SwiftUI only; depends on ExpCore for the domain enums/entities it renders.
let expUiSources: SourceFilesList = ["ExpUI/Sources/**"]
// ExpUI unit tests — the editor interchange-contract gate: `@<email>` mentions
// and `#<IDENTIFIER>` issue refs round-trip through the block editor's
// markdown serialization as PLAIN GFM text (byte parity with the web,
// mirroring apps/web/src/components/issue-editor/mention-tokens.test.ts).
let expUiTestSources: SourceFilesList = ["ExpUI/Tests/**"]
let expUiDependencies: [TargetDependency] = [
    .target(name: "ExpCore"),
    // The shared block-based markdown editor core (IssueEditorModel +
    // MarkdownConversion) lives in ExpUI and parses GFM via cmark.
    .external(name: "cmark-gfm"),
    .external(name: "cmark-gfm-extensions"),
]

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

// Single source of truth for app + extension version; keep these in lockstep so
// the extension's CFBundleVersion never drifts from the parent app.
let appMarketingVersion = "0.11.0"
let appBuildVersion = "20"

let shareExtensionInfoPlist: [String: Plist.Value] = [
    // Must match the parent app's version (CFBundleVersion mismatch trips
    // embeddedBinaryValidationUtility during signing).
    "CFBundleShortVersionString": .string(appMarketingVersion),
    "CFBundleVersion": .string(appBuildVersion),
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
    "CFBundleShortVersionString": .string(appMarketingVersion),
    "CFBundleVersion": .string(appBuildVersion),
    "UILaunchScreen": .dictionary([:]),
    "CFBundleURLTypes": .array([
        .dictionary([
            "CFBundleURLSchemes": .array([.string("exponential")]),
            "CFBundleURLName": .string("at.exponential.oauth"),
        ]),
    ]),
    "UIBackgroundModes": .array([.string("remote-notification")]),
    "ITSAppUsesNonExemptEncryption": .boolean(false),
    // Self-hosted instances are a product feature and commonly live on plain
    // HTTP inside a LAN (and http://localhost:5173 in dev/screenshot runs) —
    // allow local networking only; everything else stays ATS-default (HTTPS).
    "NSAppTransportSecurity": .dictionary([
        "NSAllowsLocalNetworking": .boolean(true),
    ]),
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
            destinations: [.iPhone, .iPad],
            product: .framework,
            bundleId: "at.exponential.core",
            deploymentTargets: .iOS("17.4"),
            sources: expCoreSources,
            dependencies: expCoreDependencies,
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "ExpCoreTests",
            destinations: [.iPhone, .iPad],
            product: .unitTests,
            bundleId: "at.exponential.core.tests",
            deploymentTargets: .iOS("17.4"),
            sources: expCoreTestSources,
            dependencies: [.target(name: "ExpCore")],
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "ExpUI",
            destinations: [.iPhone, .iPad],
            product: .framework,
            bundleId: "at.exponential.ui",
            deploymentTargets: .iOS("17.4"),
            sources: expUiSources,
            dependencies: expUiDependencies,
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "ExpUITests",
            destinations: [.iPhone, .iPad],
            product: .unitTests,
            bundleId: "at.exponential.ui.tests",
            deploymentTargets: .iOS("17.4"),
            sources: expUiTestSources,
            dependencies: [.target(name: "ExpUI")],
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "Exponential",
            destinations: [.iPhone, .iPad],
            product: .app,
            bundleId: "at.exponential",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: sharedInfoPlist.merging([
                "CFBundleDisplayName": "Exponential",
            ]) { _, new in new }),
            sources: sharedSources,
            resources: prodResources,
            entitlements: "Exponential.entitlements",
            dependencies: sharedDependencies + [.target(name: "ExpCore"), .target(name: "ExpUI"), .target(name: "ShareExtension")],
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "Exponential-Staging",
            destinations: [.iPhone, .iPad],
            product: .app,
            bundleId: "at.exponential.staging",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: sharedInfoPlist.merging([
                "CFBundleDisplayName": "Exp Staging",
            ]) { _, new in new }),
            sources: sharedSources,
            resources: stagingResources,
            entitlements: "ExponentialStaging.entitlements",
            dependencies: sharedDependencies + [.target(name: "ExpCore"), .target(name: "ExpUI"), .target(name: "ShareExtension-Staging")],
            settings: .settings(base: baseSettings.merging([
                "SWIFT_ACTIVE_COMPILATION_CONDITIONS": "$(inherited) STAGING",
            ]) { _, new in new })
        ),
        // ExponentialUITests: fastlane snapshot (automated App Store screenshots).
        // Drives the production app target through the sign-in + capture flow in
        // ExponentialUITests/StoreScreenshots.swift; run via `bundle exec fastlane
        // screenshots` (apps/ios/fastlane/Snapfile).
        .target(
            name: "ExponentialUITests",
            destinations: [.iPhone, .iPad],
            product: .uiTests,
            bundleId: "at.exponential.uitests",
            deploymentTargets: .iOS("17.4"),
            sources: ["ExponentialUITests/**"],
            dependencies: [.target(name: "Exponential")],
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "ShareExtension",
            destinations: [.iPhone, .iPad],
            product: .appExtension,
            bundleId: "at.exponential.shareextension",
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
            bundleId: "at.exponential.staging.shareextension",
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
            buildAction: .buildAction(targets: ["ExpCore", "ExpCoreTests"]),
            testAction: .targets(["ExpCoreTests"])
        ),
        .scheme(
            name: "ExpUI",
            buildAction: .buildAction(targets: ["ExpUI", "ExpUITests"]),
            testAction: .targets(["ExpUITests"])
        ),
        .scheme(
            name: "Exponential",
            buildAction: .buildAction(targets: ["Exponential"]),
            // fastlane snapshot builds-for-testing + runs the UI test bundle
            // through this scheme's test action.
            testAction: .targets(["ExponentialUITests"]),
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
