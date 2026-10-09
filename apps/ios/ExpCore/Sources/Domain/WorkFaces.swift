import Foundation

// EXP-893: the PHONE's Work screen — one screen per subject (an issue, or a
// session) with up to three FACES held as screen state, never as navigation:
// `issue`, `run` and `guide` (EXP-1251: Changes + Results merged into the
// Guide, its diff counts moved into the body). EXP-1150: the faces are TABS —
// a segmented strip under the nav bar lists `availableFaces` with
// `faceLabel` (tapping the selected `Runs` tab opens the run menu), and a
// horizontal swipe on the face body moves to the neighbour (`swipeTarget`).
// Stop / Resume sit in the nav bar's trailing slot only while the Run face
// shows. These are the PURE rules every phone client mirrors byte for byte:
// web `lib/work-faces.ts` (the spec), Android `domain/WorkFaces.kt` — same
// names, same cases, same test names.

/// The three faces. EXP-1251: `guide` = the run's Guide (its published
/// sections + pictures) over the diff (the run's live diff, else the issue's
/// open PR).
public enum WorkFaceKind: String, Equatable, Sendable, CaseIterable {
    case issue
    case run
    case guide
}

/// EXP-862: the ONE session-dot mapping, hand-mirrored with web
/// `lib/session-dot.ts` and the desktop's `queries::session_dot_tone` — a
/// running or ready-for-review run is green, one that waits on a human (needs
/// input, or gone quiet past the stale threshold) amber, a done one blue, an
/// ended or paused one muted.
public enum SessionDotTone: String, Equatable, Sendable {
    case running
    case review
    case needsInput = "needs_input"
    case done
    case muted
}

public enum WorkFaces {
    public static let issueFaceLabel = "Issue"
    public static let runFaceLabel = "Run"
    /// EXP-886: the Run face's label with MORE THAN ONE own run on the issue.
    public static let runsFaceLabel = "Runs"
    /// EXP-1251: the Guide face (contract `diffUi.guideFace`).
    public static let guideFaceLabel = DomainContract.diffUiGuideFace
    /// EXP-933: the transcript card under a settled `sessions_guide` call that
    /// switches the run to its Guide face, byte-identical ×4.
    public static let openResultsLabel = "Open \(guideFaceLabel)"
    /// The Start coding verb once the shown run ended for good.
    public static let startCodingLabel = "Start coding"

    /// The steer composer's placeholder, byte-identical ×4 (desktop
    /// `steer-composer.tsx` / `session_screen.rs`).
    public static let steerComposerPlaceholder = "Type / for commands"
    /// The composer footer's plan-mode word, blue while plan mode is on.
    public static let planModeLabel = "Plan mode"

    public static func faceLabel(_ face: WorkFaceKind, multipleRuns: Bool = false) -> String {
        switch face {
        case .issue: return issueFaceLabel
        case .run: return multipleRuns ? runsFaceLabel : runFaceLabel
        case .guide: return guideFaceLabel
        }
    }

    /// The faces a subject can show, in their fixed order. EXP-1251: the
    /// Guide shows when the run published results OR there is a diff (live,
    /// PR or branch), independent of Run: an issue with an open PR and no run
    /// of mine still has its PR files. It comes last.
    public static func availableFaces(
        hasIssue: Bool, hasRun: Bool, hasResults: Bool, hasDiff: Bool
    ) -> [WorkFaceKind] {
        var faces: [WorkFaceKind] = []
        if hasIssue { faces.append(.issue) }
        if hasRun { faces.append(.run) }
        if hasResults || hasDiff { faces.append(.guide) }
        return faces
    }

    private static func stamp(_ value: String) -> TimeInterval {
        WireTimestamps.parse(value)?.timeIntervalSince1970 ?? 0
    }

    /// Live by status AND heartbeat: a `running` row whose machine went quiet
    /// past the staleness window is neither steerable nor stoppable.
    public static func isSessionLive(_ session: CodingSessionEntity, now: Date) -> Bool {
        CodingSessionLiveness.isLive(session, now: now)
    }

    private static func newest(_ rows: [CodingSessionEntity]) -> CodingSessionEntity? {
        var best: CodingSessionEntity?
        for row in rows {
            guard let current = best else {
                best = row
                continue
            }
            let a = stamp(row.startedAt)
            let b = stamp(current.startedAt)
            if a > b || (a == b && row.id > current.id) {
                best = row
            }
        }
        return best
    }

