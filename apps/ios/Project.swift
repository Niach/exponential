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

// ExpCore: platform-neutral data/sync/domain layer shared with macOS later.
// Foundation/GRDB/Security/CryptoKit/os only — NO cmark/MarkdownUI/Firebase/SwiftUI.
let expCoreSources: SourceFilesList = ["ExpCore/Sources/**"]
let expCoreDependencies: [TargetDependency] = [.external(name: "GRDB")]
// ExpCore unit tests — currently the annotation-geometry parity gate that locks
// AnnotationGeometry.swift to the TS source of truth (shapes.test.ts) and the
// Linux Zig port.
let expCoreTestSources: SourceFilesList = ["ExpCore/Tests/**"]

// ExpUI: cross-platform SwiftUI layer (theme, glass modifiers, status/priority
// colors, WorkspaceAvatar, CrossPlatform shims) shared by the iOS and macOS apps.
// SwiftUI only; depends on ExpCore for the domain enums/entities it renders.
let expUiSources: SourceFilesList = ["ExpUI/Sources/**"]
let expUiDependencies: [TargetDependency] = [
    .target(name: "ExpCore"),
    // The shared block-based markdown editor core (IssueEditorModel +
    // MarkdownConversion) lives in ExpUI and parses GFM via cmark.
    .external(name: "cmark-gfm"),
    .external(name: "cmark-gfm-extensions"),
]

// macOS app (A2): native SwiftUI shell reusing ExpCore (data/sync) + ExpUI
// (theme). No Firebase/push/share-extension. GRDB is a direct dep for the
// ValueObservation queries in the read-only view models.
let macSources: SourceFilesList = ["ExponentialMac/**"]
let macDependencies: [TargetDependency] = [
    .target(name: "ExpCore"),
    .target(name: "ExpUI"),
    .external(name: "GRDB"),
    // Prebuilt libghostty (static) for the embedded terminal (M7). Fetched by
    // scripts/setup-ghostty-macos.sh into vendor/ (gitignored). Imported as the
    // `GhosttyKit` clang module; needs the system frameworks + libc++ below.
    .xcframework(path: "vendor/GhosttyKit.xcframework"),
]

// Folder-reference resources for libghostty: themes/shell-integration under
// Contents/Resources/ghostty (= GHOSTTY_RESOURCES_DIR) + the compiled terminfo
// DB at the sibling Contents/Resources/terminfo (libghostty derives TERMINFO as
// dirname(GHOSTTY_RESOURCES_DIR)/terminfo). Fetched by setup-ghostty-macos.sh.
let macResources: ResourceFileElements = [
    .folderReference(path: "vendor/ghostty-resources/ghostty"),
    .folderReference(path: "vendor/ghostty-resources/terminfo"),
]
let macInfoPlist: [String: Plist.Value] = [
    "CFBundleShortVersionString": "0.1.0",
    "CFBundleVersion": "1",
    "CFBundleURLTypes": .array([
        .dictionary([
            "CFBundleURLSchemes": .array([.string("exp")]),
            "CFBundleURLName": .string("at.exponential.oauth"),
        ]),
    ]),
    "LSApplicationCategoryType": .string("public.app-category.productivity"),
]

// Bootstrap (pre-build, self-healing): fetch the prebuilt GhosttyKit.xcframework
// + ghostty resources into vendor/ if missing. The first `tuist generate` still
// needs them present (Tuist validates the xcframework path at generate time —
// run scripts/setup-ghostty-macos.sh once before generating); this script just
// re-fetches if vendor/ was later cleaned, so builds don't fail mysteriously.
let ghosttyBootstrapScript = TargetScript.pre(
    script: """
    if [ ! -d "$SRCROOT/vendor/GhosttyKit.xcframework" ] || [ ! -d "$SRCROOT/vendor/ghostty-resources" ]; then
        "$SRCROOT/scripts/setup-ghostty-macos.sh"
    fi
    """,
    name: "Bootstrap libghostty (vendor/)",
    basedOnDependencyAnalysis: false
)

