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
                // A suspension leaves every shape long-poll either parked on
                // escalated backoff (up to 30s) or holding a socket that won't
                // fail until the 90s request timeout, so a returning user can
                // stare at stale data for a minute. Restart the pipelines on
                // the way back in — no wipe, same reconnect-on-.active idiom
                // AgentSessionView uses for its steer socket.
                .onChange(of: scenePhase) { _, newPhase in
                    switch newPhase {
                    case .background: dependencies.syncManager.sceneDidEnterBackground()
                    case .active:
                        dependencies.syncManager.sceneDidBecomeActive()
                        // The steer sockets rarely survive a suspension either
                        // (EXP-243) — and since EXP-621 they are app-scoped, so
                        // revival belongs here rather than in the one session
                        // screen that happens to be mounted.
                        dependencies.steerSessions.reconnectAll()
                    default: break
                    }
                }
        }
    }
}
