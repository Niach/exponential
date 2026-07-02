import FirebaseCore
import SwiftUI

@main
struct ExponentialApp: App {
    @State private var dependencies = AppDependencies()

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
        }
    }
}
