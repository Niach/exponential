import AppKit
import ExpCore
import ExpUI
import SwiftUI

/// Quit teardown: tear the local device preview down (killing its emulator /
/// serve-sim / dev-server children) so no orphaned processes survive the app.
final class MacAppDelegate: NSObject, NSApplicationDelegate {
    @MainActor static var onTerminate: (() -> Void)?

    func applicationWillTerminate(_ notification: Notification) {
        // applicationWillTerminate is delivered on the main thread.
        MainActor.assumeIsolated { Self.onTerminate?() }
    }
}

@main
struct ExponentialMacApp: App {
    @NSApplicationDelegateAdaptor(MacAppDelegate.self) private var appDelegate
    @State private var deps = MacAppDependencies()

    var body: some Scene {
        WindowGroup {
            MacRootView()
                .environment(deps)
                .preferredColorScheme(.dark)
                .frame(minWidth: 900, minHeight: 600)
                .onAppear {
                    let deps = deps
                    MacAppDelegate.onTerminate = {
                        // Tear the preview down (kills emulator/serve-sim/dev
                        // server + frees ports) so no orphan child survives the app.
                        deps.previewController.shutdown()
                    }
                }
        }
        .commands {
            SidebarCommands()
            MacAppCommands()
        }

        Settings {
            MacSettingsView()
                .environment(deps)
                .frame(width: 460, height: 380)
                .preferredColorScheme(.dark)
        }
    }
}

// MARK: - Menu commands

/// `New Issue` (⌘N) routed to whichever issue list currently owns the scene via
/// a focused scene value. The list publishes a closure when a project is
/// selected and the user can create; otherwise the menu item is disabled.
struct CreateIssueActionKey: FocusedValueKey {
    typealias Value = () -> Void
}

extension FocusedValues {
    var createIssueAction: (() -> Void)? {
        get { self[CreateIssueActionKey.self] }
        set { self[CreateIssueActionKey.self] = newValue }
    }
}

struct MacAppCommands: Commands {
    @FocusedValue(\.createIssueAction) private var createIssue

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New Issue") { createIssue?() }
                .keyboardShortcut("n", modifiers: .command)
                .disabled(createIssue == nil)
        }
    }
}