    /// The run an issue's Work screen shows — desktop `work_header.rs`
    /// `coding_target`: the bound run when it is mine and live, else my newest
    /// live run on the issue, else my newest run at all. `nil` = no run of
    /// mine.
    public static func codingTarget(
        _ rows: [CodingSessionEntity],
        issueId: String,
        boundId: String?,
        me: String?,
        now: Date
    ) -> CodingSessionEntity? {
        guard let me else { return nil }
        let mine = rows.filter { $0.issueId == issueId && $0.userId == me }
        let live = mine.filter { isSessionLive($0, now: now) }
        if let boundId, let bound = live.first(where: { $0.id == boundId }) {
            return bound
        }
        return newest(live) ?? newest(mine)
    }

    /// EXP-933: the run whose RESULTS an issue shows — `codingTarget` when
    /// that run has any, else the newest run on the issue (startedAt, ties:
    /// larger id) by ANY member with results, else (EXP-1251) the newest run
    /// anywhere whose topics are tagged with the issue's PR. Fixture
    /// `session-results.json` `issueResultsRun` (×4).
    public static func issueResultsRun(
        _ rows: [CodingSessionEntity],
        issueId: String,
        boundId: String?,
        me: String?,
        now: Date,
        issuePrUrl: String? = nil
    ) -> CodingSessionEntity? {
        if let own = codingTarget(rows, issueId: issueId, boundId: boundId, me: me, now: now),
           hasSessionResults(own.results) {
            return own
        }
        let onIssue = newest(rows.filter { $0.issueId == issueId && hasSessionResults($0.results) })
        guard onIssue == nil, let issuePrUrl, !issuePrUrl.isEmpty else { return onIssue }
        return newest(rows.filter { sessionResultPrUrls($0.results).contains(issuePrUrl) })
    }

    /// EXP-934: the header's `…` CONTEXT MENU (Share · Move to board · Unmark
    /// duplicate · Delete issue) belongs to the issue, so it shows on the
    /// ISSUE face alone. On Run and Guide the trailing slot carries
    /// the run's own verb (Stop / Resume) and nothing else — a Delete issue
    /// sitting beside a running agent acts on a subject that face is not even
    /// showing.
    public static func faceShowsContextMenu(_ face: WorkFaceKind) -> Bool {
        face == .issue
    }

    public enum PrimaryAction: Equatable, Sendable {
        case stop
        case resume
        case start
        case none
    }

    /// The nav bar's trailing verb on the Run face: Stop wins over everything; Resume needs an ended
    /// own run a machine can take; Start only for an issue subject that can be
    /// coded on.
    public static func primaryAction(
        ownLive: Bool, ownEndedResumable: Bool, canStart: Bool
    ) -> PrimaryAction {
        if ownLive { return .stop }
        if ownEndedResumable { return .resume }
        if canStart { return .start }
        return .none
    }

    /// EXP-1150: which way the finger went. `left` = the content followed the
    /// finger leftwards, so the NEXT face slides in; `right` = the previous.
    public enum SwipeDirection: String, Equatable, Sendable {
        case left
        case right
    }

    /// The face a horizontal swipe on the body lands on: the neighbour in the
    /// strip's order, nil at either end (or when the shown face is not in the
    /// strip at all).
    public static func swipeTarget(
        faces: [WorkFaceKind], shown: WorkFaceKind, direction: SwipeDirection
    ) -> WorkFaceKind? {
        guard let index = faces.firstIndex(of: shown) else { return nil }
        let next = direction == .left ? index + 1 : index - 1
        guard faces.indices.contains(next) else { return nil }
        return faces[next]
    }

    /// Where a face lands when it vanishes under the reader (the diff cleared
    /// and the results list was empty, the run row went): guide → run →
    /// issue. `nil` = nothing left.
    public static func fallbackFace(
        shown: WorkFaceKind, available: [WorkFaceKind]
    ) -> WorkFaceKind? {
        if available.contains(shown) { return shown }
        let order: [WorkFaceKind] = switch shown {
        case .guide: [.run, .issue]
        case .run: [.issue]
        case .issue: []
        }
        return order.first { available.contains($0) } ?? available.first
    }

    /// EXP-1154: whether a requested `initialFace` that is not available yet
    /// keeps HOLDING the fallback off. It lets go once the face is there or
    /// the reader moved elsewhere; otherwise it waits for the runs AND, for
    /// the Guide, for the issue row too: it reads it (the open PR, the pushed
    /// branch), and it usually lands after the runs, so a Reviews row would
    /// otherwise fall back to the Issue face. `issueResolved` =
    /// true when the subject names no issue.
    public static func holdsPendingFace(
        pending: WorkFaceKind,
        shown: WorkFaceKind,
        available: [WorkFaceKind],
        runsResolved: Bool,
        issueResolved: Bool
    ) -> Bool {
        if available.contains(pending) || shown != pending { return false }
        if !runsResolved { return true }
        switch pending {
        case .guide: return !issueResolved
        case .issue, .run: return false
        }
    }

