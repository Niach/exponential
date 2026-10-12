import ExpCore
import ExpUI
import SwiftUI

/// SLOP-28 (EXP-1259 D4, search-list.json): typing `#` in any editor or
/// composer opens THE search list as a BOTTOM SHEET (our sheet chrome, never a
/// system popover) with its input FOCUSED and prefilled with the token typed
/// after the `#`; spaces are allowed in the input. Results = the shared issue
/// engine at its DEFAULT limit (no 6/8 cap): the editor's local ranking at
/// once, then the debounced server splice (`IssueEditorModel
/// .issueRefRemoteSearch`). A pick replaces `#token` with `#IDENT ` and hands
/// focus back to the editor; a swipe down closes and leaves the text as typed;
/// Backspace in the EMPTY input closes and deletes the `#`.
///
/// Mounted by the two editor views (`MarkdownEditor`, `MarkdownComposerField`)
/// on every editable editor, so each host gets it without wiring its own.
struct IssueRefSearchSheetModifier: ViewModifier {
    let model: IssueEditorModel

    /// The request on screen — a local mirror, so a swipe-down's `nil` write
    /// lands here while the model's request waits for `onDismiss`.
    @State private var presented: IssueRefSearchRequest?
    @State private var pick: IssueRefCandidate?
    @State private var deleteHash = false

    func body(content: Content) -> some View {
        content
            .onAppear { model.issueRefOpensSheet = true }
            .onChange(of: model.issueRefSearchRequest) { _, request in
                if let request { presented = request }
            }
            .sheet(item: $presented, onDismiss: finish) { request in
                IssueRefSearchSheet(
                    initialQuery: request.query,
                    local: model.issueRefSearch,
                    remote: model.issueRefRemoteSearch,
                    onPick: { candidate in
                        pick = candidate
                        presented = nil
                    },
                    onBackspaceEmpty: {
                        deleteHash = true
                        presented = nil
                    }
                )
            }
    }

    /// The editor settles AFTER the sheet is gone, so the first responder it
    /// asks for is not fighting the sheet's own dismissal.
    private func finish() {
        if let pick {
            model.completeIssueRefSearch(pick)
        } else {
            model.dismissIssueRefSearch(deletingHash: deleteHash)
        }
        pick = nil
        deleteHash = false
    }
}

extension View {
    /// SLOP-28: the `#` typeahead's search sheet for `model`'s editor.
    func issueRefSearchSheet(_ model: IssueEditorModel) -> some View {
        modifier(IssueRefSearchSheetModifier(model: model))
    }
}

/// The sheet's body: the search list over the editor's issue scope.
struct IssueRefSearchSheet: View {
    let initialQuery: String
    let local: ((String) -> [IssueRefCandidate])?
    let remote: ((String) async -> [IssueRefCandidate])?
    let onPick: (IssueRefCandidate) -> Void
    let onBackspaceEmpty: () -> Void

    @State private var query: String
    @State private var results: [IssueRefCandidate] = []

    init(
        initialQuery: String,
        local: ((String) -> [IssueRefCandidate])?,
        remote: ((String) async -> [IssueRefCandidate])?,
        onPick: @escaping (IssueRefCandidate) -> Void,
        onBackspaceEmpty: @escaping () -> Void
    ) {
        self.initialQuery = initialQuery
        self.local = local
        self.remote = remote
        self.onPick = onPick
        self.onBackspaceEmpty = onBackspaceEmpty
        _query = State(initialValue: initialQuery)
    }

    var body: some View {
        SearchListSheet(
            SearchListView(
                items: results.map(Self.item),
                query: $query,
                placeholder: IssueSearchCopy.placeholder,
                autofocus: true,
                emptyText: IssueSearchCopy.noMatches,
                onPick: { identifier in
                    if let candidate = results.first(where: { $0.identifier == identifier }) {
                        onPick(candidate)
                    }
                },
                onBackspaceEmpty: onBackspaceEmpty
            )
        )
        .accessibilityIdentifier("issue-ref-search-sheet")
        .task(id: query) { await search(query) }
    }

    /// Local first (instant), then the server splice once the keystrokes
    /// settle — applied only while the query it answered is still typed.
    private func search(_ query: String) async {
        results = local?(query) ?? []
        guard let remote, !IssueSearch.normalizeQuery(query).isEmpty else { return }
        try? await Task.sleep(nanoseconds: 250_000_000)
        guard !Task.isCancelled else { return }
        let merged = await remote(query)
        guard !Task.isCancelled, query == self.query, !merged.isEmpty else { return }
        results = merged
    }

    /// The picker row (status glyph, `IDENT Title`), keyed by identifier.
    static func item(_ candidate: IssueRefCandidate) -> PickerItem<String> {
        PickerItem(
            value: candidate.identifier,
            label: "\(candidate.identifier) \(candidate.title)",
            icon: candidate.status?.iconName,
            color: candidate.status?.color,
            keywords: [candidate.identifier, candidate.title]
        )
    }
}

/// The issue search's ×4 copy (EXP-922: web `lib/issue-search.ts`
/// `ISSUE_SEARCH_PLACEHOLDER`).
enum IssueSearchCopy {
    static let placeholder = "Search issues"
    static let noMatches = "No matching issues"
}
