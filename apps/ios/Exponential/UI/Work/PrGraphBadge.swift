import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-897 Part 4 — the ONE badge that says a piece of work is entangled with
/// other work, and the ONE overlay behind it, ×4 (web `pr-graph-badge.tsx`,
/// desktop `pr_graph.rs`, Android `PrGraphBadge.kt`).
///
/// SLOP-16 r2: the badge is a quiet ICON BUTTON in the header's `…` style
/// (same glyph size, weight and muted ink, the bar's own capsule), not the
/// stacked issue chip that repeated the title. Its glyph follows the badge
/// SHAPE (`PrGraph.badgeShape`: a stack or batch, else a run family, else
/// open blockers), a small mono `+N` beside it; the tap opens the overlay,
/// the "Related work" sheet (`PrGraphSheet`, SLOP-16 r3): every relation
/// the subject has, the face's own FIRST (`PrGraph.overlaySections`), each a
/// group band over flat rows the product already draws — the same layout and
/// copy on every face and every platform.
struct PrGraphBadge: View {
    let shape: PrGraph.BadgeShape?
    /// How many other pieces of work ride with the subject (`chip.count`).
    let count: Int
    let accessibilityName: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 2) {
                AppIcon(Self.icon(shape), size: AppIcon.Size.medium, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: GlassTokens.controlSize, height: GlassTokens.controlSize)
                if let countSuffix {
                    Text(countSuffix)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.trailing, 6)
                        .fixedSize()
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityName)
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("pr-graph-badge")
    }

    /// `+3` — who rides with the subject; nil when nobody does.
    private var countSuffix: String? {
        count > 0 ? "+\(count)" : nil
    }

    /// The badge's glyph, by what it stands for.
    static func icon(_ shape: PrGraph.BadgeShape?) -> String {
        switch shape {
        case .stack, .stackAndBatch: AppIcons.prStack
        case .batch: AppIcons.prBatch
        case .blocked: AppIcons.relationBlockedBy
        case .runs, nil: AppIcons.sessionTree
        }
    }

    /// The badge's spoken name, by what it stands for.
    static func accessibilityName(_ shape: PrGraph.BadgeShape?) -> String {
        switch shape {
        case .stack: "Pull request stack"
        case .batch: "Batch pull request"
        case .stackAndBatch: "Stack and batch"
        case .blocked: IssueRelationsView.Copy.blockedBy
        case .runs, nil: "Related runs"
        }
    }
}

// MARK: - The overlay

/// SLOP-16 r3: THE "Related work" view, one layout ×4 — the platform's
/// standard sheet, then per `PrGraph.overlaySections` section ONE group band
/// (`GlassSectionBand`) over FLAT hairline-divided rows, nothing else. Every
/// row is a row the product already draws: the relations card's issue row,
/// the Reviews PR row content, the running list's run row, and the compact
/// blocks mini-graph under "Blocked by". A face changes WHAT leads, never how.
struct PrGraphSheet: View {
    let graph: PrGraph.Graph
    let face: WorkFaceKind
    /// EXP-930: every issue row the screen has synced — what gives a tree row
    /// its own name (an issue run's title, a batch run's `EXP-874 +2`) instead
    /// of the "Untitled issue" every issue-less row used to read.
    var issues: [IssueEntity] = []
    /// EXP-980: the transitive blocks graph around the subject issue — what
    /// the blocked section draws. Empty (or a lone subject node) = the direct
    /// blockers as issue rows.
    var blockGraph = IssueGraph.Graph(nodes: [], edges: [], hasCycle: false, truncated: false)
    /// The subject issue — the Issue face's batch section lists its PARTNERS.
    var subjectIssueId: String?
    /// The issue rows' assignee avatars and team-resolved status glyphs.
    var users: [UserEntity] = []
    var teamStatuses: [ResolvedIssueStatus] = []
    let onOpenIssue: (String) -> Void
    let onOpenRun: (String) -> Void
    /// A PR row: its Changes face / review.
    let onOpenPullRequest: (PrGraph.Entry) -> Void
    let onMergeStack: (String) -> Void

