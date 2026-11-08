import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-1284: "Link an issue" — THE search list (`SearchListView`,
/// search-list.json) in our sheet chrome, over the chat's TEAM's synced
/// issues minus the ones already in it, ranked by the shared engine
/// (`IssueSearch.rank`, EXP-892) at its default limit. A pick hands the id
/// back; the host links it (`sessions.linkIssues`) and toasts a refusal.
struct ChatLinkIssueSheet: View {
    let teamId: String
    /// Already in the chat — never offered.
    let excluded: Set<String>
    let onPick: (String) -> Void

    var body: some View {
        ChatIssueSearchList(teamId: teamId, excluded: excluded, sheet: true) { onPick($0.id) }
            .accessibilityIdentifier("chat-link-issue-sheet")
    }
}

/// THE issue search list over a team's synced issues — the Link an issue
/// sheet's body, and (S-3, composer-menu.json) the session / agent
/// composer's "+" Issue page, whose pick inserts `#IDENT` at the caret.
struct ChatIssueSearchList: View {
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
            keywords: [identifier, issue.title],
            identifier: identifier
        )
    }
}

/// EXP-1295 / #EXP-1732: THE Menu of a session screen's issues on a phone,
/// in its SHEET presentation (never an inline popover): the Issues tab's
/// `+` opens it untitled with Link an issue · New issue; an issue row's long
/// press titles it with the identifier, the same two rows, then `Unlink
/// <IDENT>` (destructive). A pick hands the action back; the host runs it
/// once the sheet has dismissed (a push or a second sheet from under a
/// dismissing one is dropped).
struct ChatIssueMenuSheet: View {
    enum Action: Equatable {
        case linkIssue
        case newIssue
        case unlink
    }

    static let linkIssueLabel = "Link an issue"
    static let newIssueLabel = "New issue"
    static func unlinkLabel(_ identifier: String) -> String { "Unlink \(identifier)" }

    /// The issue row's identifier; nil = the "+" (no title, no Unlink).
    let identifier: String?
    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: identifier) {
            VStack(spacing: 0) {
                // EXP-1334: THE chat menu's rows ×4 (desktop
                // `chat_menu_rows`): Link an issue · New issue, and on an
                // issue row a separator + Unlink.
                row(.linkIssue, Self.linkIssueLabel, icon: AppIcons.navSearch)
                row(.newIssue, Self.newIssueLabel, icon: AppIcons.uiAdd)
                if let identifier {
                    GlassDivider()
                        .padding(.vertical, 4)
                    row(.unlink, Self.unlinkLabel(identifier), icon: AppIcons.uiClose, destructive: true)
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.top, identifier == nil ? GlassSheetTokens.headerTopPadding : 0)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("chat-menu-sheet")
        }
    }

    private func row(
        _ action: Action, _ title: String, icon: String, destructive: Bool = false
    ) -> some View {
        Button {
            onPick(action)
        } label: {
            GlassMenuItemLabel(title, icon: icon, destructive: destructive)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("chat-menu-\(Self.testId(action))")
    }

    private static func testId(_ action: Action) -> String {
        switch action {
        case .linkIssue: "link-issue"
        case .newIssue: "new-issue"
        case .unlink: "unlink"
        }
    }
}
