import Foundation

// EXP-893: the PHONE's Work screen — one screen per subject (an issue, or a
// session) with up to three FACES held as screen state, never as navigation:
// `issue`, `run` and `changes`. The desktop's face toggle (EXP-877) becomes a
// floating bottom-right circle that either switches straight to the one other
// face or opens a menu above itself; Stop / Resume sit in the nav bar's
// trailing slot only while the Run face shows. These are the PURE rules every
// phone client mirrors byte for byte: web `lib/work-faces.ts` (the spec),
// Android `domain/WorkFaces.kt` — same names, same cases, same test names.

/// The three faces. `changes` is the run's diff, else the issue's open PR.
public enum WorkFaceKind: String, Equatable, Sendable, CaseIterable {
    case issue
    case run
    case changes
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
    /// The switcher menu's extra row once the shown run ended for good.
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
        }
    }

    /// The faces a subject can show, in their fixed order. Changes is
    /// independent of Run: an issue with an open PR and no run of mine still
    /// has its PR files.
    public static func availableFaces(
        hasIssue: Bool, hasRun: Bool, hasChanges: Bool
    ) -> [WorkFaceKind] {
        var faces: [WorkFaceKind] = []
        if hasIssue { faces.append(.issue) }
        if hasRun { faces.append(.run) }
        if hasChanges { faces.append(.changes) }
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

    public enum PrimaryAction: Equatable, Sendable {
        case stop
        case resume
        case start
        case none
    }

    /// The nav bar's trailing verb on the Run face, and the bottom-right
    /// circle's start glyph: Stop wins over everything; Resume needs an ended
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

    public enum SwitcherTarget: Equatable, Sendable, Hashable {
        case face(WorkFaceKind)
        case run(id: String)
        case startCoding
    }

    /// What the switcher circle offers from the shown face: the OTHER faces in
    /// order, the Run face expanded into one row per own run when there are
    /// two or more (EXP-886), and `Start coding` first when the shown run
    /// ended and cannot be resumed (desktop shows Start in that state; on the
    /// phone the circle is the switcher, so the menu carries it).
    public static func switcherTargets(
        faces: [WorkFaceKind],
        shown: WorkFaceKind,
        runIds: [String],
        shownRunId: String?,
        offerStart: Bool
    ) -> [SwitcherTarget] {
        var targets: [SwitcherTarget] = []
        if offerStart { targets.append(.startCoding) }
        for face in faces {
            if face == .run, runIds.count >= 2 {
                for id in runIds {
                    if shown == .run, id == shownRunId { continue }
                    targets.append(.run(id: id))
                }
                continue
            }
            if face == shown { continue }
            targets.append(.face(face))
        }
        return targets
    }

    public enum SwitcherMode: Equatable, Sendable {
        case hidden
        case toggle(SwitcherTarget)
        case menu([SwitcherTarget])
    }

    /// No target = no circle; exactly one = a direct switch wearing the
    /// destination's icon; two or more = the faces glyph and a menu above.
    public static func switcherMode(_ targets: [SwitcherTarget]) -> SwitcherMode {
        if targets.isEmpty { return .hidden }
        if targets.count == 1 { return .toggle(targets[0]) }
        return .menu(targets)
    }

    public enum SwitcherBadge: Equatable, Sendable {
        case tone(SessionDotTone)
        case changes
    }

    /// The circle's badge dot: off the Run face the shown session's state dot
    /// (none without a session); on the Run face a green dot while changes
    /// exist, so the reader knows a diff is waiting behind the switcher.
    public static func switcherBadge(
        shown: WorkFaceKind, sessionTone: SessionDotTone?, hasChanges: Bool
    ) -> SwitcherBadge? {
        if shown == .run { return hasChanges ? .changes : nil }
        return sessionTone.map { .tone($0) }
    }

    /// Where a face lands when it vanishes under the reader (the diff cleared,
    /// the run row went): changes → run → issue. `nil` = nothing left.
    public static func fallbackFace(
        shown: WorkFaceKind, available: [WorkFaceKind]
    ) -> WorkFaceKind? {
        if available.contains(shown) { return shown }
        let order: [WorkFaceKind] = switch shown {
        case .changes: [.run, .issue]
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