    /// EXP-1097: every relation the subject HAS gets its section on every
    /// face; the face only decides which one LEADS (`overlaySections`).
    /// A section with no rows to draw is dropped here, so a sheet left with
    /// nothing still shows the empty note (the Issue face's batch minus the
    /// subject is empty while its partners have not synced).
    private var sections: [PrGraph.OverlaySection] {
        PrGraph.overlaySections(graph, face: face).filter { section in
            switch section {
            case .batch: !batchRows.isEmpty
            case .stack: !stackEntries.isEmpty
            case .blocked, .runs: true
            }
        }
    }

    var body: some View {
        // Content-sized (≤85 %, the chrome scrolls past it): the sheet is as
        // tall as what it lists, never a mostly empty full-height page.
        GlassSheetChrome(title: PrGraph.OverlayCopy.relatedWorkTitle, height: .fitted) {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(sections, id: \.self) { section in
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(PrGraph.overlayBandTitle(section, face: face))
                        switch section {
                        case .blocked: blockedRows
                        case .batch: flatRows(batchRows.map { RelatedRow.issue($0, depth: 0) })
                        case .runs: flatRows(graph.tree.map { RelatedRow.run($0.session, depth: $0.depth) })
                        case .stack: flatRows(stackRows)
                        }
                    }
                }
                if sections.isEmpty {
                    RelatedWorkEmptyRow(text: PrGraph.OverlayCopy.empty)
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .accessibilityIdentifier("pr-graph-sheet")
    }

    // MARK: Sections

    /// EXP-980: the transitive blocks chain as THE compact mini-graph (it
    /// scrolls sideways on its own), the direct blockers as issue rows when
    /// the graph has not resolved.
    @ViewBuilder
    private var blockedRows: some View {
        if blockGraph.nodes.count > 1 {
            IssueGraphView(
                graph: blockGraph, issues: issues, density: .compact, onOpenIssue: onOpenIssue
            )
        } else {
            flatRows(graph.blockers.map { RelatedRow.issue($0, depth: 0) })
        }
    }

    /// On the Issue face the subject is the reader's own issue, so the batch
    /// lists its PARTNERS; on a run the covered set IS the run's subject
    /// (EXP-930: the whole set, not "everything but me").
    private var batchRows: [IssueEntity] {
        (graph.batch?.issues ?? []).filter { row in
            face != .issue || row.id != subjectIssueId
        }
    }

    /// The pull requests bottom-up: the stack, else the subject's lone PR
    /// (the Changes face leads with it — ONE PR row, never a note).
    private var stackEntries: [(entry: PrGraph.Entry, depth: Int)] {
        if !graph.stack.isEmpty { return graph.stack.map { ($0.entry, $0.depth) } }
        return graph.entry.map { [($0, 0)] } ?? []
    }

    /// The PR rows, a batch PR's issues folded one level under it — unless
    /// the batch section is drawn, which already lists them (no duplicates).
    private var stackRows: [RelatedRow] {
        let foldBatch = !sections.contains(.batch)
        return stackEntries.flatMap { rung -> [RelatedRow] in
            let pr = RelatedRow.pullRequest(rung.entry, depth: rung.depth)
            guard foldBatch, rung.entry.isBatch else { return [pr] }
            return [pr] + rung.entry.issues.map { RelatedRow.issue($0, depth: rung.depth + 1) }
        }
    }

    // MARK: Rows

    /// A section's rows: flat, hairline-divided, nested by `treeGuides`.
    @ViewBuilder
    private func flatRows(_ rows: [RelatedRow]) -> some View {
        let guides = TreeGuides.compute(depths: rows.map(\.depth))
        ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
            if index > 0 { GlassDivider() }
            rowView(row)
                .treeGuides(guides[index], gap: GlassTokens.hairline)
        }
    }

