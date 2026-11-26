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
        // #EXP-2168: the pushed page header (THE round back, the title
        // LEFT); Mark all read = the page's own round button.
        .pageHeader(Chats.Copy.inbox, pushed: true, glyph: nil) {
            if (inboxViewModel?.totalUnread ?? 0) > 0 {
                PageHeaderButton(icon: AppIcons.notificationMarkRead, label: "Mark all read") { inboxViewModel?.markAllRead() }
            }
        } more: {
            EmptyView()
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