    /// The composer footer's model — the `model` config option's value, nil
    /// when the engine reported none or a blank (web `lib/agent-feed.ts`).
    public static func sessionModel(_ config: AgentSessionConfig?) -> String? {
        guard let value = config?.options.first(where: { $0.id == "model" })?.value,
              !value.isEmpty
        else { return nil }
        return value
    }
}

// MARK: - EXP-1175: the Run face's status row + Show work

/// The ×4 display state (`CodingSessionDisplayState`) plus the two the status
/// row alone tells apart: a paused (offline host) run and an ended one.
public enum RunRowState: String, Equatable, Sendable {
    case working
    case needsInput = "needs_input"
    case paused
    case review
    case done
    case ended
}

/// The status row caption's tone — the session list row's tints (fixture
/// `statusTone`); the app maps it to a colour.
public enum RunRowTone: String, Equatable, Sendable {
    case muted
    case amber
    case emerald
    case sky
}

public struct RunRowCaption: Equatable, Sendable {
    public let text: String
    public let tone: RunRowTone

    public init(text: String, tone: RunRowTone) {
        self.text = text
        self.tone = tone
    }
}

public extension WorkFaces {
    /// The Run face opens as the THREAD: one status row (the agent's run mark
    /// as the spinner, `runRowCaption`, `AgentFeed.lastToolLine` muted, the
    /// Show work button) over the run's published results (`sessionThread`),
    /// pending plan/question cards still in place. Show work swaps the thread
    /// for the full transcript IN PLACE; remembered per user. Fixture
    /// `packages/domain-contract/fixtures/run-row.json` (×4).
    static let showWorkDefault = false
    static let showWorkText = "Show work"
    static let hideWorkText = "Hide work"

    /// The row's trailing button: what pressing it DOES next.
    static func showWorkLabel(_ showWork: Bool) -> String {
        showWork ? hideWorkText : showWorkText
    }

    /// The per-user preference's `UserDefaults` key.
    static func showWorkDefaultsKey(accountId: String) -> String {
        "run_show_work_\(accountId)"
    }

    /// The row's state: the VIEWER's live signals folded over the synced
    /// display state, so the row never contradicts the Run tab's mark —
    /// paused (offline host) first, then ended, then needs input when the
    /// viewer sees a pending plan/question OR the display state says so, then
    /// working when the viewer's working predicate OR the display state says
    /// so, else the display state. Fixture `run-row.json` `states` (×4).
    static func runRowState(
        paused: Bool,
        ended: Bool,
        awaitingInput: Bool,
        working: Bool,
        display: CodingSessionDisplayState
    ) -> RunRowState {
        if paused { return .paused }
        if ended { return .ended }
        if awaitingInput || display == .needsInput { return .needsInput }
        if working || display == .working { return .working }
        return display == .review ? .review : .done
    }

    /// The status row's first line and tone: `Building on <device> · <elapsed>`
    /// while it works (now − start), `Ended on <device> · <elapsed>` once it
    /// is over (end − start; the caller passes `ended_at`, else `updated_at`),
    /// else the session list row's words. The elapsed part is the working
    /// caption's ladder and drops when a stamp is missing or unparsable.
    static func runRowCaption(
        state: RunRowState,
        device: String,
        startedAt: String?,
        endedAt: String?,
        now: Date
    ) -> RunRowCaption {
        runRowCaption(
            state: state,
            device: device,
            start: startedAt.flatMap(WireTimestamps.parse),
            end: endedAt.flatMap(WireTimestamps.parse),
            now: now
        )
    }

    /// The same caption over parsed stamps (what a turn's row reads).
    static func runRowCaption(
        state: RunRowState,
        device: String,
        start: Date?,
        end: Date?,
        now: Date
    ) -> RunRowCaption {
        switch state {
        case .paused: return RunRowCaption(text: "Paused · \(device)", tone: .muted)
        case .needsInput: return RunRowCaption(text: "Needs input · \(device)", tone: .amber)
        case .review: return RunRowCaption(text: "Ready for review · \(device)", tone: .emerald)
        case .done: return RunRowCaption(text: "Done · \(device)", tone: .sky)
        case .working, .ended:
            let verb = state == .working ? "Building on" : "Ended on"
            let finish = state == .working ? now : end
            var elapsed = ""
            if let start, let finish {
                elapsed = " · \(AgentFeed.workingDuration(ms: elapsedMs(start, finish)))"
            }
            return RunRowCaption(text: "\(verb) \(device)\(elapsed)", tone: .muted)
        }
    }

