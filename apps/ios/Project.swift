import Foundation
import ProjectDescription

// EXP-1635: the share pages also compile into the app for their capture-only
// specimens (`ChatCardsScreen`, `-uiTestingStyleguide share-to`): the
// extension cannot be driven by a test.
let sharedSources: SourceFilesList = [
    "Exponential/**",
    "ShareExtension/ShareModels.swift",
    "ShareExtension/ShareToPage.swift",
    "ShareExtension/ShareChatComposerPage.swift",
    // #EXP-1639: the Live Activity's attributes (the app starts nothing, it
    // observes the pushed ones) and its drawing (the `os-live-activity`
    // capture specimen).
    "LiveActivity/SessionActivityAttributes.swift",
    "LiveActivity/SessionActivityViews.swift",
]
// VAPP-110: the app host's app-extension catalog, bundled straight from
// packages/ui/exponential-ui (the bytes the web and desktop hosts install;
// `AppHostResources` loads it by name). Exponential ships no built-in apps.
let appHostResources: [ResourceFileElement] = [
    "../../packages/ui/exponential-ui/extension.json",
    // EXP-1654: the styleguide specimens' sample pictures (`SpecimenSamples`,
    // read only under `-uiTestingStyleguide`), bundled as the `samples` folder.
    .folderReference(path: "../../packages/exponential-ui/samples"),
    // #EXP-1639: the Live Activity's mark, for the `os-live-activity` specimen.
    "LiveActivity/Assets.xcassets",
]
// GoogleService-Info.plist is per-target now (prod and staging are separate
// Firebase apps: at.exponential vs at.exponential.staging). Each iOS app target
// bundles exactly one, copied in as `GoogleService-Info.plist` (the name the
// Firebase SDK loads). The staging copy lives outside Exponential/ so the two
// files of the same bundle name don't collide.
// PrivacyInfo.xcprivacy must land at the ROOT of the .app bundle, so it is copied
// as a plain flat resource (same treatment as GoogleService-Info.plist). Both app
// targets share ONE file: prod and staging ship identical code, so their
// required-reason APIs and collected data are identical too.
let prodResources: ResourceFileElements = .resources([
    "Exponential/Assets.xcassets",
    "Exponential/Resources/**",
    "Exponential/GoogleService-Info.plist",
    "Exponential/PrivacyInfo.xcprivacy",
] + appHostResources)
let stagingResources: ResourceFileElements = .resources([
    "Exponential/Assets.xcassets",
    "Exponential/Resources/**",
    "Firebase-Staging/GoogleService-Info.plist",
    "Exponential/PrivacyInfo.xcprivacy",
] + appHostResources)
// EXP-1280: the Exponential UI painter (`ExponentialUI`, the SDK's SwiftUI
// renderer over the Rust core) paints an agent's inline surface in the plan /
// question card. Its UniFFI core ships as `ExponentialUIFFI.xcframework`
// (gitignored; `bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh`
// writes it), and the SDK's Package.swift declares the product only when the
// xcframework exists, so the app links it only then. Without it the card
// draws the surface's placeholder frame (`canImport(ExponentialUI)`).
let exponentialUiFfiPath = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appendingPathComponent("../../packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework")
    .standardizedFileURL.path
let hasExponentialUi = FileManager.default.fileExists(atPath: exponentialUiFfiPath)
// #VAPP-272: the Engine holds the apps' record outbox (`SyncedRecordStore`). Like
// eUI, `ExponentialEngineFFI.xcframework` is gitignored (`bash
// apps/desktop/crates/eengine-ffi/build-ios.sh`); without it the app builds and
// an app's writes fail (`canImport(ExponentialEngine)`).
let hasEngine = FileManager.default.fileExists(atPath: URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appendingPathComponent("../desktop/crates/eengine-ffi/out/ExponentialEngineFFI.xcframework")
    .standardizedFileURL.path)
let surfaceDependencies: [TargetDependency] = (hasExponentialUi ? [.external(name: "ExponentialUI")] : [])
    + (hasEngine ? [.external(name: "ExponentialEngine")] : [])
