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

    /// What the row is called: the issue's title, or — for a run with no issue
    /// at all — its action-name snapshot, else "Batch run" (the live rows'
    /// rule, applied to rows whose issue may no longer be synced).
    public static func title(_ session: CodingSessionEntity, issue: IssueEntity?) -> String {
        if issue == nil, session.issueId == nil {
            return session.actionName ?? "Batch run"
        }
        return issue?.title ?? "Untitled issue"
    }

    /// The row's caption: `<device> · <agent label> · ended by <who> · <rel
    /// time>`, with every unknown segment simply left out. `agent` is the
    /// RESOLVED label (iOS `LaunchVocabulary.agentLabel`) and `relativeTime`
    /// the caller's already-formatted stamp, so nothing here depends on a
    /// locale or a clock. Locked ×4 by the test
    /// `the past byline names device, agent and who ended it`.
    public static func byline(
        device: String, agent: String?, endedBy: String?, relativeTime: String
    ) -> String {
        var parts: [String] = []
        if !device.isEmpty { parts.append(device) }
        if let agent, !agent.isEmpty { parts.append(agent) }
        if let who = endedByPhrase(endedBy) { parts.append("ended by \(who)") }
        if !relativeTime.isEmpty { parts.append(relativeTime) }
        return parts.joined(separator: " · ")
    }

    /// `coding_sessions.ended_by` as the words the byline prints. An unknown
    /// or absent value drops the segment rather than inventing a culprit — an
    /// older row simply says less. Locked ×4.
    public static func endedByPhrase(_ endedBy: String?) -> String? {
        switch endedBy {
        case "agent": "agent"
        case "user": "you"
        case "client": "the app"
        case "merge": "a merge"
        case "system": "the system"
        default: nil
        }
    }
}
