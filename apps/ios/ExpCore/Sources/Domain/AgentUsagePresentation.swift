import Foundation

/// EXP-484: how a machine's per-agent auth status and rate-limit usage
/// present.
///
/// The device is the only writer: it probes its agent CLIs locally and reports
/// `agentAccounts` / `agentUsage` on register/heartbeat, and the synced
/// `devices` row is what every client draws. Nothing here fetches, refreshes
/// or holds a credential — this is read-only visibility.
///
/// Pure and hand-mirrored ×4 (web `lib/agent-usage.ts`, Android
/// `domain/AgentUsagePresentation.kt`, desktop `ui/src/usage_bar.rs`) against
/// the SAME fixture, so a bar reads identically on every client. Change a rule
/// here and the other three move with it.

/// How loud a usage percentage reads. Thresholds are locked ×4: ≥95 danger,
/// ≥75 warning, anything below normal.
public enum AgentUsageSeverity: String, Equatable, Sendable {
    case normal
    case warning
    case danger
}

/// One session's live usage: the agent it runs and that agent's report.
public struct SessionAgentUsage: Equatable, Sendable {
    public let agent: String
    public let usage: AgentUsage

    public init(agent: String, usage: AgentUsage) {
        self.agent = agent
        self.usage = usage
    }
}

/// One usage card: a titled bar with its percentage, tone and caption. Pure
/// presentation — `AgentUsagePresentation.usageGroups` is the only maker.
public struct UsageCard: Equatable, Sendable, Identifiable {
    /// The wire window key the card came from (`session`, `weekly`,
    /// `model:fable`, `credits`, …).
    public let key: String
    public let title: String
    /// Nil when the machine reported the window but no number for it.
    public let percent: Double?
    public let severity: AgentUsageSeverity
    /// Empty when there is nothing to say under the bar.
    public let caption: String

    public var id: String { key }

    public init(key: String, title: String, percent: Double?, severity: AgentUsageSeverity, caption: String) {
        self.key = key
        self.title = title
        self.percent = percent
        self.severity = severity
        self.caption = caption
    }
}

/// A titled run of cards. `key` is stable (`session` / `weekly` / `other`) so
/// a view can special-case one without matching on its title.
public struct UsageGroup: Equatable, Sendable, Identifiable {
    public let key: String
    public let title: String
    public let cards: [UsageCard]

    public var id: String { key }

    public init(key: String, title: String, cards: [UsageCard]) {
        self.key = key
        self.title = title
        self.cards = cards
    }

    init(key: String, title: String, windows: [AgentUsageWindow], now: Date) {
        self.init(
            key: key,
            title: title,
            cards: windows.map { window in
                UsageCard(
                    key: window.key,
                    title: AgentUsagePresentation.cardTitle(window),
                    percent: window.percent,
                    severity: AgentUsagePresentation.severity(window.percent),
                    caption: AgentUsagePresentation.cardCaption(window, now: now)
                )
            }
        )
    }
}

/// EXP-804: `coding_sessions.blocked` — the agent's usage wall as row state.
/// Every field is optional for the same reason the server's zod mirror is
/// `.nullish()` throughout: a newer device naming a window this build has no
/// name for must degrade that field, never fail the whole decode and leave a
/// walled run rendering healthy.
public struct CodingSessionBlocked: Decodable, Equatable, Sendable {
    public let kind: String?
    public let agent: String?
    public let window: String?
    public let resetsAt: String?
    public let since: String?

    public init(
        kind: String? = nil,
        agent: String? = nil,
        window: String? = nil,
        resetsAt: String? = nil,
        since: String? = nil
    ) {
        self.kind = kind
        self.agent = agent
        self.window = window
        self.resetsAt = resetsAt
        self.since = since
    }
}

public enum AgentUsagePresentation {
    /// Numbers older than this are not current enough to draw. Locked ×4.
    public static let freshWindow: TimeInterval = 15 * 60

    // MARK: - Parsing

    /// One agent's usage report from the stored jsonb string. Nil on absent or
    /// unparsable JSON — usage is simply not shown then, never guessed. A
    /// single malformed WINDOW is dropped rather than blanking the report
    /// (`AgentUsageWindow.init(from:)`, which also clamps the percentage).
    public static func parse(_ json: String?) -> AgentUsage? {
        guard let json, let data = jsonData(json) else { return nil }
        return try? JSONDecoder().decode(AgentUsage.self, from: data)
    }

