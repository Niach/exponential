import ExpCore
import ExpUI
import SwiftUI

/// #FEED-191 NEEDS YOU, the phone's page (needs-you.json THE LIST): pushed from
/// the Chats list's first row, the page header with the back control (the
/// Archived page's), one reading column of THE inbox page's rows, the ACTIVE
/// team's. A press opens the chat by the row's `opens`.
struct NeedsYouPage: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(SessionListsModel.self) private var lists
    @Environment(TeamState.self) private var teamState
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute

    /// The clock the ages read.
    @State private var now = Date()

    var body: some View {
        ZStack {
            AppBackground()
            if lists.loaded {
                NeedsYouPageContent(
                    rows: lists.needsYou(teamId: teamState.activeTeamId, me: deps.auth.userId, now: now)
                ) { row in
                    pushRoute(AppRoute.openingNeedsYou(row, accountId: accountId, deps: deps))
                }
            } else {
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .pageHeader(NeedsYou.Copy.entry, pushed: true)
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                now = Date()
            }
        }
    }
}

/// THE list in its scroll; a pure painter (the `needs-you` specimen too).
struct NeedsYouPageContent: View {
    let rows: [NeedsYou.Row]
    let onOpen: (NeedsYou.Row) -> Void

    var body: some View {
        if rows.isEmpty {
            NeedsYouList(rows: [], onOpen: onOpen)
                .frame(maxHeight: .infinity)
        } else {
            ScrollView {
                NeedsYouList(rows: rows, onOpen: onOpen)
                    .padding(.horizontal, 16)
                    .padding(.top, 4)
                    .padding(.bottom, 16)
            }
            .scrollContentBackground(.hidden)
            .tabBarBottomInset()
        }
    }
}
