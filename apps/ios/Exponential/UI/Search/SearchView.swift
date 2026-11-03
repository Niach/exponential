import ExpUI
import ExpCore
import SwiftUI

/// The Search page: issue search over the ACTIVE team's boards (#EXP-1382,
/// search-list.json TEAM SCOPE), augmented by a debounced server full-text
/// search (description + comments) for that team. Pure search (EXP-58): the empty-query state is a hint — the
/// "Assigned to you" list that used to live here moved to My Work.
///
/// SLOP-28 (EXP-1259 D4): it is THE search list (`SearchListView`,
/// search-list.json), the one the `#` typeahead and the composer's pickers
/// draw: the input row on top (focused on open), the picker's own rows
/// (status glyph, `IDENT Title`) under it, the result count once a query is
/// typed; a row (or Return on the top one) opens the issue.
struct SearchView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(TeamState.self) private var teamState
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel: SearchViewModel?
    @State private var query = ""

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
                // The results are the view model's STATE (ranked off the main
                // actor, #EXP-1382) — the input paints every keystroke at once.
                SearchListView(
                    items: vm.results.map { Self.item($0, team: vm.teamStatuses) },
                    query: $query,
                    // EXP-922: the ×4 copy set — same words on web, desktop,
                    // iOS and Android (web `lib/issue-search.ts`;
                    // `issue-search-surfaces.test.ts` greps this file).
                    placeholder: "Search issues",
                    inputIdentifier: "search-field",
                    autofocus: true,
                    // No empty state while the rank is still owed (#EXP-1382).
                    emptyText: trimmed.isEmpty || vm.isRanking ? nil : "No issues match",
                    onPick: { issueId in
                        pushRoute(.issue(accountId: accountId, id: issueId))
                    },
                    leading: AnyView(backButton),
                    fieldChrome: true
                )
                .overlay {
                    if trimmed.isEmpty { searchHint.allowsHitTesting(false) }
                }
            }
        }
        // EXP-1649: web-mobile's search page has no title bar: the back
        // arrow rides the field's row (the navigator's edge swipe stays
        // armed by `bareBackButton`).
        .navigationTitle("Search")
        .toolbar(.hidden, for: .navigationBar)
        .onAppear {
            if viewModel == nil {
                viewModel = SearchViewModel(accountId: accountId, db: deps.db, issuesApi: deps.issuesApi)
            }
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it. Same for
            // the server search — onDisappear cancels any in-flight request.
            viewModel?.startObserving(teamId: teamState.activeTeamId)
        }
        .onChange(of: teamState.activeTeamId) { _, teamId in
            // Switching the team re-runs the query against it.
            viewModel?.startObserving(teamId: teamId)
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
        .onChange(of: query) { _, newValue in
            // Debounced + cancelled-on-keystroke inside the VM — never blocks
            // typing.
            viewModel?.queryChanged(newValue)
        }
    }

    /// One result as THE picker row (EXP-922's flat list, SLOP-28's one row
    /// type): the issue's TEAM status glyph (the search is team-scoped,
    /// #EXP-1382; EXP-1554: web's clocks), `IDENT Title` with the identifier
    /// mono + muted. Keyed by the issue id.
    static func item(_ result: SearchViewModel.Result, team: [ResolvedIssueStatus]) -> PickerItem<String> {
        let issue = result.issue
        let status = IssueStatusResolver.resolve(issue, team: team)
        return PickerItem(
            value: issue.id,
            label: "\(issue.identifier ?? "") \(issue.title)",
            icon: status.iconName,
            color: status.color,
            identifier: issue.identifier
        )
    }

    /// Web's ghost `icon-xs` back button: the bare `ui-back` arrow, muted.
    private var backButton: some View {
        Button {
            dismiss()
        } label: {
            AppIcon(AppIcons.uiBack, size: 16)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 24, height: 24)
                // A thumb-sized target around the 24pt glyph box.
                .contentShape(Rectangle().inset(by: -10))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Back")
        .accessibilityIdentifier("BackButton")
    }

    // MARK: - Empty-query state (search hint)

    private var searchHint: some View {
        EmptyState(
            title: "Search issues across this team's boards.",
            description: "Matches identifiers, titles, and full text.",
            icon: AppIcons.navSearch
        )
        .frame(maxHeight: .infinity)
    }
}