    @ViewBuilder
    private func rowView(_ row: RelatedRow) -> some View {
        switch row {
        case let .issue(issue, _):
            RelatedIssueRow(
                issue: issue, users: users, teamStatuses: teamStatuses,
                onOpen: { onOpenIssue(issue.id) }
            )
        case let .run(session, _):
            runRow(session)
        case let .pullRequest(entry, depth):
            pullRequestRow(entry, depth: depth)
        }
    }

    /// The running list's run row (`RunningSessionRow`), opening the run.
    @ViewBuilder
    private func runRow(_ session: CodingSessionEntity) -> some View {
        let issue = session.issueId.flatMap { id in issues.first { $0.id == id } }
        RunningSessionRow(
            session: session,
            // An issue run's id, a batch's `EXP-874 +2` (EXP-876).
            identifier: sessionRowIdentifier(issue: issue, session: session, batchIssues: issues),
            title: sessionRowTitle(issue: issue, session: session, batchIssues: issues),
            state: CodingSessionDisplayState.of(
                session: session, prState: issue?.prState ?? session.prState
            ),
            device: SessionDevicePresentation.resolve(session: session, devices: []),
            open: .action { onOpenRun(session.id) }
        )
    }

    /// The Reviews PR row content on a flat row: the PR state, and "Merge
    /// stack" on the BOTTOM row of a real stack. Tapping opens its review.
    private func pullRequestRow(_ entry: PrGraph.Entry, depth: Int) -> some View {
        Button { onOpenPullRequest(entry) } label: {
            PrReviewRowContent(issues: entry.issues) {
                if let state = entry.prState, !state.isEmpty {
                    Text(state)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
                if depth == 0, graph.stack.count > 1 {
                    // Not a Button: nested in the row's label its tap would
                    // be swallowed (the Reviews rows' pattern).
                    GlassPill(PrGraph.OverlayCopy.mergeStack, icon: AppIcons.prStack)
                        .contentShape(Capsule())
                        .onTapGesture { onMergeStack(entry.representative.id) }
                        .accessibilityAddTraits(.isButton)
                        .accessibilityLabel("Merge the whole stack")
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .flatRow()
    }
}

/// One row of the Related work sheet, with the depth its tree guide nests it at.
private enum RelatedRow: Identifiable {
    case issue(IssueEntity, depth: Int)
    case run(CodingSessionEntity, depth: Int)
    case pullRequest(PrGraph.Entry, depth: Int)

    var id: String {
        switch self {
        case let .issue(issue, depth): "issue:\(issue.id):\(depth)"
        case let .run(session, _): "run:\(session.id)"
        case let .pullRequest(entry, _): "pr:\(entry.id)"
        }
    }

    var depth: Int {
        switch self {
        case let .issue(_, depth), let .run(_, depth), let .pullRequest(_, depth): depth
        }
    }
}

/// THE relation row (`IssueRelationListRow`) fed a synced issue: status
/// glyph · mono identifier · title · assignee, tap opens it, nothing to
/// unlink from here.
struct RelatedIssueRow: View {
    let issue: IssueEntity
    var users: [UserEntity] = []
    var teamStatuses: [ResolvedIssueStatus] = []
    let onOpen: () -> Void

    var body: some View {
        IssueRelationListRow(
            row: IssueRelationsView.row(
                IssueRelationsView.Issue(
                    id: issue.id,
                    identifier: issue.identifier ?? "",
                    title: issue.title,
                    status: issue.status
                )
            ),
            status: teamStatuses.isEmpty
                ? nil : IssueStatusResolver.resolve(issue, team: teamStatuses),
            assignee: issue.assigneeId.flatMap { id in users.first { $0.id == id } },
            assigneeId: issue.assigneeId,
            removeLabel: "",
            onOpen: onOpen,
            onRemove: nil
        )
    }
}

/// The sheet's ONE empty note, on a flat row like the running list's.
private struct RelatedWorkEmptyRow: View {
    let text: String

    var body: some View {
        HStack(spacing: 8) {
            Text(text)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
    }
}

/// EXP-897 Part 4: the plain issue list a batch row's glyph opens (Reviews) —
/// the same flat issue rows as the Related work sheet.
struct PrGraphIssueSheet: View {
    let title: String
    let issues: [IssueEntity]
    var users: [UserEntity] = []
    let onOpenIssue: (String) -> Void

    var body: some View {
        GlassSheetChrome(title: title, height: .fitted) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(issues.enumerated()), id: \.element.id) { index, issue in
                    if index > 0 { GlassDivider() }
                    RelatedIssueRow(issue: issue, users: users) { onOpenIssue(issue.id) }
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .accessibilityIdentifier("pr-graph-issue-sheet")
    }
}

// MARK: - The rows the graph needs

/// EXP-897 Part 4: the synced inputs of `PrGraph.build` — every pull request
/// in the store (the stack and the batches), this issue's blockers, and the
/// runs of the shown session's team (the tree). All three are narrow SQL
/// reads; the pure model does the rest.
@MainActor @Observable
final class PrGraphModel {
    private(set) var prIssues: [IssueEntity] = []
    private(set) var blockers: [IssueEntity] = []
    private(set) var relations: [IssueRelationEntity] = []
    private(set) var sessions: [CodingSessionEntity] = []
    /// The issues the team's runs are bound to — a tree row's issue with no
    /// pull request yet (and no blocker tie) is named off these rather than
    /// falling through to "Untitled issue".
    private(set) var sessionIssues: [IssueEntity] = []
    /// SLOP-16 r3: every synced member — the overlay's issue rows (THE
    /// relation row) wear their assignee's avatar.
    private(set) var users: [UserEntity] = []

    private let accountId: String
    private let db: DatabaseManager
    private var issueId: String?
    private var teamId: String?

    private var prTask: Task<Void, Never>?
    private var blockerTask: Task<Void, Never>?
    private var sessionTask: Task<Void, Never>?
    private var userTask: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager) {
        self.accountId = accountId
        self.db = db
    }

    /// A synced issue row this model already holds (a pull request, or a
    /// blocker) — the run screen's stack line resolves its own issue here
    /// instead of opening a fourth observation.
    func issue(id: String) -> IssueEntity? {
        prIssues.first { $0.id == id } ?? blockers.first { $0.id == id }
            ?? sessionIssues.first { $0.id == id }
    }

    /// EXP-930: every issue row this model holds — the overlay names its tree
    /// rows from it, the same pool `graph(...)` builds its answer on.
    var knownIssues: [IssueEntity] { prIssues + blockers + sessionIssues }

    /// EXP-980: the transitive blocks graph around `issue`, over the same
    /// synced pool the Issue face names its nodes from.
    func blockGraph(issue: IssueEntity?, pool: [IssueEntity]) -> IssueGraph.Graph {
        guard let issue else {
            return IssueGraph.Graph(nodes: [], edges: [], hasCycle: false, truncated: false)
        }
        return IssueGraph.blockGraph(
            subjectIds: [issue.id], relations: relations, issues: pool
        )
    }

    /// The graph for a subject, ready for the badge and the overlay.
    ///
    /// EXP-876: `batchIssues` are the covered issues of a BATCH run (the Work
    /// screen's `WorkSubjectModel` already observes them to name the run), so
    /// the pill and its sheet work before any pull request exists — this
    /// model's own reads are pull requests and blockers, which a batch that is
    /// still coding is neither.
    func graph(
        issue: IssueEntity?,
        session: CodingSessionEntity?,
        batchIssues: [IssueEntity] = []
    ) -> PrGraph.Graph {
        var byId: [String: IssueEntity] = [:]
        for row in prIssues + blockers + sessionIssues { byId[row.id] = row }
        if let issue { byId[issue.id] = issue }
        // Deterministic order: the PR rows first (they carry the stack), then
        // anything only a blocker brought in, then the runs' own issues.
        var pool: [IssueEntity] = []
        var seen = Set<String>()
        for row in prIssues + blockers + sessionIssues where !seen.contains(row.id) {
            seen.insert(row.id)
            pool.append(byId[row.id] ?? row)
        }
        if let issue, !seen.contains(issue.id) { pool.append(issue) }
        for row in batchIssues where !seen.contains(row.id) {
            seen.insert(row.id)
            pool.append(row)
        }
        return PrGraph.build(
            issue: issue, session: session, issues: pool,
            sessions: sessions, relations: relations
        )
    }

    /// Re-armed on every appear; the subject's issue and team may resolve late.
    func start(issueId: String?, teamId: String?) {
        if self.issueId != issueId {
            self.issueId = issueId
            blockerTask?.cancel()
            blockerTask = nil
            blockers = []
            relations = []
        }
        if self.teamId != teamId {
            self.teamId = teamId
            sessionTask?.cancel()
            sessionTask = nil
            sessions = []
            sessionIssues = []
        }
        observePullRequests()
        observeBlockers()
        observeSessions()
        observeUsers()
    }

    func stop() {
        prTask?.cancel()
        prTask = nil
        blockerTask?.cancel()
        blockerTask = nil
        sessionTask?.cancel()
        sessionTask = nil
        userTask?.cancel()
        userTask = nil
    }

    private func observeUsers() {
        guard userTask == nil, let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }
        userTask = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    self?.users = rows
                }
            } catch {}
        }
    }