    /// The whole `agentUsage` map (agent id → report) from the stored jsonb.
    public static func parseMap(_ json: String?) -> [String: AgentUsage]? {
        guard let json, let data = jsonData(json) else { return nil }
        return try? JSONDecoder().decode([String: AgentUsage].self, from: data)
    }

    /// The `agentAccounts` map (agent id → sign-in status) from the stored jsonb.
    public static func parseAccounts(_ json: String?) -> [String: AgentAccount]? {
        guard let json, let data = jsonData(json) else { return nil }
        return try? JSONDecoder().decode([String: AgentAccount].self, from: data)
    }

    /// EXP-804: the run's usage wall from the stored jsonb string. Nil on
    /// absent or unparsable JSON — a run is then simply not shown as blocked,
    /// never guessed.
    public static func parseBlocked(_ json: String?) -> CodingSessionBlocked? {
        guard let json, let data = jsonData(json) else { return nil }
        return try? JSONDecoder().decode(CodingSessionBlocked.self, from: data)
    }

    private static func jsonData(_ json: String) -> Data? {
        let trimmed = json.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return Data(trimmed.utf8)
    }

    // MARK: - Grouping (EXP-688)

    /// The reported windows as the CARDS a usage surface draws, in three fixed
    /// groups. There is no pinned/tracked window any more (EXP-688 deleted the
    /// concept ×4): every window the machine reported is shown, grouped the
    /// way Claude's own app groups them.
    ///
    /// Locked ×4 (web `usageGroups`, Android `usageGroups`, desktop
    /// `usage_groups`) against the same fixture:
    ///   - `session` → "Current session" / card title "Current session";
    ///   - `weekly` + `model:*` → NO title (EXP-694: the card titles already
    ///     say "All models" / "<Label> only", so the group header was noise),
    ///     the all-models window first;
    ///   - anything else (`credits`, codex's `43200`) → "Other" with the wire
    ///     label, in report order.
    /// Empty groups are omitted and the group order never varies. A group with
    /// an EMPTY title draws no header — every renderer skips it.
    public static func usageGroups(_ usage: AgentUsage, now: Date = Date()) -> [UsageGroup] {
        let windows = usage.windows ?? []
        let session = windows.filter { $0.key == sessionWindowKey }
        let weekly = windows.filter { $0.key == weeklyWindowKey }
            + windows.filter { $0.key.hasPrefix(modelWindowPrefix) }
        let other = windows.filter {
            $0.key != sessionWindowKey && $0.key != weeklyWindowKey
                && !$0.key.hasPrefix(modelWindowPrefix)
        }
        return [
            UsageGroup(key: "session", title: "Current session", windows: session, now: now),
            UsageGroup(key: "weekly", title: "", windows: weekly, now: now),
            UsageGroup(key: "other", title: "Other", windows: other, now: now),
        ].filter { !$0.cards.isEmpty }
    }

    // MARK: - Session context + spend (EXP-746)

    /// The heading over the run's own context/spend block in the Usage sheet.
    /// A SIBLING of `usageGroups`, never a group inside it: those cards are
    /// fixture-locked ×4 percentages of a rate-limit window, and a token count
    /// with no percent would draw an empty rail there.
    /// Byte-identical ×4 (web `agent-usage.ts` `CONTEXT_SECTION_TITLE`,
    /// Android `AgentUsagePresentation.kt`, desktop `ui/src/usage_bar.rs`).
    public static let contextSectionTitle = "Context"

    /// The run's context line — `"124k / 200k (62%)"`. Counts round to whole
    /// thousands from 1000 up, the percentage is FLOORED, and an unknown size
    /// prints nothing at all. Locked ×4 by the test
    /// `context usage reads used over size with a percent`.
    public static func formatContextUsage(used: Int, size: Int) -> String? {
        guard size > 0 else { return nil }
        let percent = min(100, max(0, Int((Double(max(0, used)) * 100 / Double(size)).rounded(.down))))
        return "\(formatTokens(max(0, used))) / \(formatTokens(size)) (\(percent)%)"
    }