// Tuist vends the SwiftPM `ExponentialUI` target as a static
// `ExponentialUIWrapper.framework` whose module is `ExponentialUI`: a
// framework search finds modules by FRAMEWORK name, so the app adds its
// Modules directory to the Swift include path.
let surfaceSettings: SettingsDictionary = hasExponentialUi
    ? ["SWIFT_INCLUDE_PATHS": "$(inherited) $(BUILT_PRODUCTS_DIR)/ExponentialUIWrapper.framework/Modules"]
    : [:]

// EXP-1435: sealed pushes. `ExpPush` (Foundation + Security) holds the
// install identity and the push key derived from it in the shared keychain
// group and opens the relay's sealed payloads through eNet's push-only UniFFI
// binding: the generated Swift is COMMITTED under
// apps/desktop/crates/exponential-net-ffi/bindings/push (module
// `ExponentialNetPush`), its static library ships as a gitignored xcframework
// (`ENET_PUSH_ONLY=1 bash apps/desktop/crates/exponential-net-ffi/build-ios.sh`).
// Like eUI, the binding compiles and links only when the xcframework exists
// (`ENET_PUSH`); without it the project still builds, the Notification Service
// Extension delivers the relay's placeholder unchanged and the app registers
// its bare FCM token (legacy). tuist caches the evaluated manifest by its
// content, so after building or deleting the xcframework locally a stale
// `hasEnetPush` survives until Project.swift changes (CI starts fresh).
let enetFfiDir = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appendingPathComponent("../desktop/crates/exponential-net-ffi")
    .standardizedFileURL.path
let hasEnetPush = FileManager.default.fileExists(atPath: "\(enetFfiDir)/out/push/ExponentialNetPush.xcframework")
let enetPushBinding: [TargetDependency] = hasEnetPush
    ? [.xcframework(path: "../desktop/crates/exponential-net-ffi/out/push/ExponentialNetPush.xcframework")]
    : []
let expPushSources: SourceFilesList = hasEnetPush
    ? ["ExpPush/Sources/**", "../desktop/crates/exponential-net-ffi/bindings/push/swift/ExponentialNetPush.swift"]
    : ["ExpPush/Sources/**"]
let enetPushSettings: SettingsDictionary = hasEnetPush
    ? ["SWIFT_ACTIVE_COMPILATION_CONDITIONS": "$(inherited) ENET_PUSH"]
    : [:]
// Every target that links ExpPush (a static framework) links the binding's
// static library too.
let expPushDependencies: [TargetDependency] = [.target(name: "ExpPush")] + enetPushBinding

// VAPP-40 / EXP-1457: the FULL eNet node (iroh) in ExpCore, so every install
// runs a node and registers it with the install registry. Same pattern: the
// generated binding (COMMITTED, bindings/swift, module `ExponentialNetFFI`)
// compiles into ExpCore and the gitignored `out/ExponentialNetFFI.xcframework`
// (`bash apps/desktop/crates/exponential-net-ffi/build-ios.sh`) links into it
// only when built (`ENET_NODE`); without it eNet stays off. Its C module and
// library are named apart from the push build's, so ExpCore (a dynamic
// framework: the node's library lives in it) and the extension never collide.
let hasEnetNode = FileManager.default.fileExists(atPath: "\(enetFfiDir)/out/ExponentialNetFFI.xcframework")
let enetNodeSources: [SourceFileGlob] = hasEnetNode
    ? ["../desktop/crates/exponential-net-ffi/bindings/swift/ExponentialNetFFI.swift"]
    : []
// iroh's netwatch reads the network paths (Network, SystemConfiguration).
let enetNodeBinding: [TargetDependency] = hasEnetNode
    ? [
        .xcframework(path: "../desktop/crates/exponential-net-ffi/out/ExponentialNetFFI.xcframework"),
        .sdk(name: "Network", type: .framework),
        .sdk(name: "SystemConfiguration", type: .framework),
    ]
    : []
let enetNodeSettings: SettingsDictionary = hasEnetNode
    ? ["SWIFT_ACTIVE_COMPILATION_CONDITIONS": "$(inherited) ENET_NODE"]
    : [:]

