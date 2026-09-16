import ExpUI
import ExpCore
import SwiftUI

/// The searchable list of candidate issues both issue pickers show (EXP-736):
/// "Duplicate of" and the second stage of "Add relation". Loads once, ranks
/// with the shared `IssueSearch` engine (EXP-892 — the ONE algorithm every
/// client runs), and commits immediately on tap — the host owns the sheet
/// chrome (and its pinned search field), this owns the rows.
struct IssueCandidateList: View {
    /// Candidate issues (same team, self excluded), newest first.
    let candidates: [IssueEntity]?
    let searchText: String
    /// The empty-state glyph + copy, so each host keeps its own wording.
    let emptyIcon: String
    let emptyHint: String
    /// EXP-892 — optional server augmentation: a debounced `issues.search` the
    /// host answers, so a query that only matches a COMMENT still finds its
    /// issue. Hits outside the picker's own pool are dropped (a picker never
    /// widens past the candidates it was handed). nil = local-only.
    var serverSearch: ((String) async -> [SearchIssueHit])?
    let onSelect: (IssueEntity) -> Void

    /// Relevance-ordered hits for `hitsQuery`; merged only while the rendered
    /// query still matches, so a stale response never bleeds into a newer
    /// keystroke's rows.
    @State private var hits: [SearchIssueHit] = []
    @State private var hitsQuery = ""

    private var trimmedQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var filtered: [IssueEntity] {
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
        content
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

    @ViewBuilder
    private var content: some View {
        if candidates == nil {
            ProgressView()
                .frame(maxWidth: .infinity)
                .padding(.vertical, 40)
        } else if filtered.isEmpty {
            VStack(spacing: 8) {
                AppIcon(emptyIcon, size: AppIcon.Size.xlarge)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text("No matching issues")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(emptyHint)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 32)
            .padding(.vertical, 32)
        } else {
            // A `ScrollView` of rows, not a `List`: the chrome measures its
            // content, and a List reports an unbounded height (EXP-687).
            LazyVStack(spacing: 2) {
                ForEach(Array(filtered.enumerated()), id: \.element.id) { index, issue in
                    Button {
                        onSelect(issue)
                    } label: {
                        HStack(spacing: 10) {
                            AppIcon(IssueStatus.from(issue.status).iconName, size: AppIcon.Size.small)
                                .foregroundStyle(IssueStatus.from(issue.status).color)
                                .frame(width: 24)
                            if let identifier = issue.identifier {
                                Text(identifier)
                                    .font(.caption.monospaced())
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }
                            Text(issue.title)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 14)
                        .frame(minHeight: 44)
                        // EXP-892: while a query is being typed the best match
                        // — the top row — reads as selected, the same contract
                        // the `#` menu keeps.
                        .background(
                            index == 0 && !trimmedQuery.isEmpty
                                ? Color.white.opacity(GlassMenuTokens.activeFillOpacity)
                                : Color.clear,
                            in: RoundedRectangle(cornerRadius: GlassMenuTokens.activeFillRadius)
                        )
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 6)
            .padding(.bottom, 16)
        }
    }
}