    private func observePullRequests() {
        guard prTask == nil, let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try IssueEntity.filter(Column("pr_url") != nil).fetchAll(db)
        }
        prTask = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    self?.prIssues = rows
                }
            } catch {}
        }
    }

    /// EXP-980: every `blocks` row and the issues at BOTH its ends, in ONE
    /// join. The Issue face draws the TRANSITIVE chain now, so the direct
    /// inverse rows this used to read are no longer enough; `StackStart` and
    /// `IssueGraph` apply the terminal-status and ordering rules on the way
    /// out, and a wider pool changes neither's answer.
    private func observeBlockers() {
        guard blockerTask == nil, issueId != nil else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db -> ([IssueEntity], [IssueRelationEntity]) in
            let relations = try IssueRelationEntity
                .filter(Column("type") == IssueRelationType.blocks.rawValue)
                .fetchAll(db)
            let ids = Array(Set(relations.flatMap { [$0.issueId, $0.relatedIssueId] }))
            let issues = ids.isEmpty
                ? []
                : try IssueEntity.filter(ids.contains(Column("id"))).fetchAll(db)
            return (issues, relations)
        }
        blockerTask = Task { [weak self] in
            do {
                for try await (issues, relations) in observation.values(in: pool) {
                    self?.blockers = issues
                    self?.relations = relations
                }
            } catch {}
        }
    }

    /// The team's runs, and the issues they are bound to in the SAME tracked
    /// read — one observation, so a tree row is named the moment its run (or
    /// its issue) syncs.
    private func observeSessions() {
        guard sessionTask == nil, let teamId else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db -> ([CodingSessionEntity], [IssueEntity]) in
            let sessions = try CodingSessionEntity.filter(Column("team_id") == teamId).fetchAll(db)
            let ids = Array(Set(sessions.compactMap(\.issueId)))
            guard !ids.isEmpty else { return (sessions, []) }
            let issues = try IssueEntity.filter(ids.contains(Column("id"))).fetchAll(db)
            return (sessions, issues)
        }
        sessionTask = Task { [weak self] in
            do {
                for try await (rows, issues) in observation.values(in: pool) {
                    self?.sessions = rows
                    self?.sessionIssues = issues
                }
            } catch {}
        }
    }
}