let sharedDependencies: [TargetDependency] = [
    .external(name: "GRDB"),
    .external(name: "FirebaseCore"),
    .external(name: "FirebaseMessaging"),
    .external(name: "cmark-gfm"),
    .external(name: "cmark-gfm-extensions"),
    // NEW-69: the call screen draws THE VoiceWave (`VoiceWaveView`).
    .external(name: "ExponentialUIPrimitives"),
    // #LOGS-2: crash/hang/performance reporting (`Observability.swift`), the
    // app targets only: ExpCore and the extensions stay free of it.
    .external(name: "Sentry"),
] + surfaceDependencies

// ExpCore: iOS(+iPad) data/sync/domain layer. Foundation/GRDB/Security/
// CryptoKit/os only — NO cmark/MarkdownUI/Firebase/SwiftUI.
let expCoreSources: SourceFilesList = .sourceFilesList(globs: ["ExpCore/Sources/**"] + enetNodeSources)
let expCoreDependencies: [TargetDependency] = [.external(name: "GRDB")] + enetNodeBinding
// ExpCore unit tests — currently the annotation-geometry parity gate that locks
// AnnotationGeometry.swift to the TS source of truth (shapes.test.ts) for
// web + iOS parity.
let expCoreTestSources: SourceFilesList = ["ExpCore/Tests/**"]

// ExpUI: the iOS SwiftUI presentation layer (theme, glass modifiers,
// status/priority colors, TeamAvatar, the block markdown editor core).
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
    // SLOP-18 / VAPP-88: the GENERIC drawing (pill, segmented control, field
    // chrome, switch, avatar, meter track, ring) lives in the Exponential UI
    // SDK; ExpUI's glass views are thin wrappers that feed it DesignTokens.
    .external(name: "ExponentialUIPrimitives"),
]

// Foundation-only files reused by the Share Extension. Compiled into the
// extension's own module (no `public` needed, no shared framework). Verified to
// import only Foundation/Security/CryptoKit — no GRDB/Firebase/SwiftUI drag-in.
// These now live in ExpCore/Sources but the extension still compiles its own
// curated copy (it does NOT link ExpCore, so it stays GRDB-free).
let shareExtensionSources: SourceFilesList = [
    "ExpCore/Sources/AppConstants.swift",
    // HTTPClient's 426 handling references the gate (EXP-104); the extension
    // compiles it too, where a trip is simply never read.
    "ExpCore/Sources/UpdateGate.swift",
    // Same for its 401 dead-session gate: the extension records a trip that
    // only the app (SyncManager) ever acts on.
    "ExpCore/Sources/SessionGate.swift",
    "ExpCore/Sources/Shared/**",
    // AccountStore compares instance URLs through `WebLinks.normalizedBase`, so
    // "same server" has exactly one normalization rule across the codebase.
    "ExpCore/Sources/Domain/WebLinks.swift",
    // VAPP-218: WebLinks classifies app-screen links through AppLink (Foundation-only).
    "ExpCore/Sources/Domain/AppLink.swift",
    "ExpCore/Sources/Auth/KeychainStore.swift",
    "ExpCore/Sources/Auth/AccountStore.swift",
    "ExpCore/Sources/Auth/ServerAccount.swift",
    "ExpCore/Sources/Auth/AuthRepository.swift",
    "ExpCore/Sources/API/HTTPClient.swift",
    "ExpCore/Sources/API/TrpcClient.swift",
    // The neutral plan-cap copy the compose form renders (EXP-216).
    "ExpCore/Sources/API/TrpcErrorInfo.swift",
    "ExpCore/Sources/API/IssuesApi.swift",
    "ExpCore/Sources/API/AttachmentsApi.swift",
    // EXP-1635: a chat started from a share: the targets model, the send and
    // its sealing (ExpPush carries the crypto).
    "ExpCore/Sources/Domain/DomainContract.generated.swift",
    "ExpCore/Sources/Domain/AgentOrdering.swift",
    "ExpCore/Sources/Domain/ShareTargets.swift",
    // SendSealing (#EXP-1683) classifies a file line.
    "ExpCore/Sources/Domain/AttachmentFiles.swift",
    "ExpCore/Sources/Domain/ComposerAgentRow.swift",
    "ExpCore/Sources/Domain/ComposerMenu.swift",
    "ExpCore/Sources/Domain/AgentComposerPrompt.swift",
    "ExpCore/Sources/Domain/HomeGreeting.swift",
    "ExpCore/Sources/Domain/FeedbackStates.swift",
    "ExpCore/Sources/SteerImageMessage.swift",
    "ExpCore/Sources/API/SessionsSendInput.swift",
    "ExpCore/Sources/API/SessionKeysApi.swift",
    "ExpCore/Sources/API/InstallsApi.swift",
    "ExpCore/Sources/API/RepositoriesApi.swift",
    "ExpCore/Sources/Sealed/SealedContent.swift",
    "ExpCore/Sources/Sealed/SealedKeyRing.swift",
    "ExpCore/Sources/Sealed/TreeKeyHolder.swift",
    "ExpCore/Sources/Sealed/SendSealing.swift",
    // THE composer card and agent row, ExpUI's own files: the extension links
    // no ExpUI, these compile under `SHARE_EXTENSION` (no `import ExpCore`).
    "ExpUI/Sources/AppIcon.swift",
    "ExpUI/Sources/AppIcons.generated.swift",
    "ExpUI/Sources/ColorHex.swift",
    "ExpUI/Sources/DesignTokens.generated.swift",
    "ExpUI/Sources/GlassTokens.swift",
    "ExpUI/Sources/Motion.swift",
    "ExpUI/Sources/GlassComposer.swift",
    "ExpUI/Sources/AgentBubbleCircle.swift",
    "ExpUI/Sources/ComposerAgentRowView.swift",
    "ExpUI/Sources/Feedback/FeedbackMark.swift",
    "ExpUI/Sources/Feedback/Spinner.swift",
    "ShareExtension/**",
]