    /// The run's spend — `"$1.24"`, two decimals. Nil under half a cent (and
    /// for an absent figure): an agent that reports `0.0009` has effectively
    /// spent nothing and "$0.00" reads as a bug. Locked ×4 by the test
    /// `a cost under half a cent renders nothing`.
    public static func formatUsageCost(_ cost: Double?) -> String? {
        guard let cost, cost >= 0.005 else { return nil }
        return String(format: "$%.2f", cost)
    }

    /// Whole thousands from 1000 up (`124000` → `124k`), the raw count below.
    private static func formatTokens(_ count: Int) -> String {
        guard count >= 1000 else { return "\(count)" }
        return "\(Int((Double(count) / 1000).rounded()))k"
    }

    /// The idle-session line Claude's own app shows: a session window at 0%
    /// with nothing to reset has not started yet. Locked ×4.
    public static let sessionNotStartedCaption = "Starts when a message is sent"

    static let sessionWindowKey = "session"
    static let weeklyWindowKey = "weekly"
    static let modelWindowPrefix = "model:"

    /// One card's title: the two named windows read as words, a per-model
    /// window as "<Label> only", and anything else keeps the wire label.
    static func cardTitle(_ window: AgentUsageWindow) -> String {
        if window.key == sessionWindowKey { return "Current session" }
        if window.key == weeklyWindowKey { return "All models" }
        if window.key.hasPrefix(modelWindowPrefix) { return "\(window.label) only" }
        return window.label
    }

    /// The line under a card's bar: the countdown when the window resets, the
    /// not-started sentence for an untouched session window, else nothing.
    static func cardCaption(_ window: AgentUsageWindow, now: Date) -> String {
        if let countdown = resetCountdown(resetsAt: window.resetsAt, now: now) { return countdown }
        if window.key == sessionWindowKey, window.percent == 0 { return sessionNotStartedCaption }
        return ""
    }

    /// Locked thresholds. An unreported percentage reads normal — absence is
    /// not an alarm.
    public static func severity(_ percent: Double?) -> AgentUsageSeverity {
        guard let percent else { return .normal }
        if percent >= 95 { return .danger }
        if percent >= 75 { return .warning }
        return .normal
    }

    // MARK: - Freshness

    /// Whether numbers fetched at [fetchedAt] are current enough to draw.
    /// FAIL-CLOSED like device liveness: an absent or unparsable stamp reads
    /// stale, because claiming an old percentage is current is the bad failure.
    /// A stamp in the future is clock skew, not staleness.
    public static func isFresh(fetchedAt: String?, now: Date = Date()) -> Bool {
        guard let fetchedAt, let fetched = WireTimestamps.parse(fetchedAt) else { return false }
        return now.timeIntervalSince(fetched) < freshWindow
    }

    /// `resets in 2h 10m` / `resets in 3d 14h` / `resets in 45m` / `resets
    /// soon` (under a minute, or already past). Nil when the window never
    /// resets or the stamp is unreadable — the strings are locked ×4, and a
    /// zero smaller unit is dropped (`resets in 2h`, never `resets in 2h 0m`).
    public static func resetCountdown(resetsAt: String?, now: Date = Date()) -> String? {
        guard let resetsAt, let reset = WireTimestamps.parse(resetsAt) else { return nil }
        let minutes = Int(reset.timeIntervalSince(now) / 60)
        guard minutes >= 1 else { return "resets soon" }
        let days = minutes / (60 * 24)
        let hours = (minutes / 60) % 24
        if days > 0 {
            return hours > 0 ? "resets in \(days)d \(hours)h" : "resets in \(days)d"
        }
        if hours > 0 {
            let rest = minutes % 60
            return rest > 0 ? "resets in \(hours)h \(rest)m" : "resets in \(hours)h"
        }
        return "resets in \(minutes)m"
    }

