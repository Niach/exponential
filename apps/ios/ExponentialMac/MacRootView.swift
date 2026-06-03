import ExpCore
import ExpUI
import SwiftUI

/// Auth gate: show the login flow until at least one account is signed in,
/// then the main split-view shell. Mirrors the iOS `AppNavigator` gate.
struct MacRootView: View {
    @Environment(MacAppDependencies.self) private var deps

    var body: some View {
        Group {
            if deps.auth.accounts.contains(where: { $0.token != nil }) {
                MacShell()
            } else {
                MacLoginView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(AppBackground())
    }
}