    private static func elapsedMs(_ start: Date, _ end: Date) -> Int {
        Int((end.timeIntervalSince(start) * 1000).rounded())
    }

    /// EXP-1245: one status row PER TURN. A settled turn reads `Done on
    /// <device> · <turn duration>`; the open (newest) turn reads the run's row
    /// (`runRowCaption`) timed from the TURN's start, not the run's; a sent
    /// message still waiting for its turn has no row (nil). `runEndedAt` =
    /// the run's `ended_at`, else `updated_at`. Fixture `run-row.json`
    /// `turnCaptions` (×4).
    static func turnRowCaption(
        turnStartedAt: Date?,
        turnEndedAt: Date?,
        state: RunRowState,
        device: String,
        runEndedAt: Date?,
        now: Date
    ) -> RunRowCaption? {
        guard let start = turnStartedAt else { return nil }
        if let end = turnEndedAt {
            return RunRowCaption(
                text: "Done on \(device) · \(AgentFeed.workingDuration(ms: elapsedMs(start, end)))",
                tone: .muted
            )
        }
        return runRowCaption(state: state, device: device, start: start, end: runEndedAt, now: now)
    }

    /// A turn's row caption off the turns model (its ms stamps).
    static func turnRowCaption(
        _ turn: SessionTurn,
        state: RunRowState,
        device: String,
        runEndedAt: Date?,
        now: Date
    ) -> RunRowCaption? {
        turnRowCaption(
            turnStartedAt: turn.startedAt.map { Date(timeIntervalSince1970: $0 / 1000) },
            turnEndedAt: turn.endedAt.map { Date(timeIntervalSince1970: $0 / 1000) },
            state: state,
            device: device,
            runEndedAt: runEndedAt,
            now: now
        )
    }
}

// MARK: - EXP-1251: the Guide's section pages

/// A section page under the Guide: a 1-based section number, `lead` (the
/// Summary's own files), `other` (the automatic Other changes) or `all`
/// (Show complete diff). Web `GuideSectionKey`.
public enum GuideSectionKey: Equatable, Hashable, Sendable {
    case section(Int)
    case lead
    case other
    case all
}

/// A section page: the back row's caption (`02 / 06`, numbered sections
/// only), its title, the files it covers and their summed counts.
public struct GuideSectionPage: Equatable, Sendable {
    public let section: GuideSectionKey
    public let caption: String?
    public let title: String
    public let files: [Diff.File]
    public let additions: Int
    public let deletions: Int
}

public extension WorkFaces {
    /// The page `section` opens over `files` (the diff the Guide counts): the
    /// same coverage the Guide's rows read, so a page never shows a file its
    /// row did not count. nil = no such section (a stale link) or no diff
    /// loaded.
    static func guideSectionPage(
        _ groups: [SessionResultGroup],
        files: [Diff.File]?,
        section: GuideSectionKey
    ) -> GuideSectionPage? {
        guard let files else { return nil }
        let coverage = guideCoverage(groups, files)
        func page(_ title: String, _ caption: String?, _ set: GuideChangeSet?) -> GuideSectionPage? {
            guard let set else { return nil }
            return GuideSectionPage(
                section: section, caption: caption, title: title,
                files: set.files, additions: set.additions, deletions: set.deletions
            )
        }
        switch section {
        case .all: return page(guideChangesTopic, nil, coverage.complete)
        case .other:
            guard let other = coverage.other else { return nil }
            return page(other.topic, nil, other.changes)
        case .lead:
            guard let lead = coverage.lead else { return nil }
            return page(lead.group.topic, nil, lead.changes)
        case let .section(index):
            guard let hit = coverage.sections.first(where: { $0.index == index }) else { return nil }
            return page(hit.group.topic, guideSectionCaption(hit.index, hit.total), hit.changes)
        }
    }

    /// The section page's back-row summary: `+A −D · N files`.
    static func guideSectionSummary(_ page: GuideSectionPage) -> String {
        "\(Diff.additionsLabel(page.additions)) \(Diff.deletionsLabel(page.deletions)) · \(guideFileCountLabel(page.files.count))"
    }
}
