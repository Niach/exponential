import Foundation

// EXP-893: the PHONE's Work screen — one screen per subject (an issue, or a
// session) with up to four FACES held as screen state, never as navigation:
// `issue`, `run`, `changes` and (EXP-879) `results`. EXP-1150: the faces are
// TABS — a segmented strip under the nav bar lists `availableFaces` with
// `faceLabel` (tapping the selected `Runs` tab opens the run menu), and a
// horizontal swipe on the face body moves to the neighbour (`swipeTarget`).
// Stop / Resume sit in the nav bar's trailing slot only while the Run face
// shows, the merge bar only on Changes, and the Run face's bar keeps ONE
// trailing circle (`runBarTrailing`). These are the PURE rules every
// phone client mirrors byte for byte: web `lib/work-faces.ts` (the spec),
// Android `domain/WorkFaces.kt` — same names, same cases, same test names.

/// The four faces. `changes` is the run's diff, else the issue's open PR;
/// `results` (EXP-879) is the shown RUN's published screenshots, so — like the
/// run's own diff — it is a sub-face of Run: no run of mine, no results.
public enum WorkFaceKind: String, Equatable, Sendable, CaseIterable {
    case issue
    case run
    case changes
    case results
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
    public static let changesFaceLabel = "Changes"
    /// EXP-879: the run's published screenshots.
    public static let resultsFaceLabel = "Results"
    /// EXP-933: the inline `sessions_results` card's button that switches the
    /// Work screen to its Results face, byte-identical ×4.
    public static let openResultsLabel = "Open Results"
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
        case .changes: return changesFaceLabel
        case .results: return resultsFaceLabel
        }
    }

    /// The faces a subject can show, in their fixed order. Changes is
    /// independent of Run: an issue with an open PR and no run of mine still
    /// has its PR files. Results (EXP-879) goes LAST and is not: it belongs to
    /// the shown run's own row.
    public static func availableFaces(
        hasIssue: Bool, hasRun: Bool, hasChanges: Bool, hasResults: Bool
    ) -> [WorkFaceKind] {
        var faces: [WorkFaceKind] = []
        if hasIssue { faces.append(.issue) }
        if hasRun { faces.append(.run) }
        if hasChanges { faces.append(.changes) }
        if hasResults { faces.append(.results) }
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
    /// larger id) by ANY member with results. Fixture
    /// `session-results.json` `issueResultsRun` (×4).
    public static func issueResultsRun(
        _ rows: [CodingSessionEntity],
        issueId: String,
        boundId: String?,
        me: String?,
        now: Date
    ) -> CodingSessionEntity? {
        if let own = codingTarget(rows, issueId: issueId, boundId: boundId, me: me, now: now),
           hasSessionResults(own.results) {
            return own
        }
        return newest(rows.filter { $0.issueId == issueId && hasSessionResults($0.results) })
    }

    /// EXP-934: the header's `…` CONTEXT MENU (Share · Move to board · Unmark
    /// duplicate · Delete issue) belongs to the issue, so it shows on the
    /// ISSUE face alone. On Run, Changes and Results the trailing slot carries
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

    public enum RunBarTrailing: String, Equatable, Sendable {
        case merge
        case start
        case none
    }

    /// The Run face's bar keeps ONE circle on its right: Merge PR while the run's
    /// PR is open and mergeable from here, else Start coding once the shown run
    /// ended for good, else nothing.
    public static func runBarTrailing(canMerge: Bool, offerStart: Bool) -> RunBarTrailing {
        if canMerge { return .merge }
        if offerStart { return .start }
        return .none
    }

    /// Where a face lands when it vanishes under the reader (the diff cleared,
    /// the results went with the run row): changes → run → issue, and results
    /// the same way — both are the run's. `nil` = nothing left.
    public static func fallbackFace(
        shown: WorkFaceKind, available: [WorkFaceKind]
    ) -> WorkFaceKind? {
        if available.contains(shown) { return shown }
        let order: [WorkFaceKind] = switch shown {
        case .changes: [.run, .issue]
        case .results: [.run, .issue]
        case .run: [.issue]
        case .issue: []
        }
        return order.first { available.contains($0) } ?? available.first
    }

    /// The composer footer's model — the `model` config option's value, nil
    /// when the engine reported none or a blank (web `lib/agent-feed.ts`).
    public static func sessionModel(_ config: AgentSessionConfig?) -> String? {
        guard let value = config?.options.first(where: { $0.id == "model" })?.value,
              !value.isEmpty
        else { return nil }
        return value
    }

    /// The state dot's tone off the viewer phase — the `PhaseDot` rule: green
    /// while live, amber while it waits on a human (or went quiet), muted once
    /// it is over or paused. `connecting` says whether to pulse it.
    public static func phaseDotTone(
        live: Bool, connecting: Bool, awaitingInput: Bool, paused: Bool, stale: Bool
    ) -> (tone: SessionDotTone, connecting: Bool) {
        let connecting = !paused && connecting
        let tone: SessionDotTone = if !paused, live {
            (awaitingInput || stale) ? .needsInput : .running
        } else {
            .muted
        }
        return (tone, connecting)
    }
}
