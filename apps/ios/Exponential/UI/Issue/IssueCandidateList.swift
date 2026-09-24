import ExpUI
import ExpCore
import SwiftUI

/// The searchable issue picker both linkers show (EXP-736): "Duplicate of" and
/// the second stage of "Add relation". EXP-1021 made it the SHARED
/// `IssuePicker` — plain rows on the one sheet, each led by the issue's status
/// glyph, the filter field pinned at the top — so the linker that was the
/// reference for every other picker now literally is the same component.
///
/// What stays here is the RANKING, which the picker never owns: the pool is
/// ordered by the ONE search engine (EXP-892 `IssueSearch.rank`) and merged
/// with the host's debounced `issues.search`, so a query that only matches a
/// COMMENT still finds its issue. Hits outside the pool are dropped — a picker
/// never widens past the candidates it was handed.
struct IssueCandidatePicker: View {
    /// Candidate issues (same team, self excluded), newest first. nil = still
    /// loading, which the picker draws as its own "loading" row.
    let candidates: [IssueEntity]?
    /// The sheet headline. It is the ONLY thing naming the link being made —
    /// "Duplicate of", or which of the six relations the linker's first stage
    /// picked — so every caller says it (EXP-1021 review r2; Android's
    /// linkers pass the same).
    let title: String
    /// Host-driven, because both entry points are a menu item or a stacked
    /// sheet rather than a chip the picker could wrap.
    let open: Binding<Bool>
    var onDismiss: (() -> Void)?
    /// EXP-892 — optional server augmentation the host answers. nil =
    /// local-only.
    var serverSearch: ((String) async -> [SearchIssueHit])?
    let onSelect: (IssueEntity) -> Void

    @State private var searchText = ""
    /// Relevance-ordered hits for `hitsQuery`; merged only while the rendered
    /// query still matches, so a stale response never bleeds into a newer
    /// keystroke's rows.
    @State private var hits: [SearchIssueHit] = []
    @State private var hitsQuery = ""

    private var trimmedQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var ranked: [IssueEntity] {
        guard let candidates else { return [] }
        let local = IssueSearch.rank(
            candidates,
            query: trimmedQuery,
            limit: Int.max,
            projection: \.searchRow
        )
        guard !trimmedQuery.isEmpty, hitsQuery == trimmedQuery, !hits.isEmpty else { return local }
        let byId = Dictionary(candidates.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return IssueSearch.mergeServerHits(
            local: local,
            hits: hits,
            limit: Int.max,
            localId: \.id,
            hitId: \.id,
            resolve: { byId[$0.id] }
        )
    }

    var body: some View {
        let rows = ranked
        IssuePicker(
            issues: rows.map(IssuePickerIssue.init),
            // EXP-892: while a query is being typed the best match — the top
            // row — reads as selected, the same contract the `#` menu keeps.
            // The picker's own highlight is what draws it (EXP-1021).
            value: trimmedQuery.isEmpty ? [] : Set(rows.prefix(1).map(\.id)),
            onChange: { picked in
                guard let issue = rows.first(where: { picked.contains($0.id) }) else { return }
                onSelect(issue)
            },
            // The caller ranks, so the picker renders the order verbatim and
            // only reports what was typed.
            query: $searchText,
            loading: candidates == nil,
            title: title,
            // Both linkers filter a pool, so "nothing left" is always a
            // SEARCH answer, never "this team has no issues".
            emptyText: "No matching issues",
            open: open,
            hideTrigger: true,
            onDismiss: onDismiss,
            trigger: { EmptyView() }
        )
        // The picker is HOST-driven and permanently mounted (both entry
        // points hang it on a `.background`), so nothing unmounts this state
        // between openings — a query typed into one opening would still be
        // filtering the next. Cleared on the OPENING edge, not the closing
        // one: re-filtering while the sheet animates away would flash the
        // whole pool back in behind it.
        .onChange(of: open.wrappedValue) { _, isOpen in
            guard isOpen else { return }
            searchText = ""
            hits = []
            hitsQuery = ""
        }
        // Debounced server augmentation: `.task(id:)` cancels the previous
        // sleep on every keystroke, so only a settled query round-trips.
        .task(id: trimmedQuery) {
            guard let serverSearch else { return }
            let query = trimmedQuery
            guard !query.isEmpty else {
                hits = []
                hitsQuery = ""
                return
            }
            try? await Task.sleep(nanoseconds: 250_000_000)
            guard !Task.isCancelled else { return }
            let found = await serverSearch(query)
            guard !Task.isCancelled else { return }
            hits = found
            hitsQuery = query
        }
    }
}
