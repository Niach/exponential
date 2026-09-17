import Foundation

/// EXP-746: "Recent" (named "Past" until EXP-886) — the caller's own finished
/// runs under the Agent page's Running section, on all four clients.
///
/// This deliberately re-adds what EXP-676 removed (`ExpUI/EndedRunRow.swift`
/// documents that removal). The reason it comes back is a different one:
/// sessions are now first-class screens, so a finished run is where its
/// transcript and its Resume live — not a passive recap list. Automation runs
/// are still NOT here: `started_reason` is non-null on those and the
/// Automations tab's "Recent automated runs" stays their one home (EXP-676),
/// which is why the predicate names it explicitly.
///
/// Pure and mirrored ×4 (web `lib/past-runs.ts`, Android
/// `AgentsViewModel.pastRunRows` + its DAO query, desktop
/// `ui/src/queries.rs own_ended_runs`) — same predicate, same ordering key,
/// same cap, so the three lists hold the same rows.
public enum PastRuns {
    /// How many rows the section ever shows. Locked ×4.
    public static let cap = 20

    /// EXP-758: how many rows the SQL observation fetches — Android's
    /// `PAST_RUN_QUERY_LIMIT`. Wider than `cap` on purpose: the query is the
    /// bound that keeps an unscoped whole-history fetch off the main thread,
    /// while the extra rows leave `select` room to drop one without pulling a
    /// real row off the end.
    public static let queryLimit = 50

    /// EXP-888: the staleness sweep's end — `ended_by == "stale"`. The server
    /// flips a silent row to `ended` only when its host device advertises the
    /// `stale-end` cap; that device IGNORES the flip and its next heartbeat
    /// (up to 30 min later) revives the row to `running`. So a sweep end says
    /// "silent for the staleness window", NEVER "this run is over". ×4 (web
    /// `runIsStaleEnd`, desktop `run_rows::run_is_stale_end`, Android
    /// `runIsStaleEnd`).
    public static func isStaleEnd(_ session: CodingSessionEntity) -> Bool {
        session.status == DomainContract.codingSessionStatusEnded
            && session.endedBy == DomainContract.codingSessionEndedByStale
    }

    /// THE predicate: whether the run is OVER. `isLive` is its negation.
    /// Every Running-vs-Past, Stop-vs-Resume and composer-enabled decision
    /// goes through this and never through `status == "ended"` alone — or a
    /// swept-but-alive run lists as past, greys its composer out and offers a
    /// Resume that would put a SECOND agent on the same worktree.
    /// Byte-identical ×4 (web `runHasEnded`, desktop `run_rows::run_has_ended`,
    /// Android `runHasEnded`).
    public static func hasEnded(_ session: CodingSessionEntity) -> Bool {
        session.status == DomainContract.codingSessionStatusEnded && !isStaleEnd(session)
    }

    /// The negation of ``hasEnded(_:)`` — the run may still be alive. ×4.
    public static func isLive(_ session: CodingSessionEntity) -> Bool {
        !hasEnded(session)
    }

    /// The section's rows: own + active-team + `ended` + PERSON-started,
    /// newest first, capped. Ordering key is `ended_at ?? updated_at` — a row
    /// whose end never landed still sorts by its last heartbeat, and ISO-8601
    /// UTC strings compare lexicographically in time order.
    public static func select(
        _ sessions: [CodingSessionEntity],
        userId: String?,
        teamId: String?,
        cap limit: Int = cap
    ) -> [CodingSessionEntity] {
        sessions
            .filter { CodingSessionOwnership.isOwn($0, userId: userId, teamId: teamId) }
            // EXP-888: a sweep end is not an end — that row belongs to Running.
            .filter { hasEnded($0) }
            // EXP-676: an automation run belongs under Automations, never here.
            .filter { ($0.startedReason ?? "").isEmpty }
            .sorted { endedAt($0) > endedAt($1) }
            .prefix(limit)
            .map { $0 }
    }

    /// A run that is alive by STATUS — running or in review. Staleness is not
    /// consulted: a run whose machine went quiet is still one of the issue's
    /// runs, it only sorts by its last heartbeat.
    public static func isLiveRunStatus(_ status: String) -> Bool {
        status == DomainContract.codingSessionStatusRunning
            || status == DomainContract.codingSessionStatusInReview
    }

    /// The row-level twin of ``isLiveRunStatus(_:)``: live by status, plus
    /// EXP-888's sweep end, which is a live run whose heartbeat has not landed
    /// yet. Every list that paints a live badge or sorts live runs first reads
    /// THIS. ×4 (web `isLiveRun`, desktop `queries::is_live_run_status`,
    /// Android `isLiveRun`).
    public static func isLiveRun(_ session: CodingSessionEntity) -> Bool {
        isStaleEnd(session) || isLiveRunStatus(session.status)
    }

    /// The word the run switcher shows in a live run's time slot, in place of
    /// the ended relative time. Byte-identical ×4 (web `LIVE_RUN_LABEL`).
    public static let liveRunLabel = "Live"

