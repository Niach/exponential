import ExpCore
import ExpUI
import SwiftUI

/// Auth gate: show the login flow until at least one account is signed in,
/// then the main split-view shell. Mirrors the iOS `AppNavigator` gate.
struct MacRootView: View {
    @Environment(MacAppDependencies.self) private var deps

    @State private var invite: InviteTarget?

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
        // Handle exp://invite/<token> (the `exp` scheme is registered in the
        // mac Info.plist). Mirrors the iOS AppNavigator deep-link routing.
        .onOpenURL { url in
            guard url.host == "invite",
                  let token = url.pathComponents.dropFirst().first,
                  let accountId = deps.auth.activeAccountId else { return }
            invite = InviteTarget(accountId: accountId, token: String(token))
        }
        .sheet(item: $invite) { target in
            MacInviteAcceptView(accountId: target.accountId, token: target.token)
                .environment(deps)
                .preferredColorScheme(.dark)
        }
    }

    private struct InviteTarget: Identifiable {
        let accountId: String
        let token: String
        var id: String { token }
    }
}