// Link settings for the macOS app: the static libghostty xcframework needs the
// system frameworks + libc++ below. Hardened Runtime is on for
// distribution/notarization.
let macLinkSettings: SettingsDictionary = [
    // Sign the macOS app with a STABLE Apple Development identity instead of
    // ad-hoc "Sign to Run Locally". macOS Debug defaults to ad-hoc (its signature
    // changes every build → the login keychain re-prompts every launch); forcing
    // CODE_SIGN_IDENTITY to the Development cert gives a stable designated
    // requirement so a one-time keychain "Always Allow" sticks across rebuilds.
    // Automatic signing, team from baseSettings (V6W7BVCSM8), Apple ID in Xcode —
    // build with -allowProvisioningUpdates.
    "CODE_SIGN_IDENTITY": "Apple Development",
    "LD_RUNPATH_SEARCH_PATHS": ["$(inherited)", "@executable_path/../Frameworks"],
    "ENABLE_HARDENED_RUNTIME": "YES",
    "OTHER_LDFLAGS": [
        "$(inherited)", "-lc++",
        "-framework", "Metal", "-framework", "MetalKit", "-framework", "QuartzCore",
        "-framework", "CoreText", "-framework", "CoreGraphics", "-framework", "IOKit",
        "-framework", "Carbon", "-framework", "UserNotifications",
    ],
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
            "CFBundleURLName": .string("at.exponential.oauth"),
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
            bundleId: "at.exponential.core",
            deploymentTargets: .multiplatform(iOS: "17.4", macOS: "14.0"),
            sources: expCoreSources,
            dependencies: expCoreDependencies,
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "ExpCoreTests",
            destinations: [.iPhone, .iPad, .mac],
            product: .unitTests,
            bundleId: "at.exponential.core.tests",
            deploymentTargets: .multiplatform(iOS: "17.4", macOS: "14.0"),
            sources: expCoreTestSources,
            dependencies: [.target(name: "ExpCore")],
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "ExpUI",
            destinations: [.iPhone, .iPad, .mac],
            product: .framework,
            bundleId: "at.exponential.ui",
            deploymentTargets: .multiplatform(iOS: "17.4", macOS: "14.0"),
            sources: expUiSources,
            dependencies: expUiDependencies,
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
        .target(
            name: "Exponential-macOS",
            destinations: [.mac],
            product: .app,
            bundleId: "at.exponential.mac",
            deploymentTargets: .macOS("14.0"),
            infoPlist: .extendingDefault(with: macInfoPlist.merging([
                "CFBundleDisplayName": "Exponential",
            ]) { _, new in new }),
            sources: macSources,
            resources: macResources,
            entitlements: "ExponentialMac.entitlements",
            scripts: [ghosttyBootstrapScript],
            dependencies: macDependencies,
            settings: .settings(base: baseSettings.merging(macLinkSettings) { _, new in new })
        ),
        .target(
            name: "Exponential-macOS-Staging",
            destinations: [.mac],
            product: .app,
            bundleId: "at.exponential.mac.staging",
            deploymentTargets: .macOS("14.0"),
            infoPlist: .extendingDefault(with: macInfoPlist.merging([
                "CFBundleDisplayName": "Exp Mac Staging",
            ]) { _, new in new }),
            sources: macSources,
            resources: macResources,
            entitlements: "ExponentialMac.entitlements",
            scripts: [ghosttyBootstrapScript],
            dependencies: macDependencies,
            settings: .settings(base: baseSettings
                .merging(macLinkSettings) { _, new in new }
                .merging(["SWIFT_ACTIVE_COMPILATION_CONDITIONS": "$(inherited) STAGING"]) { _, new in new })
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
            buildAction: .buildAction(targets: ["ExpUI"])
        ),
        .scheme(
            name: "Exponential-macOS",
            buildAction: .buildAction(targets: ["Exponential-macOS"]),
            runAction: .runAction(configuration: .debug),
            archiveAction: .archiveAction(configuration: .release)
        ),
        .scheme(
            name: "Exponential-macOS-Staging",
            buildAction: .buildAction(targets: ["Exponential-macOS-Staging"]),
            runAction: .runAction(configuration: .debug),
            archiveAction: .archiveAction(configuration: .release)
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
