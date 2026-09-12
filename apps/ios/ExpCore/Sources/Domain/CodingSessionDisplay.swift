import Foundation

/// EXP-214: how a LIVE coding session renders. The synced status alone is not
/// the whole story — `in_review` splits on the linked issue's PR outcome
/// (merged → the run is done, review otherwise, matching the issue-status
/// palette: review green, done blue), and the desktop-written `needs_input`
/// attention flag (agent parked on a plan-approval / AskUserQuestion picker)
/// marks a still-RUNNING session as an amber "Needs input". Callers
/// pass only sessions that already passed CodingSessionLiveness.
/// EXP-540: a PR merge ENDS the session (EXP-498), so a merged run leaves the
/// live set instead of parking in a status of its own. The `in_review` +
/// merged-PR arm stays as old-server tolerance: a lagging self-host server can
/// still leave a row in `in_review` after its PR merged.
/// EXP-531: `in_review` also outranks the needs-input flag — once the PR is
/// open the run is done coding, and claude's idle-nudge notification (which
/// the desktop forwards as needs_input) must not mask "Ready for review".
/// EXP-679: the server accepts the flag on every live status now (a
/// person-started run stays live after its PR, and the idle edge is "your
/// turn"), so this ordering is the ONLY mask — the nav dot goes through it too.
public enum CodingSessionDisplayState {
    case running
    case needsInput
    case review
    case done

    public static func of(
        session: CodingSessionEntity,
        prState: String?
    ) -> CodingSessionDisplayState {
        let merged = prState == DomainContract.prStateMerged
        if session.status == DomainContract.codingSessionStatusInReview {
            return merged ? .done : .review
        }
        if session.needsInput && !merged { return .needsInput }
        return .running
    }

    /// EXP-848: whether the dot PULSES — the row-level mirror of the
    /// in-session working predicate (`AgentFeed.working`), and the ONE rule
    /// every list, badge and header shares.
    ///
    /// `status = running` only ever meant "this row is live", so a run sitting
    /// idle between turns, parked on a question or walled by a rate limit drew
    /// a pulsing "coding now" it had no business drawing. The device-written
    /// `agent_busy` is the input instead; a row from before the column (or a
    /// machine too old to write it) simply never pulses, which is the honest
    /// degradation. `paused`/`live` are the callers' own narrowings — an
    /// offline host or a dead socket is never "coding now" whatever the row
    /// says. Mirrored ×4.
    public static func pulses(
        state: CodingSessionDisplayState,
        agentBusy: Bool,
        paused: Bool = false,
        live: Bool = true
    ) -> Bool {
        state == .running && agentBusy && !paused && live
    }
}
