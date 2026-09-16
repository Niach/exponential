import Foundation

/// EXP-916 — the ONE "edited files" card a transcript draws for a run of
/// consecutive file edits, and the ONE rule that decides which tool calls form
/// it. Hand-mirrored ×4 (TS `@exp/domain-contract` `edit-card.ts` + web
/// `@exp/ui` `EditedFilesCard`, desktop `steer::feed` + `domain::edit_card`,
/// Android `domain/EditCard.kt`, iOS here) and byte-locked by
/// `packages/domain-contract/fixtures/feed/edit-cards.json`, which every
/// client's feed test replays through its own row projection
/// (`AgentFeed.rows` / `AgentFeed.laneRows` here).
///
/// Grouping (a render ROW of the feed projection, `AgentFeedRow.edits`):
/// - an `edits` row is a MAXIMAL run of consecutive feed items of ONE lane
///   (`subagentId`, nil = the main lane) that are all edit calls: a `tool` with
///   a `toolKind` ∈ `edit|delete|move` that is NOT a workflow card (EXP-850 §3
///   keeps a workflow call as its own row — on iOS a workflow call is named by
///   its `callId` in the projection's `workflowIds`, there is no field);
/// - the rule reads ONLY the kind, the tool kind and the lane — never
///   `settled`/`failed`/`diff` — so the card exists before any patch lands and
///   a later `tool_update` can never re-split it;
/// - ANY other item ends the run: narration, a user turn, a question, a tool of
///   another kind, a workflow call, another lane's item. "Nothing between" is
///   literal;
/// - the row's id is its FIRST item's id (stable while a live run grows); the
///   window `start` (EXP-783) opens a fresh card at its boundary like every
///   other group.
///
/// The card (`EditCard.card`):
/// - one row per PATH: every member's patch is parsed and folded with
///   `Diff.mergeFilesByPath` (a file touched twice = one row, counts summed,
///   hunks concatenated); a pathless patch (bare hunks) borrows its call's
///   `detail`;
/// - a member WITHOUT a patch still has a row on its `detail` — `pending`
///   while the call runs, `failed` once it settled without one — unless a
///   patch for that path already exists in the card;
/// - order: the ready rows in first-touch order, THEN the pending/failed
///   stubs in first-touch order;
/// - `liveIndex` names the row of the card's LAST member when that member is
///   the transcript's live tool row (`AgentFeed.liveToolRowId`): the one row a
///   client opens by itself, the diff inline. Everything else starts collapsed,
///   and a tap toggles a row in place — a card never navigates anywhere.
public enum EditCard {
    /// The tool kinds whose calls form an edited-files card.
    public static let kinds: [String] = ["edit", "delete", "move"]

    /// How many rows a card lists before it folds the rest behind "N more".
    public static let preview: Int = DomainContract.diffUiCardPreviewFiles

    public enum RowState: String, Equatable, Sendable {
        case ready
        case pending
        case failed
    }

    public struct Row: Equatable, Sendable, Identifiable {
        public let path: String
        public let state: RowState
        /// The merged patch for the path; nil for a `pending`/`failed` row.
        public let file: Diff.File?

        public var id: String { path }

        public init(path: String, state: RowState, file: Diff.File?) {
            self.path = path
            self.state = state
            self.file = file
        }
    }

    public struct View: Equatable, Sendable {
        /// `1 file edited` / `N files edited`.
        public let title: String
        public let rows: [Row]
        /// The row the client opens by itself, or nil.
        public let liveIndex: Int?

        public init(title: String, rows: [Row], liveIndex: Int?) {
            self.title = title
            self.rows = rows
            self.liveIndex = liveIndex
        }
    }

    /// Whether a feed item is a call that belongs in an edited-files card.
    /// `workflowIds` names the calls that ARE workflow cards (EXP-850).
    public static func isEditCall(
        _ item: AgentFeedItem, workflowIds: Set<String> = []
    ) -> Bool {
        guard case let .tool(_, _, _, _, callId, toolKind, _, _, _, _, _) = item,
              let toolKind, kinds.contains(toolKind)
        else { return false }
        if !workflowIds.isEmpty, let callId, workflowIds.contains(callId) { return false }
        return true
    }

