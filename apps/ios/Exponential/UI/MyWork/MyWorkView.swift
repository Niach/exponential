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
        .toolbar {
            if let vm = inboxViewModel, vm.totalUnread > 0 {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Mark all read") { vm.markAllRead() }
                }
            }
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
