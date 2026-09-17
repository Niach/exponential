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
///   while the call runs, `failed` when the call's OWN `failed` flag is set,
///   `done` once it settled without either (a delete, a move, an edit that
///   changed nothing: the wire carries no patch for those) — unless a patch
///   for that path already exists in the card;
/// - order: the ready rows in first-touch order, THEN the stubs in
///   first-touch order; a later settle may upgrade a stub (pending → done /
///   failed) but never moves it;
/// - `truncatedLines` = the lines the publisher cut off the members' patches
///   (EXP-786 markers), summed, so the card can say what it is not showing;
/// - `liveIndex` names the row of the card's LAST member when that member is
///   the transcript's live tool row (`AgentFeed.liveToolRowId`): the one row a
///   client opens by itself, the diff inline. Everything else starts collapsed,
///   and a tap toggles a row in place — a card never navigates anywhere.
public enum EditCard {
    /// The tool kinds whose calls form an edited-files card — the contract's
    /// `toolKind.editKinds`, generated ×4 so no mirror restates the list.
    public static let kinds: [String] = DomainContract.toolKindEditValues

    /// How many rows a card lists before it folds the rest behind "N more".
    public static let preview: Int = DomainContract.diffUiCardPreviewFiles

    public enum RowState: String, Equatable, Sendable {
        case ready
        case pending
        case done
        case failed
    }

    public struct Row: Equatable, Sendable, Identifiable {
        public let path: String
        public let state: RowState
        /// The merged patch for the path; nil for a `pending`/`done`/`failed`
        /// row.
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
        /// Lines the publisher cut off the members' patches, summed (0 = the
        /// card shows the whole thing).
        public let truncatedLines: Int

        public init(
            title: String, rows: [Row], liveIndex: Int?, truncatedLines: Int = 0
        ) {
            self.title = title
            self.rows = rows
            self.liveIndex = liveIndex
            self.truncatedLines = truncatedLines
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
        var truncatedLines = 0
        // The path the LAST member names — its patch's first file, else its
        // `detail` — recorded while its patch is parsed once, never re-parsed.
        var lastPath: String?
        for (index, item) in items.enumerated() {
            guard case let .tool(_, _, detail, _, _, _, settled, failed, diff, _, _) = item
            else { continue }
            let isLast = index == items.count - 1
            let subject = detail?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if let diff, !diff.isEmpty {
                let parsed = Diff.parse(diff)
                truncatedLines += parsed.truncatedLines ?? 0
                for file in parsed.files {
                    // A pathless section (hunks with no header) borrows the
                    // call's own subject — the engine names the file in
                    // `detail`.
                    let path = file.path.isEmpty ? subject : file.path
                    if path.isEmpty { continue }
                    if isLast, lastPath == nil { lastPath = path }
                    if file.path == path {
                        ready.append(file)
                    } else {
                        var renamed = file
                        renamed.path = path
                        ready.append(renamed)
                    }
                }
                if isLast, lastPath == nil { lastPath = subject.isEmpty ? nil : subject }
                continue
            }
            if isLast { lastPath = subject.isEmpty ? nil : subject }
            if subject.isEmpty { continue }
            let state: RowState = failed ? .failed : (settled ? .done : .pending)
            // First touch wins the position; a later settle upgrades a pending
            // stub (pending → done / failed) and never demotes a settled one.
            let held = stubs[subject]
            if held == nil {
                stubOrder.append(subject)
                stubs[subject] = state
            } else if held == .pending {
                stubs[subject] = state
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
           let path = lastPath,
           let at = rows.firstIndex(where: { $0.path == path }) {
            liveIndex = at
        }
        return View(
            title: title(rows.count), rows: rows, liveIndex: liveIndex,
            truncatedLines: truncatedLines
        )
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
    /// path done | path failed | live=path | truncated=N`. Deliberately ASCII
    /// (`-d`, unlike `Diff.deletionsLabel`), like `Diff.render`.
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
        if view.truncatedLines > 0 { parts.append("truncated=\(view.truncatedLines)") }
        return parts.joined(separator: " | ")
    }
}