    /// The inclusive end index of the maximal run of same-lane edit calls that
    /// starts at `start` (which must itself be an edit call). A caller's group
    /// scan uses it exactly like its tool-run scan.
    public static func runEnd(
        _ feed: [AgentFeedItem], start: Int, workflowIds: Set<String> = []
    ) -> Int {
        guard feed.indices.contains(start) else { return start }
        let lane = feed[start].subagentKey
        var end = start
        while end + 1 < feed.count,
              isEditCall(feed[end + 1], workflowIds: workflowIds),
              feed[end + 1].subagentKey == lane {
            end += 1
        }
        return end
    }

    /// The card one run of edit calls draws.
    public static func card(
        _ items: [AgentFeedItem], liveItemId: Int? = nil
    ) -> View {
        var ready: [Diff.File] = []
        // An ORDERED map: first touch wins the position, a later settle may
        // still flip the state.
        var stubOrder: [String] = []
        var stubs: [String: RowState] = [:]
        for item in items {
            guard case let .tool(_, _, detail, _, _, _, settled, _, diff, _, _) = item
            else { continue }
            let subject = detail?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if let diff, !diff.isEmpty {
                for file in Diff.parse(diff).files {
                    // A pathless section (hunks with no header) borrows the
                    // call's own subject — the engine names the file in
                    // `detail`.
                    let path = file.path.isEmpty ? subject : file.path
                    if path.isEmpty { continue }
                    if file.path == path {
                        ready.append(file)
                    } else {
                        var renamed = file
                        renamed.path = path
                        ready.append(renamed)
                    }
                }
                continue
            }
            if subject.isEmpty { continue }
            let state: RowState = settled ? .failed : .pending
            if stubs[subject] == nil {
                stubOrder.append(subject)
                stubs[subject] = state
            } else if state == .failed, stubs[subject] == .pending {
                stubs[subject] = .failed
            }
        }
        let merged = Diff.mergeFilesByPath(ready)
        let readyPaths = Set(merged.map(\.path))
        var rows = merged.map { Row(path: $0.path, state: .ready, file: $0) }
        for path in stubOrder where !readyPaths.contains(path) {
            rows.append(Row(path: path, state: stubs[path] ?? .pending, file: nil))
        }
        var liveIndex: Int?
        if let last = items.last, let liveItemId, liveItemId == last.id,
           let path = itemPath(last),
           let at = rows.firstIndex(where: { $0.path == path }) {
            liveIndex = at
        }
        return View(title: title(rows.count), rows: rows, liveIndex: liveIndex)
    }

    /// The path a member names: its patch's first file, else its `detail`.
    private static func itemPath(_ item: AgentFeedItem) -> String? {
        guard case let .tool(_, _, detail, _, _, _, _, _, diff, _, _) = item
        else { return nil }
        if let diff, !diff.isEmpty {
            let first = Diff.parse(diff).files.first
            if let path = first?.path, !path.isEmpty { return path }
        }
        let subject = detail?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return subject.isEmpty ? nil : subject
    }

    /// The card's title — `1 file edited` / `4 files edited`, ×4.
    public static func title(_ count: Int) -> String {
        count == 1
            ? DomainContract.diffUiEditedFilesOne
            : DomainContract.diffUiEditedFilesMany
                .replacingOccurrences(of: "{n}", with: String(count))
    }

    /// The fold row under the first `preview` rows, or nil.
    public static func moreLabel(_ count: Int) -> String? {
        let rest = count - preview
        guard rest > 0 else { return nil }
        return DomainContract.diffUiMoreFiles
            .replacingOccurrences(of: "{n}", with: String(rest))
    }

    /// The byte-lock projection of a card: `title | path +a -d | path pending |
    /// path failed | live=path`. Deliberately ASCII (`-d`, unlike
    /// `Diff.deletionsLabel`), like `Diff.render`.
    public static func render(_ view: View) -> String {
        var parts = [view.title]
        for row in view.rows {
            if row.state == .ready, let file = row.file {
                parts.append("\(row.path) +\(file.additions) -\(file.deletions)")
            } else {
                parts.append("\(row.path) \(row.state.rawValue)")
            }
        }
        if let liveIndex = view.liveIndex, view.rows.indices.contains(liveIndex) {
            parts.append("live=\(view.rows[liveIndex].path)")
        }
        return parts.joined(separator: " | ")
    }
}
