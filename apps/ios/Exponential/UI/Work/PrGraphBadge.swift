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
/// SHAPE (`PrGraph.badgeShape`: a stack or batch, else open blockers), a
/// small mono `+N` beside it; the tap opens the "Related work" sheet
/// (`PrGraphSheet`, SLOP-16 r5): the relations card's bands and nothing else
/// — the same layout and copy on every face and every platform.
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
        case .blocked, nil: AppIcons.relationBlockedBy
        }
    }

    /// The badge's spoken name, by what it stands for.
    static func accessibilityName(_ shape: PrGraph.BadgeShape?) -> String {
        switch shape {
        case .stack: "Pull request stack"
        case .batch: "Batch pull request"
        case .stackAndBatch: "Stack and batch"
        case .blocked, nil: IssueRelationsView.Copy.blockedBy
        }
    }
}

// MARK: - The overlay

/// SLOP-16 r5: THE "Related work" view, one layout ×4 — the platform's
/// standard sheet whose body is EXACTLY the relations card's bands
/// (`IssueRelationBand`: foldable, counted, capped at 3 behind "Show N
/// more"), in one order on every face: Blocked by (the direct open
/// blockers), Same pull request (the batch partners), Pull request stack
/// (the OTHER pull requests of the stack, bottom-up). Nothing else.
struct PrGraphSheet: View {
    let graph: PrGraph.Graph
    /// The subject issue (a run's issue on a run) — never its own partner.
    var subjectIssueId: String?
    /// The issue rows' assignee avatars and team-resolved status glyphs.
    var users: [UserEntity] = []
    var teamStatuses: [ResolvedIssueStatus] = []
    let onOpenIssue: (String) -> Void
    /// A PR row: that pull request's Changes face.
    let onOpenPullRequest: (PrGraph.Entry) -> Void

    /// Bands the reader folded (every band opens by default here).
    @State private var folded: Set<PrGraph.OverlaySection> = []
    /// Bands whose "Show N more" was pressed.
    @State private var showAll: Set<PrGraph.OverlaySection> = []

    private var blockers: [IssueEntity] { graph.blockers }

    /// Everything sharing the subject's pull request but the subject itself.
    private var partners: [IssueEntity] {
        (graph.batch?.issues ?? []).filter { $0.id != subjectIssueId }
    }

    /// The stack's OTHER pull requests, bottom-up.
    private var otherPullRequests: [PrGraph.Entry] {
        graph.stack.map(\.entry).filter { $0.id != graph.entry?.id }
    }

    /// The bands with rows to draw — a batch whose partners have not synced
    /// is left out, so a sheet left with nothing shows the empty note.
    private var sections: [PrGraph.OverlaySection] {
        PrGraph.overlaySections(graph).filter { count($0) > 0 }
    }

    private func count(_ section: PrGraph.OverlaySection) -> Int {
        switch section {
        case .blocked: blockers.count
        case .batch: partners.count
        case .stack: otherPullRequests.count
        }
    }

    var body: some View {
        // Content-sized (≤85 %, the chrome scrolls past it): the sheet is as
        // tall as what it lists, never a mostly empty full-height page.
        GlassSheetChrome(title: PrGraph.OverlayCopy.relatedWorkTitle, height: .fitted) {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(sections, id: \.self) { section in
                    band(section)
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

    // MARK: Bands

    private func band(_ section: PrGraph.OverlaySection) -> some View {
        let total = count(section)
        let expanded = !folded.contains(section)
        let everything = showAll.contains(section)
        let overflow = total > IssueRelationsView.bandCap
        let shown = everything || !overflow ? total : IssueRelationsView.bandCap
        let more: String? = !overflow
            ? nil
            : everything ? IssueRelationsView.Copy.showLess
            : IssueRelationsView.showMore(total - IssueRelationsView.bandCap)
        return IssueRelationBand(
            title: PrGraph.overlayBandTitle(section),
            icon: Self.icon(section),
            count: total,
            expanded: expanded,
            moreLabel: more,
            identifier: "related-work-band-\(section.rawValue)",
            onToggle: { toggle(&folded, section) },
            onToggleShowAll: { toggle(&showAll, section) }
        ) {
            VStack(spacing: 0) {
                switch section {
                case .blocked: issueRows(Array(blockers.prefix(shown)))
                case .batch: issueRows(Array(partners.prefix(shown)))
                case .stack: pullRequestRows(Array(otherPullRequests.prefix(shown)))
                }
            }
        }
    }

    private func toggle(
        _ set: inout Set<PrGraph.OverlaySection>, _ section: PrGraph.OverlaySection
    ) {
        if set.contains(section) { set.remove(section) } else { set.insert(section) }
    }

    /// One glyph per band, the relations card's style.
    static func icon(_ section: PrGraph.OverlaySection) -> String {
        switch section {
        case .blocked: AppIcons.relationBlockedBy
        case .batch: AppIcons.prBatch
        case .stack: AppIcons.prStack
        }
    }

    // MARK: Rows

    @ViewBuilder
    private func issueRows(_ rows: [IssueEntity]) -> some View {
        ForEach(Array(rows.enumerated()), id: \.element.id) { index, issue in
            if index > 0 { GlassDivider() }
            RelatedIssueRow(
                issue: issue, users: users, teamStatuses: teamStatuses,
                onOpen: { onOpenIssue(issue.id) }
            )
        }
    }

    @ViewBuilder
    private func pullRequestRows(_ rows: [PrGraph.Entry]) -> some View {
        ForEach(Array(rows.enumerated()), id: \.element.id) { index, entry in
            if index > 0 { GlassDivider() }
            RelatedPullRequestRow(entry: entry) { onOpenPullRequest(entry) }
        }
    }
}

/// A pull request in the relations row shape: PR glyph · mono `#n` (the
/// identifier when there is no number) · the representative issue's title ·
/// the PR state pill. Tap opens its Changes face.
struct RelatedPullRequestRow: View {
    let entry: PrGraph.Entry
    let onOpen: () -> Void

    var body: some View {
        Button(action: onOpen) {
            HStack(spacing: 12) {
                AppIcon(Self.glyph(entry.prState), size: IssueRelationRowTokens.glyphSize)
                    .foregroundStyle(IssueStatus.inReview.color)
                    .frame(width: 20)
                Text(number)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .fixedSize()
                Text(entry.representative.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                GlassPill(Self.stateLabel(entry.prState), icon: Self.glyph(entry.prState))
            }
            .padding(.horizontal, 12)
            .frame(minHeight: IssueRelationRowTokens.minHeight)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .flatRow()
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(number) \(entry.representative.title), \(Self.stateLabel(entry.prState))")
        .accessibilityAddTraits(.isButton)
    }

    private var number: String {
        entry.prNumber.map { "#\($0)" } ?? entry.representative.identifier ?? ""
    }

    static func glyph(_ state: String?) -> String {
        switch state {
        case DomainContract.prStateMerged: AppIcons.prMerged
        case DomainContract.prStateClosed: AppIcons.prClosed
        case DomainContract.prStateDraft: AppIcons.prDraft
        default: AppIcons.prOpen
        }
    }

    static func stateLabel(_ state: String?) -> String {
        switch state {
        case DomainContract.prStateMerged: "Merged"
        case DomainContract.prStateClosed: "Closed"
        case DomainContract.prStateDraft: "Draft"
        default: "Open"
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
