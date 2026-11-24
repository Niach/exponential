import ExpUI
import ExpCore
import SwiftUI

/// The Search page: issue search over every member team's boards (#EXP-2109,
/// search-list.json), augmented by a debounced server full-text search
/// (description + comments) over those teams. Pure search (EXP-58): the empty-query state is a hint — the
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
    @Environment(SessionListsModel.self) private var lists
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel: SearchViewModel?
    @State private var query = ""

    /// The empty-query detail, the ×4 copy (`issue-search-surfaces.test.ts`
    /// greps this file for it).
    static let emptyDetail = "Matches identifiers, titles and full text. Filter with by:, is:, status:, created: and updated:."

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
                let shown = sections(vm)
                // The results are the view model's STATE (ranked off the main
                // actor, #EXP-1382) — the input paints every keystroke at once.
                SearchListView(
                    items: vm.results.map { Self.item($0, team: vm.boardStatuses[$0.issue.boardId] ?? []) },
                    query: $query,
                    // EXP-922: the ×4 copy set — same words on web, desktop,
                    // iOS and Android (web `lib/issue-search.ts`;
                    // `issue-search-surfaces.test.ts` greps this file).
                    // #EDE-99: with a chat store the search finds chats too.
                    placeholder: vm.hasChatStore ? SearchList.GlobalChats.Copy.placeholder : "Search issues",
                    inputIdentifier: "search-field",
                    autofocus: true,
                    // No empty state while the rank is still owed (#EXP-1382).
                    emptyText: trimmed.isEmpty || vm.isRanking ? nil
                        : vm.hasChatStore ? SearchList.GlobalChats.Copy.noResults : "No issues match",
                    onPick: { issueId in
                        pushRoute(.issue(accountId: accountId, id: issueId))
                    },
                    leading: AnyView(backButton),
                    fieldChrome: true,
                    sections: shown.titled ? SearchListSections(
                        issuesTitle: SearchList.GlobalChats.Copy.issues,
                        count: vm.isRanking && vm.results.isEmpty ? nil : shown.count,
                        empty: shown.empty,
                        after: chatsSection(vm, shown: shown),
                        submit: { if let top = vm.chatRows.first { pick(top, vm: vm) } }
                    ) : nil,
                    // #NEW-133: a recognised sentence = ONE action row on
                    // top; a pick rewrites the field, the search re-runs.
                    action: vm.suggestion.map { suggestion in
                        SearchListAction(
                            icon: AppIcons.conceptOrName(SearchList.SuggestionSpec.icon) ?? AppIcons.navSearch,
                            label: suggestion.label,
                            identifier: SearchList.SuggestionSpec.testId
                        ) {
                            query = suggestion.query
                        }
                    }
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
            viewModel?.startObserving()
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
        // #EDE-99: the Chats rows name every synced root.
        .onChange(of: lists.searchRoots, initial: true) { _, roots in
            viewModel?.setChatRoots(roots)
        }
        .onChange(of: viewModel != nil) { _, _ in
            viewModel?.setChatRoots(lists.searchRoots)
        }
        .onChange(of: query) { _, newValue in
            // Debounced + cancelled-on-keystroke inside the VM — never blocks
            // typing.
            viewModel?.queryChanged(newValue)
        }
    }

    /// One result as THE picker row (EXP-922's flat list, SLOP-28's one row
    /// type): the issue's TEAM status glyph (EXP-1554: web's clocks), `IDENT Title` with the identifier
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

    // MARK: - Chats (#EDE-99, search-list.json CHATS)

    /// Which sections show (`globalSections`): no store = today's list.
    private func sections(_ vm: SearchViewModel) -> SearchList.GlobalSections {
        SearchList.globalSections(
            query: query,
            issues: vm.results.count,
            chats: vm.hasChatStore ? (rows: vm.chatRows.count, loading: vm.chatsLoading) : nil
        )
    }

    /// The Chats Section after the issues: THE Section band (count, the
    /// spinner while a newer query runs) over THE picker rows, each body the
    /// title, the snippet with genUI's `highlight` and the muted date.
    private func chatsSection(_ vm: SearchViewModel, shown: SearchList.GlobalSections) -> AnyView? {
        guard let section = shown.sections.first(where: { $0.key == .chats }) else { return nil }
        let preselect = vm.results.isEmpty && vm.suggestion == nil ? vm.chatRows.first?.key : nil
        return AnyView(
            VStack(spacing: GlassPickerTokens.rowSpacing) {
                GlassSectionBand(SearchList.GlobalChats.Copy.chats, count: section.count, loading: section.loading)
                    .padding(.top, vm.results.isEmpty ? 0 : 8)
                    .accessibilityIdentifier(SearchList.GlobalChats.TestIds.chats)
                ForEach(vm.chatRows) { row in
                    GlassPickerSheetRow(
                        item: PickerItem(value: row.key, label: row.title, icon: AppIcons.conceptOrName(row.icon) ?? row.icon),
                        checked: .none,
                        style: GlassPickerTokens.selectionStyle(for: .single),
                        content: AnyView(chatBody(row))
                    ) {
                        pick(row, vm: vm)
                    }
                    .background(
                        row.key == preselect ? GlassTokens.fillActive : Color.clear,
                        in: RoundedRectangle(cornerRadius: GlassPickerTokens.rowRadius)
                    )
                    .accessibilityIdentifier(SearchList.GlobalChats.TestIds.chatRow)
                    .accessibilityAddTraits(row.key == preselect ? .isSelected : [])
                }
            }
        )
    }

    private func chatBody(_ row: SearchList.GlobalChatRow) -> some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(row.title)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                    .lineLimit(1)
                subtitle(row)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Text(row.trailing)
                .font(.subheadline)
                .monospacedDigit()
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
                .fixedSize()
        }
        .frame(minHeight: CGFloat(SearchList.GlobalChats.chatHeightPhone) - 12)
    }

    /// The snippet with every match painted `match` (text-highlight.json, no
    /// current).
    private func subtitle(_ row: SearchList.GlobalChatRow) -> Text {
        guard let marks = FindMarks.title(row.subtitle, mark: FindMark(query: row.highlight, current: nil)) else {
            return Text(row.subtitle)
        }
        return Text(marks.painted(row.subtitle))
    }

    /// A pick opens the hit's session with find in chat on the query, its
    /// start the hit's stop (`globalChatPick`).
    private func pick(_ row: SearchList.GlobalChatRow, vm: SearchViewModel) {
        let pick = SearchList.globalChatPick(query: query, hit: row.hit)
        pushRoute(.chat(accountId: accountId, chatId: row.rootId, child: .find(ChatFindSeed(
            sessionId: pick.sessionId == row.rootId ? nil : pick.sessionId, query: pick.query, target: pick.target
        ))))
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
            title: "Search issues across all your boards.",
            description: Self.emptyDetail,
            icon: AppIcons.navSearch
        )
        .frame(maxHeight: .infinity)
    }
}
