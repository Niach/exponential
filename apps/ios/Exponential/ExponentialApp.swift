import FirebaseCore
import SwiftUI
import ExpCore
import ExpUI

@main
struct ExponentialApp: App {
    // #EXP-1638: the scene delegate receives the app icon's shortcuts.
    @UIApplicationDelegateAdaptor(ExponentialAppDelegate.self) private var appDelegate
    @State private var dependencies: AppDependencies
    @Environment(\.scenePhase) private var scenePhase

    init() {
        // #LOGS-2: crash reporting first, from the last cached server answer.
        Observability.launch(origins: [])
        // Built first, as the property initializer did before (#EXP-1640:
        // held here so an intent launched in the background reaches it).
        let dependencies = AppDependencies()
        _dependencies = State(initialValue: dependencies)
        AppIntentsHost.deps = dependencies
        MonitoringState.shared.setOrigins(dependencies.auth.accounts.map(\.instanceUrl))
        FirebaseApp.configure()
        // Heal installs whose shared URLCache holds poisoned Electric shape
        // snapshots (shape responses carry `cache-control: public` and older
        // builds fetched them through the default cache — a stale empty
        // snapshot replayed on refetch wiped all local rows). ShapeClient now
        // bypasses URLCache entirely; this purge cleans up what's left behind.
        URLCache.shared.removeAllCachedResponses()
    }

    private var monitoringKey: String {
        let auth = dependencies.auth
        let active = auth.accounts.first { $0.id == auth.activeAccountId }
        return ([active?.instanceUrl ?? "", active?.userId ?? ""] + auth.accounts.map(\.instanceUrl)).joined(separator: "|")
    }

    var body: some Scene {
        WindowGroup {
            AppNavigator()
                .environment(dependencies)
                .onAppear { AppShortcutItems.deps = dependencies }
                // #LOGS-2: follow the active server's monitoring config.
                .task(id: monitoringKey) {
                    let auth = dependencies.auth
                    let active = auth.accounts.first { $0.id == auth.activeAccountId }
                    await Observability.refresh(
                        activeOrigin: active?.instanceUrl ?? AppConstants.defaultCloudUrl,
                        origins: auth.accounts.map(\.instanceUrl),
                        userId: active?.userId
                    )
                }
                .preferredColorScheme(.dark)
                // EXP-643: every Toggle in the tree (sheets included) renders
                // the black-on-white glass switch instead of the system one.
                .toggleStyle(.glass)
                // EXP-656: the shape pipelines PARK on the way out and relaunch
                // on the way back in. Carrying 19 long-polls into a suspension
                // left them holding sockets the OS had already killed — nothing
                // failed until the 90s request timeout, so a returning user
                // stared at pre-sleep data (a machine still reading "Paused")
                // for as long as that took. The relaunch resumes from each
                // shape's persisted cursor with a non-live catch-up poll, which
                // answers in one round trip.
                .onChange(of: scenePhase) { _, newPhase in
                    switch newPhase {
                    case .background:
                        dependencies.syncManager.sceneDidEnterBackground()
                        // #FEED-219: pending composer drafts go out now.
                        let composerDrafts = dependencies.composerDrafts
                        Task { @MainActor in composerDrafts.flushAll() }
                        // No `audio` background mode (App Review 2.5.4): a
                        // call ends cleanly instead of freezing mid-stream.
                        VoiceCallClient.shared.end()
                        // #EXP-1528: the connection analytics go out on the way out.
                        let connStats = dependencies.connStats
                        Task { await connStats.flush() }
                    case .active:
                        dependencies.syncManager.sceneDidBecomeActive()
                        // The steer sockets rarely survive a suspension either
                        // (EXP-243) — and since EXP-621 they are app-scoped, so
                        // revival belongs here rather than in the one session
                        // screen that happens to be mounted.
                        // EXP-1531: the eNet node re-probes the network and
                        // migrates its paths alongside (iOS never tells it).
                        dependencies.installNode?.networkChange()
                        dependencies.steerSessions.reconnectAll(reason: "foreground")
                    default: break
                    }
                }
        }
    }
}