// The .appex is its own bundle and needs its own manifest — it can't inherit the
// app's. Deliberately a SEPARATE file from the app's rather than a shared one:
// the curated source list above is much narrower than the app, so the extension
// declares only the UserDefaults reason plus the content the user shares (no
// name/email capture, no Firebase push token). Both extension targets share it.
let shareExtensionResources: ResourceFileElements = [
    "ShareExtension/PrivacyInfo.xcprivacy",
    // EXP-1635: the registry glyphs (an agent's icon can be any of them).
    "Exponential/Assets.xcassets",
]
let shareExtensionConditions = "$(inherited) SHARE_EXTENSION" + (hasEnetPush ? " ENET_PUSH" : "")

// Single source of truth for app + extension version; keep these in lockstep so
// the extension's CFBundleVersion never drifts from the parent app.
let appMarketingVersion = "0.14.119"
let appBuildVersion = "316"
// Staging always runs one build AHEAD of prod (EXP-405): the staging apps have
// their own store namespaces, so an equal number silently uploads and then
// masquerades as prod's build in ASC. Derived, so one bump covers all targets.
let stagingBuildVersion = String(Int(appBuildVersion)! + 1)

let shareExtensionInfoPlist: [String: Plist.Value] = [
    // Must match the parent app's version (CFBundleVersion mismatch trips
    // embeddedBinaryValidationUtility during signing).
    "CFBundleShortVersionString": .string(appMarketingVersion),
    "CFBundleVersion": .string(appBuildVersion),
    "NSExtension": .dictionary([
        "NSExtensionPointIdentifier": .string("com.apple.share-services"),
        "NSExtensionPrincipalClass": .string("$(PRODUCT_MODULE_NAME).ShareViewController"),
        "NSExtensionAttributes": .dictionary([
            // EXP-1635: direct share, the donated agents and chats
            // (`DirectShareDonations`) open this extension.
            "IntentsSupported": .array([.string("INSendMessageIntent")]),
            "NSExtensionActivationRule": .dictionary([
                "NSExtensionActivationSupportsImageWithMaxCount": .integer(20),
                "NSExtensionActivationSupportsText": .boolean(true),
                "NSExtensionActivationSupportsWebURLWithMaxCount": .integer(1),
                "NSExtensionActivationSupportsFileWithMaxCount": .integer(4),
            ]),
        ]),
    ]),
]

// The Notification Service Extension (prod + staging): ExpPush only, no
// Firebase, no GRDB (the extension's memory cap is ~24 MB).
let notificationServiceInfoPlist: [String: Plist.Value] = [
    // Lockstep with the parent app, like the Share Extension.
    "CFBundleShortVersionString": .string(appMarketingVersion),
    "CFBundleVersion": .string(appBuildVersion),
    "NSExtension": .dictionary([
        "NSExtensionPointIdentifier": .string("com.apple.usernotifications.service"),
        "NSExtensionPrincipalClass": .string("$(PRODUCT_MODULE_NAME).NotificationService"),
    ]),
]

