import ExpCore
import ExpUI
import SwiftUI

/// EXP-1373 (chats.json `shell`): the INBOX page — pushed from the Chats
/// header's inbox circle; notifications only (no segments: My issues and
/// Drafts moved to the board's chips), the ACTIVE team's rows (one team on
/// a phone, no bands). Mark all read rides the header while any is unread.
struct InboxPage: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @Environment(\.accountId) private var accountId
    @State private var inboxViewModel: InboxViewModel?

    var body: some View {
        ZStack {
            AppBackground()
            if let vm = inboxViewModel {
                InboxListContent(viewModel: vm)
            }
        }
        .navigationTitle(Chats.Copy.inbox)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        // EXP-1647: web's header pill (`Pill mode="action" size="md"`), its
        // own chrome: never iOS 26's large Liquid Glass item capsule.
        .ownChromeToolbarItem(
            placement: .topBarTrailing,
            enabled: (inboxViewModel?.totalUnread ?? 0) > 0
        ) {
            GlassPill("Mark all read", size: .md, mode: .action { inboxViewModel?.markAllRead() })
        }
        .onAppear {
            if inboxViewModel == nil {
                inboxViewModel = InboxViewModel(
                    accountId: accountId,
                    db: deps.db,
                    auth: deps.auth,
                    notificationsApi: deps.notificationsApi
                )
            }
            inboxViewModel?.teamId = teamState.activeTeamId
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            inboxViewModel?.startObserving()
        }
        .onDisappear {
            inboxViewModel?.stopObserving()
        }
        .onChange(of: teamState.activeTeamId) { _, teamId in
            inboxViewModel?.teamId = teamId
        }
        .accessibilityIdentifier("inbox-page")
    }
}
