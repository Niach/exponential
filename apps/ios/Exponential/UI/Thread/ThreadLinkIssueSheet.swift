import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-1284: "Link an issue" — THE search list (`SearchListView`,
/// search-list.json) in our sheet chrome, over the thread's TEAM's synced
/// issues minus the ones already in it, ranked by the shared engine
/// (`IssueSearch.rank`, EXP-892) at its default limit. A pick hands the id
/// back; the host links it (`sessions.linkIssues`) and toasts a refusal.
struct ThreadLinkIssueSheet: View {
    let teamId: String
    /// Already in the thread — never offered.
    let excluded: Set<String>
    let onPick: (String) -> Void

    var body: some View {
        ThreadIssueSearchList(teamId: teamId, excluded: excluded, sheet: true) { onPick($0.id) }
            .accessibilityIdentifier("thread-link-issue-sheet")
    }
}

/// THE issue search list over a team's synced issues — the Link an issue
/// sheet's body, and (S-3, composer-menu.json) the session / agent
/// composer's "+" Issue page, whose pick inserts `#IDENT` at the caret.
struct ThreadIssueSearchList: View {
    let teamId: String
    var excluded: Set<String> = []
    /// In its own sheet (the search list's sheet chrome); false = a page.
    var sheet = false
    let onPick: (IssueEntity) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var query = ""
    @State private var pool: [IssueEntity] = []

    var body: some View {
        Group {
            if sheet { SearchListSheet(list) } else { list }
        }
        .task { load() }
    }

    private var list: SearchListView<String> {
        SearchListView(
            items: ranked.map(Self.item),
            query: $query,
            placeholder: IssueSearchCopy.placeholder,
            autofocus: true,
            emptyText: IssueSearchCopy.noMatches,
            onPick: { id in
                if let issue = pool.first(where: { $0.id == id }) { onPick(issue) }
            }
        )
    }

    private var ranked: [IssueEntity] {
        IssueSearch.rank(
            pool.filter { !excluded.contains($0.id) },
            query: query,
            projection: { issue in
                IssueSearch.Row(
                    id: issue.id,
                    identifier: issue.identifier ?? "",
                    title: issue.title,
                    description: issue.description,
                    createdAt: issue.createdAt,
                    updatedAt: issue.updatedAt,
                    status: issue.status
                )
            }
        )
    }

    /// The team's issues (issues sync no team id: through its boards).
    private func load() {
        guard let dbPool = try? deps.db.pool(forAccountId: accountId) else { return }
        let teamId = teamId
        pool = (try? dbPool.read { db in
            let boardIds = try BoardEntity
                .filter(Column("team_id") == teamId)
                .fetchAll(db)
                .map(\.id)
            return try IssueEntity
                .filter(boardIds.contains(Column("board_id")))
                .order(Column("updated_at").desc)
                .fetchAll(db)
        }) ?? []
    }

    /// The picker row (status glyph, `IDENT Title`), keyed by issue id.
    static func item(_ issue: IssueEntity) -> PickerItem<String> {
        let status = IssueStatus.from(issue.status)
        let identifier = issue.identifier ?? ""
        return PickerItem(
            value: issue.id,
            label: identifier.isEmpty ? issue.title : "\(identifier) \(issue.title)",
            icon: status.iconName,
            color: status.color,
            keywords: [identifier, issue.title]
        )
    }
}
