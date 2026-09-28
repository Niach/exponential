import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-897 Part 4 — the ONE badge that says a piece of work is entangled with
/// other work, and the ONE overlay behind it, ×4 (web `pr-graph-badge.tsx`,
/// desktop `pr_graph.rs`, Android `PrGraphBadge.kt`).
///
/// EXP-1058: the badge is the STACKED issue chip (`IssueChipStack`) in the
/// Work header — the subject PR's representative issue in front with `+N`
/// for every other issue of its stack or batch (`PrGraph.badgeChip`), or, for
/// a run family with no issue, the run itself behind the session-tree glyph.
/// EXP-1097: the chip is FACE-INDEPENDENT (`PrGraph.badgeShape`: a stack or
/// batch, else a run family, else open blockers) and COMPACT (glyph ·
/// identifier · `+N`), beside the header's `…` on every face. The front chip
/// is inert: the whole stack is ONE tap target that opens the overlay, whose
/// sections are every relation the subject has, the face's own FIRST
/// (`PrGraph.overlaySections`) — the same rows and copy on every face:
///
/// - Issue face → EXP-980: the blocks MINI-GRAPH (the transitive chain, waves
///   and all — it replaced the flat "Blocked by" chips) + "In batch with"
/// - Run face → EXP-930: a BATCH run's covered "Issues" first, then the
///   session tree (nested, live dots, tap opens the run)
/// - Changes face → the PR stack bottom-up (identifiers, PR state, a batch's
///   issues folded underneath, "Merge stack" on the bottom entry)
struct PrGraphBadge: View {
    let chip: PrGraph.BadgeChip
    /// What the no-issue front chip names: the run's own identity.
    let runName: String?
    let accessibilityName: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 0) {
                IssueChipStack(depth: min(max(chip.count, 1), 2)) { front }
                // The `+N` rides BESIDE the stack, clear of the ghosts (web
                // `IssueChipStack`'s count slot, EXP-1097).
                if let countSuffix {
                    Text(countSuffix)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.leading, 6)
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

    /// `+3` — who rides behind the front chip; nil when nobody does.
    private var countSuffix: String? {
        chip.count > 0 ? "+\(chip.count)" : nil
    }

    /// EXP-1097: the phone header's COMPACT chip — status glyph + identifier,
    /// no title, on every face. A run family with no issue names the run
    /// behind the session-tree glyph instead.
    @ViewBuilder
    private var front: some View {
        if let issue = chip.issue {
            IssueChip(
                identifier: issue.identifier,
                title: nil,
                status: IssueStatus.from(issue.status)
            )
            .fixedSize()
        } else {
            IssueChip(
                identifier: nil,
                title: runName ?? accessibilityName,
                iconName: AppIcons.sessionTree,
                statusColor: .white.opacity(TextOpacity.secondary)
            )
            .frame(maxWidth: Self.maxWidth)
        }
    }

    /// The run-name chip truncates its title before it crowds the header.
    static let maxWidth: CGFloat = 140

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

/// The badge's sheet. Same row primitives on every face — a `.glassRow()` band
/// per section — so switching faces changes WHAT is listed, never how.
struct PrGraphSheet: View {
    let graph: PrGraph.Graph
    let face: WorkFaceKind
    /// EXP-930: every issue row the screen has synced — what gives a tree row
    /// its own name (an issue run's title, a batch run's `EXP-874 +2`) instead
    /// of the "Untitled issue" every issue-less row used to read.
    var issues: [IssueEntity] = []
    /// EXP-980: the transitive blocks graph around the subject issue — what
    /// the Issue face leads with now. Empty (or a lone subject node) means
    /// nothing blocks it and it blocks nothing.
    var blockGraph = IssueGraph.Graph(nodes: [], edges: [], hasCycle: false, truncated: false)
    /// The subject issue — the Issue face's batch section lists its PARTNERS.
    var subjectIssueId: String?
    let onOpenIssue: (String) -> Void
    let onOpenRun: (String) -> Void
    let onMergeStack: (String) -> Void

    /// EXP-1097: every relation the subject HAS gets its section on every
    /// face; the face only decides which one LEADS (`overlaySections`).
    private var sections: [PrGraph.OverlaySection] {
        PrGraph.overlaySections(graph, face: face)
    }

    var body: some View {
        GlassSheetChrome(title: title, height: .fitted) {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(sections, id: \.self) { section in
                    switch section {
                    case .blocked: blockedSection
                    case .batch: batchSection
                    case .runs: runsSection
                    case .stack: stackSection
                    }
                }
                if sections.isEmpty {
                    emptyNote("Nothing else is linked to this issue.")
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .accessibilityIdentifier("pr-graph-sheet")
    }

    private var title: String {
        switch face {
        // EXP-930: the run face lists the batch's issues as well as its runs,
        // so the sheet wears the neutral heading both sections sit under.
        case .issue, .run, .results: "Related work"
        case .changes: "Pull requests"
        }
    }

    // MARK: Sections

    /// EXP-980: the transitive blocks chain as THE mini-graph (its own
    /// `Wave n` bands are the headings), the direct blockers as rows when the
    /// graph has not resolved.
    @ViewBuilder
    private var blockedSection: some View {
        if blockGraph.nodes.count > 1 {
            IssueGraphView(graph: blockGraph, issues: issues, onOpenIssue: onOpenIssue)
        } else {
            section(IssueRelationsView.Copy.blockedBy) {
                ForEach(graph.blockers, id: \.id) { issue in
                    issueRow(issue)
                }
            }
        }
    }

    /// On the Issue face the subject is the reader's own issue, so the batch
    /// lists its PARTNERS; on a run the covered set IS the run's subject
    /// (EXP-930: the whole set, not "everything but me").
    @ViewBuilder
    private var batchSection: some View {
        let rows = (graph.batch?.issues ?? []).filter { row in
            face != .issue || row.id != subjectIssueId
        }
        if !rows.isEmpty {
            section(face == .issue ? "In batch with" : "Issues") {
                ForEach(rows, id: \.id) { issue in
                    issueRow(issue)
                }
            }
        }
    }

    @ViewBuilder
    private var runsSection: some View {
        section("Runs") {
            // EXP-965: the run tree's connector, off its depths. The section
            // stacks its rows flush (spacing 0), so no gap to bridge.
            let guides = TreeGuides.compute(depths: graph.tree.map(\.depth))
            ForEach(Array(graph.tree.enumerated()), id: \.element.session.id) { index, row in
                runRow(row, guide: guides[index])
            }
        }
    }

    @ViewBuilder
    private func runRow(
        _ row: SessionTree.Row<CodingSessionEntity>, guide: TreeGuide
    ) -> some View {
        let session = row.session
        let state = CodingSessionDisplayState.of(session: session, prState: session.prState)
        Button { onOpenRun(session.id) } label: {
            HStack(spacing: 8) {
                SessionStateDot(
                    tone: CodingSessionLiveness.isLive(session)
                        ? SessionStateDot.tone(of: state) : .muted,
                    pulsing: session.agentBusy,
                    size: 8
                )
                // EXP-930: named off the rows this screen holds — the run's own
                // issue when it has one, the batch's covered set when it does
                // not (`BatchRun.name` reads only the ones it covers).
                Text(
                    sessionRowTitle(
                        issue: session.issueId.flatMap { id in issues.first { $0.id == id } },
                        session: session,
                        batchIssues: issues
                    )
                )
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.leading, CGFloat(row.depth) * TreeGuides.indentPerLevel)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .background(TreeGuidesOverlay(guide: guide))
            .glassRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    /// The pull requests bottom-up — the stack, or the subject's lone PR
    /// when the Changes face leads with it.
    @ViewBuilder
    private var stackSection: some View {
        if graph.stack.isEmpty {
            emptyNote("This pull request stands on its own.")
        } else {
            section("The stack, bottom first") {
                // EXP-965: the same connector the lists draw, over the rungs.
                let guides = TreeGuides.compute(depths: graph.stack.map(\.depth))
                ForEach(Array(graph.stack.enumerated()), id: \.element.id) { index, rung in
                    stackRow(rung, guide: guides[index])
                }
            }
        }
    }

    @ViewBuilder
    private func stackRow(_ rung: PrGraph.StackEntry, guide: TreeGuide) -> some View {
        let entry = rung.entry
        let isCurrent = entry.id == graph.entry?.id
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                AppIcon(entry.isBatch ? AppIcons.prBatch : AppIcons.prOpen, size: 12)
                    .foregroundStyle(IssueStatus.inReview.color)
                Text(entry.identifiers.joined(separator: ", "))
                    .font(isCurrent ? .subheadline.weight(.semibold) : .subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                if let state = entry.prState, !state.isEmpty {
                    Text(state)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                Spacer(minLength: 0)
                if rung.depth == 0, graph.size > 1 {
                    GlassPill("Merge stack", icon: AppIcons.prStack)
                        .contentShape(Capsule())
                        .onTapGesture { onMergeStack(entry.representative.id) }
                        .accessibilityAddTraits(.isButton)
                        .accessibilityLabel("Merge the whole stack")
                }
            }
            // The batch's own issues, folded underneath its entry — one
            // level deeper (EXP-965: 14 pt like everywhere else, with the
            // connector that says they hang off this row).
            if entry.isBatch, !sections.contains(.batch) {
                let childGuides = TreeGuides.compute(depths: entry.issues.map { _ in 1 })
                ForEach(Array(entry.issues.enumerated()), id: \.element.id) { index, issue in
                    Button { onOpenIssue(issue.id) } label: {
                        Text("\(issue.identifier ?? "") \(issue.title)")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                    // The stack row's own VStack spaces them 6pt apart.
                    .treeGuides(childGuides[index], base: 0, gap: 6)
                }
            }
        }
        .padding(.leading, CGFloat(rung.depth) * TreeGuides.indentPerLevel)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(TreeGuidesOverlay(guide: guide))
        .glassRow()
    }

    // MARK: Primitives

    @ViewBuilder
    private func section(
        _ title: String, @ViewBuilder rows: () -> some View
    ) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(title)
            rows()
        }
    }

    @ViewBuilder
    private func issueRow(_ issue: IssueEntity) -> some View {
        Button { onOpenIssue(issue.id) } label: {
            HStack(spacing: 8) {
                IssueChip(
                    identifier: issue.identifier,
                    title: issue.title,
                    status: IssueStatus.from(issue.status)
                )
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func emptyNote(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 12)
            .glassRow()
    }
}

/// EXP-897 Part 4: the plain issue list a batch row's glyph opens (Reviews).
/// Same row primitive as the badge's overlay, so the two read alike.
struct PrGraphIssueSheet: View {
    let title: String
    let issues: [IssueEntity]
    let onOpenIssue: (String) -> Void

    var body: some View {
        GlassSheetChrome(title: title, height: .fitted) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(issues, id: \.id) { issue in
                    Button { onOpenIssue(issue.id) } label: {
                        HStack(spacing: 8) {
                            IssueChip(
                                identifier: issue.identifier,
                                title: issue.title,
                                status: IssueStatus.from(issue.status)
                            )
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .glassRow()
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
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

    private let accountId: String
    private let db: DatabaseManager
    private var issueId: String?
    private var teamId: String?

    private var prTask: Task<Void, Never>?
    private var blockerTask: Task<Void, Never>?
    private var sessionTask: Task<Void, Never>?

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
    }

    func stop() {
        prTask?.cancel()
        prTask = nil
        blockerTask?.cancel()
        blockerTask = nil
        sessionTask?.cancel()
        sessionTask = nil
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