    /// EXP-886: an issue's RUNS — the caller's OWN runs of THAT issue
    /// (`issue_id == issue AND user_id == me`), whatever their status and
    /// `started_reason` (a live run, an ended one, an automated one),
    /// UNCAPPED. Live runs first, then by the same `endedAt` key as Recent.
    /// Backs the session screen's run switcher. Batch runs carry no
    /// `issue_id`, so they never match. Mirrored ×4 (web `selectIssueRuns`,
    /// Android `issueRunRows`, desktop `issue_runs`).
    public static func issueRuns(
        _ sessions: [CodingSessionEntity],
        issueId: String?,
        userId: String?
    ) -> [CodingSessionEntity] {
        guard let issueId, !issueId.isEmpty else { return [] }
        return sessions
            .filter { $0.issueId == issueId }
            .filter { CodingSessionOwnership.isOwn($0, userId: userId) }
            .sorted { a, b in
                let liveA = isLiveRun(a)
                let liveB = isLiveRun(b)
                if liveA != liveB { return liveA }
                return endedAt(a) > endedAt(b)
            }
    }

    /// EXP-886: the switcher's `<when>` for a run — `liveRunLabel` for a
    /// live-status run, else the caller's already-formatted ended time. The
    /// entry is `byline(device:relativeTime:)` over it; the trigger names the
    /// run on show by it. ×4 (web `issueRunWhen`).
    public static func issueRunWhen(_ session: CodingSessionEntity, endedRelative: String) -> String {
        isLiveRun(session) ? liveRunLabel : endedRelative
    }

    /// When the run finished, for ordering and the byline's relative time.
    public static func endedAt(_ session: CodingSessionEntity) -> String {
        session.endedAt ?? session.updatedAt
    }

    /// What the row is called: the issue's title, else — while that issue row
    /// has not synced yet — "Issue syncing…", else the run's action-name
    /// snapshot (which outlives the action, and is how a chat run reads
    /// "Chat", EXP-615), else the batch's own name (EXP-876: its first covered
    /// issue's title, else "Batch run"). Byte-identical ×4 with web
    /// `pastRunTitle`, Android `pastRunTitle` and desktop `session_title`, so
    /// the same ended run is named the same everywhere. Locked by
    /// `a row titles itself from whatever it has`.
    public static func title(
        _ session: CodingSessionEntity,
        issue: IssueEntity?,
        batchIssues: [IssueEntity] = []
    ) -> String {
        if let issue {
            let title = issue.title.trimmingCharacters(in: .whitespacesAndNewlines)
            return title.isEmpty ? "Untitled issue" : title
        }
        if session.issueId != nil { return "Issue syncing…" }
        if let chat = chatSubject(session) { return chat }
        let action = (session.actionName ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        if !action.isEmpty { return action }
        return BatchRun.name(session, issues: batchIssues).subject
    }

    /// The hidden Chat builtin's name — the `action_name` snapshot every chat
    /// run carries (EXP-615; byte-identical with `ActionDto.builtinChatAction`).
    public static let chatRunName = "Chat"

    /// EXP-905: a CHAT run's subject — the agent CLI's own auto-title
    /// (`agent_title`, trimmed) when non-empty, else "Chat". nil for every
    /// other run (issue/action/batch keep their names). A chat run = no issue,
    /// no action row, `action_name` == "Chat". Mirrored ×4.
    public static func chatSubject(_ session: CodingSessionEntity) -> String? {
        guard session.issueId == nil, session.actionId == nil,
              session.actionName == chatRunName else { return nil }
        let title = (session.agentTitle ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        return title.isEmpty ? chatRunName : title
    }

    /// EXP-876: the row's mono lead-in — the issue's identifier, a batch's
    /// `EXP-874 +2`, else none. The twin of `title`, ×4 lockstep (web
    /// `pastRunIdentifier`).
    public static func identifier(
        _ session: CodingSessionEntity,
        issue: IssueEntity?,
        batchIssues: [IssueEntity] = []
    ) -> String? {
        if let issue { return issue.identifier }
        if session.issueId != nil || session.actionName != nil { return nil }
        return BatchRun.name(session, issues: batchIssues).identifier
    }

    /// The row's caption: `<device> · <rel time>`, with an empty segment
    /// simply left out. `relativeTime` is the caller's already-formatted
    /// stamp, so nothing here depends on a locale or a clock. EXP-833 dropped
    /// the agent label and the "ended by" clause: the right side had grown
    /// wider than the titles, and the agent already shows as the row's lead
    /// glyph. Locked ×4 by the test
    /// `the past byline names the device and when it ended`.
    public static func byline(device: String, relativeTime: String) -> String {
        var parts: [String] = []
        if !device.isEmpty { parts.append(device) }
        if !relativeTime.isEmpty { parts.append(relativeTime) }
        return parts.joined(separator: " · ")
    }
}
