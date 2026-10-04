import Foundation

/// EXP-1184 (EXP-214/531/848 before it): how a LIVE coding session renders —
/// ONE rule ×4, locked by `packages/domain-contract/fixtures/session-display.json`
/// (web `lib/coding-session-display.ts`, desktop
/// `queries::coding_session_display`, Android `CodingSessionDisplay.kt`).
/// First match wins:
/// - `needsInput`: the run waits on a person — on every live status, an open
///   PR included (the old EXP-531 "in_review masks needs input" rule is gone:
///   the server clears the flag on every turn start and every PR park).
/// - `working`: the agent is mid-turn (the device-written `agent_busy`, or the
///   steering screen's own working signal) — a follow-up turn on an
///   `in_review` run included.
/// - `review`: `in_review` with the PR neither merged nor closed.
/// - `done`: idle with no open PR (none, closed, or merged).
/// The session row's status never changes for this. A paused (offline) run
/// and an ended row are the caller's to decide, before this rule.
public enum CodingSessionDisplayState: String, Sendable {
    case working
    case needsInput = "needs_input"
    case review
    case done

    /// `agentBusy` overrides the synced `agent_busy` — the Work screen passes
    /// its viewer's live working signal; lists leave it nil.
    public static func of(
        session: CodingSessionEntity,
        prState: String?,
        agentBusy: Bool? = nil
    ) -> CodingSessionDisplayState {
        of(
            status: session.status,
            needsInput: session.needsInput,
            agentBusy: agentBusy ?? session.agentBusy,
            prState: prState
        )
    }

    public static func of(
        status: String,
        needsInput: Bool,
        agentBusy: Bool,
        prState: String?
    ) -> CodingSessionDisplayState {
        if needsInput { return .needsInput }
        if agentBusy { return .working }
        let prOpen = prState != DomainContract.prStateMerged && prState != DomainContract.prStateClosed
        return status == DomainContract.codingSessionStatusInReview && prOpen ? .review : .done
    }

    /// EXP-848: whether the row ANIMATES (the working mark) — the agent is
    /// executing a turn right now on a row that is still live; an ended row
    /// never does, whatever the flag says. `paused`/`live` are the callers'
    /// own narrowings — an offline host or a dead socket is never "coding
    /// now" whatever the row says. Same fixture as `of`.
    public static func working(
        status: String,
        state: CodingSessionDisplayState,
        paused: Bool = false,
        live: Bool = true
    ) -> Bool {
        status != DomainContract.codingSessionStatusEnded && state == .working && !paused && live
    }

    /// The tone the list row's status line paints in (fixture `statusTone`).
    public var statusTone: SessionStatusTone {
        switch self {
        case .working: .muted
        case .needsInput: .amber
        case .review: .emerald
        case .done: .sky
        }
    }
}

/// The tone a session row's status line paints in (web `SessionStatusTone`).
public enum SessionStatusTone: String, Sendable {
    case muted
    case amber
    case emerald
    case sky
}