// #EXP-1639 (live-activity.json): the Live Activity widget extension (prod +
// staging): ExpPush + the curated token files only, no GRDB, no ExpCore. It
// opens the sealed content-state with the push key in the shared keychain.
let liveActivityInfoPlist: [String: Plist.Value] = [
    "CFBundleShortVersionString": .string(appMarketingVersion),
    "CFBundleVersion": .string(appBuildVersion),
    "NSExtension": .dictionary([
        "NSExtensionPointIdentifier": .string("com.apple.widgetkit-extension"),
    ]),
]
let liveActivitySources: SourceFilesList = [
    "LiveActivity/**",
    "ExpUI/Sources/DesignTokens.generated.swift",
    "ExpUI/Sources/GlassTokens.swift",
    // DesignTokens' motion curves.
    "ExpUI/Sources/Motion.swift",
]
let liveActivityResources: ResourceFileElements = [
    "LiveActivity/Assets.xcassets",
    "LiveActivity/PrivacyInfo.xcprivacy",
]
let liveActivityConditions = "$(inherited) LIVE_ACTIVITY_EXTENSION" + (hasEnetPush ? " ENET_PUSH" : "")

let sharedInfoPlist: [String: Plist.Value] = [
    "CFBundleShortVersionString": .string(appMarketingVersion),
    "CFBundleVersion": .string(appBuildVersion),
    "UILaunchScreen": .dictionary([:]),
    // EXP-1635: the direct-share donations.
    "NSUserActivityTypes": .array([.string("INSendMessageIntent")]),
    // #EXP-1638 (app-shortcuts.json): the app icon's long-press menu, the
    // three statics in order (type = fixture id, title = fixture copy, icon =
    // the concept's lucide imageset); the recent chat is set at runtime
    // (`AppShortcutsPublisher`).
    "UIApplicationShortcutItems": .array([
        .dictionary([
            "UIApplicationShortcutItemType": .string("new-issue"),
            "UIApplicationShortcutItemTitle": .string("New issue"),
            "UIApplicationShortcutItemIconFile": .string("lucide-square-pen"),
        ]),
        .dictionary([
            "UIApplicationShortcutItemType": .string("new-chat"),
            "UIApplicationShortcutItemTitle": .string("New chat"),
            "UIApplicationShortcutItemIconFile": .string("lucide-message-circle"),
        ]),
        .dictionary([
            "UIApplicationShortcutItemType": .string("search"),
            "UIApplicationShortcutItemTitle": .string("Search"),
            "UIApplicationShortcutItemIconFile": .string("lucide-search"),
        ]),
    ]),
    "CFBundleURLTypes": .array([
        .dictionary([
            "CFBundleURLSchemes": .array([.string("exponential")]),
            "CFBundleURLName": .string("at.exponential.oauth"),
        ]),
    ]),
    // No `audio`: App Review (2.5.4) refuses it for a call with an agent, so
    // an iOS call ends when the app leaves the foreground (voice-call.json
    // BACKGROUND, ios-info-plist.test.ts).
    "UIBackgroundModes": .array([.string("remote-notification")]),
    // #EXP-1639: a running session on the Lock Screen (push-to-start).
    "NSSupportsLiveActivities": .boolean(true),
    "ITSAppUsesNonExemptEncryption": .boolean(false),
    // NEW-69: a call with an agent streams the microphone to the member's
    // voice device.
    "NSMicrophoneUsageDescription": .string("Exponential uses the microphone for calls with your agents."),
    // EXP-1635: the composer's + Camera row.
    "NSCameraUsageDescription": .string("Exponential uses the camera to attach a photo to your message."),
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
    // SLOP-18 / VAPP-88: an extension member is visible only in a file that
    // imports its module. Without it, ExpUI's dependency on the Exponential
    // UI SDK leaked the SDK's `Color(hex: String)` into every file of ExpUI
    // AND the app, silently out-ranking ExpUI's tolerant `Color(hex: String?)`
    // (and, in the app, failing to link).
    "SWIFT_UPCOMING_FEATURE_MEMBER_IMPORT_VISIBILITY": "YES",
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
            settings: .settings(base: baseSettings.merging(enetNodeSettings) { _, new in new })
        ),
        .target(
            name: "ExpCoreTests",
            destinations: [.iPhone, .iPad],
            product: .unitTests,
            bundleId: "at.exponential.core.tests",
            deploymentTargets: .iOS("17.4"),
            sources: expCoreTestSources,
            dependencies: [.target(name: "ExpCore")],
            // EnetInstallNodesTests run real nodes only when ExpCore links eNet.
            settings: .settings(base: baseSettings.merging(enetNodeSettings) { _, new in new })
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
            // PrimitivesConvergenceTests reads the SDK style structs the glass
            // wrappers build (one dynamic copy, see Tuist/Package.swift).
            dependencies: [.target(name: "ExpUI"), .external(name: "ExponentialUIPrimitives")],
            settings: .settings(base: baseSettings)
        ),
        // EXP-1435: the sealed-push core the app and the Notification Service
        // Extension share (static: linked into each, no embedded framework).
        .target(
            name: "ExpPush",
            destinations: [.iPhone, .iPad],
            product: .staticFramework,
            bundleId: "at.exponential.push",
            deploymentTargets: .iOS("17.4"),
            sources: expPushSources,
            dependencies: enetPushBinding,
            settings: .settings(base: baseSettings.merging(enetPushSettings) { _, new in new })
        ),
        // The push-seal fixture replay (packages/domain-contract/fixtures/
        // push-seal.json) + the userInfo mapping; runs in the ExpCore scheme.
        .target(
            name: "ExpPushTests",
            destinations: [.iPhone, .iPad],
            product: .unitTests,
            bundleId: "at.exponential.push.tests",
            deploymentTargets: .iOS("17.4"),
            sources: ["ExpPush/Tests/**"],
            dependencies: expPushDependencies,
            settings: .settings(base: baseSettings.merging(enetPushSettings) { _, new in new })
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
            // App-only entitlements (EXP-92): adds associated-domains for
            // Universal Links, which the ShareExtension (still on
            // Exponential.entitlements) must not inherit.
            entitlements: "ExponentialApp.entitlements",
            dependencies: sharedDependencies + expPushDependencies + [.target(name: "ExpCore"), .target(name: "ExpUI"), .target(name: "ShareExtension"), .target(name: "NotificationService"), .target(name: "LiveActivity")],
            settings: .settings(base: baseSettings.merging(surfaceSettings) { _, new in new })
        ),
        .target(
            name: "Exponential-Staging",
            destinations: [.iPhone, .iPad],
            product: .app,
            bundleId: "at.exponential.staging",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: sharedInfoPlist.merging([
                "CFBundleDisplayName": "Exp Staging",
                "CFBundleVersion": .string(stagingBuildVersion),
            ]) { _, new in new }),
            sources: sharedSources,
            resources: stagingResources,
            // App-only entitlements (EXP-92) — see the production target.
            entitlements: "ExponentialAppStaging.entitlements",
            dependencies: sharedDependencies + expPushDependencies + [.target(name: "ExpCore"), .target(name: "ExpUI"), .target(name: "ShareExtension-Staging"), .target(name: "NotificationService-Staging"), .target(name: "LiveActivity-Staging")],
            settings: .settings(base: baseSettings.merging(surfaceSettings) { _, new in new }.merging([
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
            // EXP-1267: the runner signs the suites in over plain HTTP
            // (http://localhost:5173) before launching the app — the same
            // local-networking allowance the app itself carries.
            infoPlist: .extendingDefault(with: [
                "NSAppTransportSecurity": .dictionary([
                    "NSAllowsLocalNetworking": .boolean(true),
                ]),
            ]),
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
            resources: shareExtensionResources,
            entitlements: "Exponential.entitlements",
            dependencies: expPushDependencies,
            settings: .settings(base: baseSettings.merging([
                "SWIFT_ACTIVE_COMPILATION_CONDITIONS": .string(shareExtensionConditions),
            ]) { _, new in new })
        ),
        .target(
            name: "ShareExtension-Staging",
            destinations: [.iPhone, .iPad],
            product: .appExtension,
            bundleId: "at.exponential.staging.shareextension",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: shareExtensionInfoPlist.merging([
                "CFBundleDisplayName": "Exp Staging",
                "CFBundleVersion": .string(stagingBuildVersion),
            ]) { _, new in new }),
            sources: shareExtensionSources,
            resources: shareExtensionResources,
            entitlements: "ExponentialStaging.entitlements",
            dependencies: expPushDependencies,
            settings: .settings(base: baseSettings.merging([
                "SWIFT_ACTIVE_COMPILATION_CONDITIONS": .string("\(shareExtensionConditions) STAGING"),
            ]) { _, new in new })
        ),
        .target(
            name: "NotificationService",
            destinations: [.iPhone, .iPad],
            product: .appExtension,
            bundleId: "at.exponential.notificationservice",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: notificationServiceInfoPlist.merging([
                "CFBundleDisplayName": "Exponential",
            ]) { _, new in new }),
            sources: ["NotificationService/**"],
            resources: ["NotificationService/PrivacyInfo.xcprivacy"],
            entitlements: "NotificationService.entitlements",
            dependencies: expPushDependencies,
            settings: .settings(base: baseSettings)
        ),
        .target(
            name: "NotificationService-Staging",
            destinations: [.iPhone, .iPad],
            product: .appExtension,
            bundleId: "at.exponential.staging.notificationservice",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: notificationServiceInfoPlist.merging([
                "CFBundleDisplayName": "Exp Staging",
                "CFBundleVersion": .string(stagingBuildVersion),
            ]) { _, new in new }),
            sources: ["NotificationService/**"],
            resources: ["NotificationService/PrivacyInfo.xcprivacy"],
            entitlements: "NotificationServiceStaging.entitlements",
            dependencies: expPushDependencies,
            settings: .settings(base: baseSettings.merging([
                "SWIFT_ACTIVE_COMPILATION_CONDITIONS": "$(inherited) STAGING",
            ]) { _, new in new })
        ),
        .target(
            name: "LiveActivity",
            destinations: [.iPhone, .iPad],
            product: .appExtension,
            bundleId: "at.exponential.liveactivity",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: liveActivityInfoPlist.merging([
                "CFBundleDisplayName": "Exponential",
            ]) { _, new in new }),
            sources: liveActivitySources,
            resources: liveActivityResources,
            entitlements: "LiveActivity.entitlements",
            dependencies: expPushDependencies + [
                .sdk(name: "WidgetKit", type: .framework),
                .sdk(name: "SwiftUI", type: .framework),
            ],
            settings: .settings(base: baseSettings.merging([
                "SWIFT_ACTIVE_COMPILATION_CONDITIONS": .string(liveActivityConditions),
            ]) { _, new in new })
        ),
        .target(
            name: "LiveActivity-Staging",
            destinations: [.iPhone, .iPad],
            product: .appExtension,
            bundleId: "at.exponential.staging.liveactivity",
            deploymentTargets: .iOS("17.4"),
            infoPlist: .extendingDefault(with: liveActivityInfoPlist.merging([
                "CFBundleDisplayName": "Exp Staging",
                "CFBundleVersion": .string(stagingBuildVersion),
            ]) { _, new in new }),
            sources: liveActivitySources,
            resources: liveActivityResources,
            entitlements: "LiveActivityStaging.entitlements",
            dependencies: expPushDependencies + [
                .sdk(name: "WidgetKit", type: .framework),
                .sdk(name: "SwiftUI", type: .framework),
            ],
            settings: .settings(base: baseSettings.merging([
                "SWIFT_ACTIVE_COMPILATION_CONDITIONS": .string("\(liveActivityConditions) STAGING"),
            ]) { _, new in new })
        ),
    ],
    schemes: [
        .scheme(
            name: "ExpCore",
            // EXP-1435: ExpPushTests ride the ExpCore scheme, so `bun run
            // ios:test` and test-ios.yml run the push-seal fixture with it.
            buildAction: .buildAction(targets: ["ExpCore", "ExpCoreTests", "ExpPushTests"]),
            testAction: .targets(["ExpCoreTests", "ExpPushTests"])
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
