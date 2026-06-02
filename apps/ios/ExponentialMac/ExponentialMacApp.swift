import ExpCore
import ExpUI
import SwiftUI

@main
struct ExponentialMacApp: App {
    @State private var deps = MacAppDependencies()

    var body: some Scene {
        WindowGroup {
            MacRootView()
                .environment(deps)
                .preferredColorScheme(.dark)
                .frame(minWidth: 900, minHeight: 600)
        }
        .commands {
            SidebarCommands()
        }

        Settings {
            MacSettingsView()
                .environment(deps)
                .frame(width: 460, height: 380)
                .preferredColorScheme(.dark)
        }
    }
}
