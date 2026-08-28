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

    private static func jsonData(_ json: String) -> Data? {
        let trimmed = json.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return Data(trimmed.utf8)
    }

    // MARK: - Selection

    /// The window a bar shows: the caller's remembered key when the report
    /// still carries it, else the fullest one (a window with no percentage
    /// never wins). Nil when there is nothing to draw.
    public static func selectWindow(
        _ usage: AgentUsage?,
        preferredKey: String? = nil
    ) -> AgentUsageWindow? {
        guard let windows = usage?.windows, !windows.isEmpty else { return nil }
        if let preferredKey, let pinned = windows.first(where: { $0.key == preferredKey }) {
            return pinned
        }
        return windows.max { ($0.percent ?? -1) < ($1.percent ?? -1) }
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

    // MARK: - Accounts

    /// The caption after the agent name. Locked ×4: `signed in as <email> ·
    /// <plan>` / `signed in as <email>` / the bare plan (pi, which reports a
    /// provider instead of an email) / `signed in` / `signed out`. A missing
    /// report is `unknown` — the device never probed, which is not "signed
    /// out".
    public static func accountCaption(_ account: AgentAccount?) -> String {
        guard let account else { return "unknown" }
        guard account.signedIn == true else { return "signed out" }
        if let email = account.email, !email.isEmpty {
            if let plan = account.plan, !plan.isEmpty {
                return "signed in as \(email) · \(plan)"
            }
            return "signed in as \(email)"
        }
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
