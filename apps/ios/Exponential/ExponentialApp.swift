import FirebaseCore
import SwiftUI

@main
struct ExponentialApp: App {
    @State private var dependencies = AppDependencies()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        FirebaseApp.configure()
        // Heal installs whose shared URLCache holds poisoned Electric shape
        // snapshots (shape responses carry `cache-control: public` and older
        // builds fetched them through the default cache — a stale empty
        // snapshot replayed on refetch wiped all local rows). ShapeClient now
        // bypasses URLCache entirely; this purge cleans up what's left behind.
        URLCache.shared.removeAllCachedResponses()
    }

    var body: some Scene {
        WindowGroup {
            AppNavigator()
                .environment(dependencies)
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
                    case .background: dependencies.syncManager.sceneDidEnterBackground()
                    case .active:
                        dependencies.syncManager.sceneDidBecomeActive()
                        // The steer sockets rarely survive a suspension either
                        // (EXP-243) — and since EXP-621 they are app-scoped, so
                        // revival belongs here rather than in the one session
                        // screen that happens to be mounted.
                        dependencies.steerSessions.reconnectAll(reason: "foreground")
                    default: break
                    }
                }
        }
    }
}