    /// EXP-804: the one-line badge for a run's usage wall — `Rate limited ·
    /// resets in 2h`, or bare `Rate limited` when the agent named no reset
    /// time. Nil when the run is not blocked.
    ///
    /// The wall is ORTHOGONAL to the session state: a blocked run still reads
    /// `running`, so this NEVER replaces `CodingSessionDisplay`'s state — it
    /// renders beside it. An unrecognised `kind` still gets a badge
    /// (`Blocked`): a future device reporting a wall this build has no name
    /// for must not render silent. Locked ×4 (web `blockedBadgeLabel`).
    public static func blockedBadgeLabel(
        _ blocked: CodingSessionBlocked?,
        now: Date = Date()
    ) -> String? {
        guard let blocked else { return nil }
        let label = (blocked.kind ?? "rate_limit") == "rate_limit"
            ? "Rate limited"
            : "Blocked"
        guard let countdown = resetCountdown(resetsAt: blocked.resetsAt, now: now) else {
            return label
        }
        return "\(label) · \(countdown)"
    }

    // MARK: - Accounts

    /// The caption after the agent name. Locked ×4 and, since EXP-694, as
    /// short as it can be: the bare `<email>` (no "signed in as" prefix, no
    /// " · <plan>" suffix — the row's context already says both) / the bare
    /// plan (pi, which reports a provider instead of an email) / `signed in` /
    /// `signed out`. A missing report is `unknown` — the device never probed,
    /// which is not "signed out".
    public static func accountCaption(_ account: AgentAccount?) -> String {
        guard let account else { return "unknown" }
        guard account.signedIn == true else { return "signed out" }
        if let email = account.email, !email.isEmpty { return email }
        if let plan = account.plan, !plan.isEmpty { return plan }
        return "signed in"
    }

    /// The whole row: `<agent> · <caption>`.
    public static func accountRow(agent: String, account: AgentAccount?) -> String {
        "\(agent) · \(accountCaption(account))"
    }

    // MARK: - Remote login

    /// The sign-in link an `agent_login` device command published, read off
    /// `device_commands.result`.
    ///
    /// The machine completes that command EARLY — the moment the URL (and,
    /// for codex, the device code) is on its screen — with the JSON body
    /// `{"agent":…,"phase":"url","url":…,"code":…}` (desktop
    /// `coding::agent_login::LoginProgress`). Nil for anything else: a failure
    /// sentence, a result from another command kind, or an older build's plain
    /// text, all of which the caller shows verbatim instead. The URL is
    /// scheme-checked here because the only thing a client does with it is
    /// hand it to the system opener.
    public static func parseAgentLoginResult(_ result: String?) -> (url: String, code: String?)? {
        guard let result, let data = jsonData(result) else { return nil }
        guard let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else {
            return nil
        }
        guard let raw = object["url"] as? String,
              let url = URL(string: raw),
              let scheme = url.scheme?.lowercased(),
              scheme == "https" || scheme == "http"
        else { return nil }
        let code = (object["code"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        return (raw, code)
    }

    // MARK: - Session join

    /// The usage bar a coding session shows, or nil when it shows none.
    ///
    /// Renders only for a run that is still going (`running` / `in_review`) on
    /// a machine whose report is FRESH and non-empty for the run's own agent —
    /// a finished run's host limits are nobody's business, and stale numbers
    /// beside a live agent read as current ones.
    ///
    /// The devices-row join mirrors `SessionDevicePresentation` exactly: the
    /// stamped `device_id`, preferring the session owner's own row (two users
    /// may see the same machine id through a shared server row).
    public static func sessionUsage(
        session: CodingSessionEntity,
        devices: [DeviceEntity],
        now: Date = Date()
    ) -> SessionAgentUsage? {
        guard session.status == DomainContract.codingSessionStatusRunning
            || session.status == DomainContract.codingSessionStatusInReview
        else { return nil }
        guard let agent = session.agent, !agent.isEmpty else { return nil }
        guard let row = matchedRow(session: session, devices: devices) else { return nil }
        guard let usage = parseMap(row.agentUsage)?[agent] else { return nil }
        guard isFresh(fetchedAt: usage.fetchedAt, now: now) else { return nil }
        guard let windows = usage.windows, !windows.isEmpty else { return nil }
        return SessionAgentUsage(agent: agent, usage: usage)
    }

    private static func matchedRow(
        session: CodingSessionEntity,
        devices: [DeviceEntity]
    ) -> DeviceEntity? {
        guard let deviceId = session.deviceId, !deviceId.isEmpty else { return nil }
        let byId = devices.filter { $0.deviceId == deviceId }
        return byId.first { $0.userId == session.userId } ?? byId.first
    }
}
