import FirebaseCore
import SwiftUI

@main
struct ExponentialApp: App {
    @State private var dependencies = AppDependencies()

    init() {
        FirebaseApp.configure()
    }

    var body: some Scene {
        WindowGroup {
            AppNavigator()
                .environment(dependencies)
                .preferredColorScheme(.dark)
        }
    }
}
