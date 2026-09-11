import Foundation

/// EXP-746: "Past" — the caller's own finished runs under the Devices screen's
/// Running section, on all four clients.
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
            .filter { $0.status == DomainContract.codingSessionStatusEnded }
            // EXP-676: an automation run belongs under Automations, never here.
            .filter { ($0.startedReason ?? "").isEmpty }
            .sorted { endedAt($0) > endedAt($1) }
            .prefix(limit)
            .map { $0 }
    }

    /// When the run finished, for ordering and the byline's relative time.
    public static func endedAt(_ session: CodingSessionEntity) -> String {
        session.endedAt ?? session.updatedAt
    }

    /// What the row is called: the issue's title, else — while that issue row
    /// has not synced yet — "Issue syncing…", else the run's action-name
    /// snapshot (which outlives the action, and is how a chat run reads
    /// "Chat", EXP-615), else "Batch run". Byte-identical ×4 with web
    /// `pastRunTitle`, Android `pastRunTitle` and desktop `session_title`, so
    /// the same ended run is named the same everywhere. Locked by
    /// `a row titles itself from whatever it has`.
    public static func title(_ session: CodingSessionEntity, issue: IssueEntity?) -> String {
        if let issue {
            let title = issue.title.trimmingCharacters(in: .whitespacesAndNewlines)
            return title.isEmpty ? "Untitled issue" : title
        }
        if session.issueId != nil { return "Issue syncing…" }
        let action = (session.actionName ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        return action.isEmpty ? "Batch run" : action
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
