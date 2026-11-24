import ExpCore
import ExpUI
import SwiftUI

/// EXP-1373 (chats.json `shell`): the INBOX page — pushed from a root
/// page's inbox circle; notifications only (no segments: My issues and
/// Drafts moved to the board's chips), every team's rows. Mark all read rides the header while any is unread.
struct InboxPage: View {
    @Environment(AppDependencies.self) private var deps
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
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            inboxViewModel?.startObserving()
        }
        .onDisappear {
            inboxViewModel?.stopObserving()
        }
        .accessibilityIdentifier("inbox-page")
    }
}
