import ExpUI
import ExpCore
import SwiftUI

/// The Search tab: cross-board issue search over the active account's local
/// data, augmented by a debounced server full-text search (description +
/// comments). The glass field mirrors the inline search that used to live in
/// the issue list. Pure search (EXP-58): the empty-query state is a hint —
/// the "Assigned to you" list that used to live here moved to My Work.
struct SearchView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var viewModel: SearchViewModel?
    @State private var query = ""
    @FocusState private var searchFocused: Bool

    var body: some View {
        ZStack {
            AppBackground()

            VStack(spacing: 0) {
                searchField
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)

                if let vm = viewModel {
                    let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
                    if trimmed.isEmpty {
                        searchHint
                    } else {
                        let results = vm.results(for: trimmed)
                        if results.isEmpty {
                            noResults
                        } else {
                            resultsList(results)
                        }
                    }
                }
            }
        }
        .navigationTitle("Search")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .onAppear {
            if viewModel == nil {
                viewModel = SearchViewModel(accountId: accountId, db: deps.db, issuesApi: deps.issuesApi)
            }
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it. Same for
            // the server search — onDisappear cancels any in-flight request.
            viewModel?.startObserving()
            viewModel?.queryChanged(query)
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
        .onChange(of: query) { _, newValue in
            // Debounced + cancelled-on-keystroke inside the VM — never blocks
            // typing; local substring results stay instant.
            viewModel?.queryChanged(newValue)
        }
    }

    // Custom glass search field. NOT system .searchable — on iOS 26+ the
    // navigationBarDrawer placement renders as a bottom-edge glass bar on
    // iPhone, colliding with the floating tab bar.
    private var searchField: some View {
        GlassSheetSearchField(
            // EXP-922: the ×4 copy set — same words on web, desktop, iOS and
            // Android (web `lib/issue-search.ts`; `issue-search-surfaces.test.ts` greps this file).
            placeholder: "Search issues",
            text: $query,
            accessibilityIdentifier: "search-field"
        )
        .focused($searchFocused)
    }

    // MARK: - Empty-query state (search hint)

    private var searchHint: some View {
        VStack(spacing: 12) {
            Spacer()
            AppIcon(AppIcons.navSearch, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("Search issues across all your boards.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Matches identifiers, titles, and full text.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    // MARK: - Results

    private var noResults: some View {
        VStack(spacing: 12) {
            Spacer()
            AppIcon(AppIcons.uiEmptySearch, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No issues match")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    /// EXP-922: ONE flat relevance-ordered list — the web sheet's and the
    /// desktop palette's shape. The board moved onto each row's sub-line; the
    /// per-board sections are gone, because banding fought the ranking (a
    /// board's header jumped to wherever its best hit landed and pulled its
    /// weaker hits up with it, so the same query read differently here than on
    /// every other client).
    @ViewBuilder
    private func resultsList(_ results: [SearchViewModel.Result]) -> some View {
        List {
            ForEach(results) { result in
                resultRow(result)
                    .listRowBackground(Color.clear)
                    .listRowSeparator(.hidden)
                    .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
            }
        }
        .listStyle(.plain)
        // Same compact-list treatment as the board IssueListView (EXP-80):
        // zero the List's own content margins, kill the implicit 44pt row
        // floor, and flow sections without the inter-section band, so search
        // results match the issue list's row rhythm.
        .contentMargins(.horizontal, 0, for: .scrollContent)
        .contentMargins(.top, 0, for: .scrollContent)
        .environment(\.defaultMinListRowHeight, 0)
        .listSectionSpacing(0)
        .scrollContentBackground(.hidden)
        .background(Color.clear)
        // No tab-bar clearance: EXP-686 made Search a pushed detail, so the
        // floating bar is hidden here and the full height is ours.
    }

    /// The ×4 search row (EXP-922): the issue's status glyph, its title, and a
    /// sub-line carrying the board's own glyph, the board name and the
    /// identifier — byte-for-byte the web `IssueSearchSheet` row and the
    /// desktop palette's `render_issue_row`. Priority is deliberately absent:
    /// no other client shows it here.
    @ViewBuilder
    private func resultRow(_ result: SearchViewModel.Result) -> some View {
        let issue = result.issue
        NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
            HStack(spacing: 10) {
                // Anchor glyph (EXP-314): search spans teams, and status rows
                // are team-scoped — the anchor renders correctly for builtins
                // and is the right neutral fallback for customs.
                AppIcon(IssueStatus.from(issue.status).iconName, size: AppIcon.Size.small)
                    .foregroundStyle(IssueStatus.from(issue.status).color)
                    .frame(width: 16)

                VStack(alignment: .leading, spacing: 2) {
                    Text(issue.title)
                        .font(.subheadline)
                        .foregroundStyle(.white)
                        .lineLimit(1)

                    HStack(spacing: 6) {
                        if let board = result.board {
                            AppIcon(BoardTypeDisplay.iconName(for: board), size: 11)
                                .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
                            Text("\(board.name) · \(issue.identifier ?? "")")
                                .lineLimit(1)
                        } else {
                            Text(issue.identifier ?? "")
                                .lineLimit(1)
                        }
                    }
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
    }
}
