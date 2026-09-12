import Foundation

// Mirrors apps/web/src/lib/trpc/steer.ts (the ticket-minting router) + the relay
// wire contract in apps/steer-relay/src/protocol.ts. The steer relay is the
// data-plane for the live activity channel (Electric can't carry it). The desktop
// mints a short-lived HS256 relay ticket per socket via tRPC, then dials the
// relay outbound (`wss://<relay>/ws?ticket=<token>`). `STEER_RELAY_URL` unset ⇒
// the subsystem reports disabled and the desktop opens no sockets (graceful-off).

/// Whether remote start + live steering is available on this instance.
public struct SteerConfig: Decodable, Sendable {
    public let enabled: Bool
    public let relayUrl: String?

    public init(enabled: Bool, relayUrl: String?) {
        self.enabled = enabled
        self.relayUrl = relayUrl
    }
}

/// A minted relay ticket + the wss URL to dial. `disabled == true` (or a nil
/// ticket/url) means the subsystem is off on this instance.
public struct SteerTicket: Decodable, Sendable {
    public let ticket: String?
    public let url: String?
    public let disabled: Bool?

    public init(ticket: String?, url: String?, disabled: Bool?) {
        self.ticket = ticket
        self.url = url
        self.disabled = disabled
    }

    public var isDisabled: Bool { disabled == true || ticket == nil || url == nil }

    /// Dial URL — the server returns `url` as the full
    /// `ws(s)://<relay>/ws?ticket=<token>` (the relay reads the ticket from the
    /// query string; browsers can't set WS headers and the desktop mirrors that).
    /// Appends `ticket` only when the server URL doesn't already carry one.
    public func connectURL() -> URL? {
        guard let url, let ticket, var comps = URLComponents(string: url) else { return nil }
        var items = comps.queryItems ?? []
        if !items.contains(where: { $0.name == "ticket" }) {
            items.append(URLQueryItem(name: "ticket", value: ticket))
            comps.queryItems = items
        }
        return comps.url
    }
}

/// EXP-432: the owner of a machine SHARED with the team — set only on rows
/// that are not the caller's own, so the lists/pickers can attribute them.
public struct DeviceOwner: Decodable, Sendable {
    public let id: String
    public let name: String

    public init(id: String, name: String) {
        self.id = id
        self.name = name
    }
}

/// EXP-437: one agent's saved launch options on a machine, as advertised on
/// the relay presence row. `model`/`effort` are always sent — `""` means "CLI
/// default / omit the flag", which is a real choice, not an absence — while
/// `ultracode`/`planMode` are serialized only when true (absent = false).
/// Everything is optional here anyway: the sender is a desktop of unknown
/// vintage, and an older one may still send the EXP-690-removed skip-
/// permissions key — Codable ignores unknown keys.
/// Equatable so a settings surface can diff the live row against its drafts
/// (EXP-490: the iOS device-settings sheet re-seeds off `onChange`).
public struct AgentLaunchDefaults: Decodable, Equatable, Sendable {
    public let model: String?
    public let effort: String?
    public let ultracode: Bool?
    public let planMode: Bool?

    public init(
        model: String? = nil,
        effort: String? = nil,
        ultracode: Bool? = nil,
        planMode: Bool? = nil
    ) {
        self.model = model
        self.effort = effort
        self.ultracode = ultracode
        self.planMode = planMode
    }
}

/// EXP-437: the coding defaults a machine advertises, so a remote Start-coding
/// sheet opens on THAT machine's settings instead of whatever the phone last
/// sent. `agents` covers the RUNNABLE agents only (contract `codingAgent`
/// ids). Absent entirely on an older desktop — every reader falls back to the
/// static contract defaults.
///
/// EXP-773 dropped `startInTerminal`: the PTY coding path is gone. Decoding
/// ignores unknown keys, so a row an older server still stamps it onto keeps
/// parsing.
public struct DeviceLaunchDefaults: Decodable, Equatable, Sendable {
    /// The machine's configured default agent. Clamped to what it actually
    /// runs by the reader — a signed-out default must not preselect.
    public let defaultAgent: String?
    public let agents: [String: AgentLaunchDefaults]?

    public init(
        defaultAgent: String? = nil,
        agents: [String: AgentLaunchDefaults]? = nil
    ) {
        self.defaultAgent = defaultAgent
        self.agents = agents
    }
}

/// EXP-484: one coding agent's sign-in status on a machine, as the device
/// reported it (`devices.agent_accounts[agent]`). READ-ONLY visibility — no
/// credential ever leaves the machine. `plan` is the subscription tier for
/// claude/codex and `"<provider> (oauth|api key)"` for an agent with no email.
/// Every field optional: the sender is a desktop/daemon of unknown vintage and
/// the server clamps rather than rejects.
public struct AgentAccount: Decodable, Equatable, Sendable {
    public let signedIn: Bool?
    public let email: String?
    public let plan: String?
    /// When the device last probed the agent.
    public let checkedAt: String?
    /// EXP-849: how the machine's last probe went — the raw wire value
    /// (`ok` / `needs_relogin` / `signed_out` / `unknown`, server-clamped to
    /// exactly those four). Read it through
    /// `AgentAccountHealth.resolve(_:signedIn:)`, never directly: an absent
    /// field DERIVES from `signedIn` and an unknown string degrades to
    /// `unknown` rather than rendering a raw token.
    public let health: String?
    /// EXP-825: the agent's login PROFILES on the machine (web
    /// `agentAccounts[agent].profiles`) — the Account picker offers them
    /// when there are two or more. Decoded LENIENTLY: a profile entry of a
    /// shape this build does not know must not throw the whole accounts map
    /// away (nil = the device reported none).
    public let profiles: [AgentAccountProfile]?

    public init(
        signedIn: Bool? = nil,
        email: String? = nil,
        plan: String? = nil,
        checkedAt: String? = nil,
        profiles: [AgentAccountProfile]? = nil,
        health: String? = nil
    ) {
        self.signedIn = signedIn
        self.email = email
        self.plan = plan
        self.checkedAt = checkedAt
        self.profiles = profiles
        self.health = health
    }

    private enum CodingKeys: String, CodingKey {
        case signedIn, email, plan, checkedAt, profiles, health
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        signedIn = try c.decodeIfPresent(Bool.self, forKey: .signedIn)
        email = try c.decodeIfPresent(String.self, forKey: .email)
        plan = try c.decodeIfPresent(String.self, forKey: .plan)
        checkedAt = try c.decodeIfPresent(String.self, forKey: .checkedAt)
        profiles = (try? c.decodeIfPresent([AgentAccountProfile].self, forKey: .profiles)) ?? nil
        health = try? c.decodeIfPresent(String.self, forKey: .health)
    }
}

/// EXP-825: one login profile of an agent on a machine (`id` is what a start
/// sends as `account`; `active` marks the machine's current login; `email`
/// names it). Every field but the id optional — the sender's vintage varies.
public struct AgentAccountProfile: Decodable, Equatable, Sendable, Identifiable {
    public let id: String
    /// EXP-829: the profile's display name (`Default` for the ambient login
    /// when the device sent none — `AgentAccountsRows` applies that fallback).
    public let label: String?
    public let active: Bool?
    /// EXP-829: the profile's OWN sign-in state, plan, probe stamp and usage
    /// report (web `DeviceAgentProfileEntry`). The Accounts section reads one
    /// row per profile off these; a pre-profile device carries none.
    public let signedIn: Bool?
    public let email: String?
    public let plan: String?
    public let checkedAt: String?
    public let usage: AgentUsage?
    /// EXP-849: this profile's own probe outcome — same four wire values as
    /// `AgentAccount.health`, read through
    /// `AgentAccountHealth.resolve(_:signedIn:)`.
    public let health: String?

    public init(
        id: String,
        label: String? = nil,
        active: Bool? = nil,
        signedIn: Bool? = nil,
        email: String? = nil,
        plan: String? = nil,
        checkedAt: String? = nil,
        usage: AgentUsage? = nil,
        health: String? = nil
    ) {
        self.id = id
        self.label = label
        self.active = active
        self.signedIn = signedIn
        self.email = email
        self.plan = plan
        self.checkedAt = checkedAt
        self.usage = usage
        self.health = health
    }

    private enum CodingKeys: String, CodingKey {
        case id, label, active, signedIn, email, plan, checkedAt, usage, health
    }

    /// Only the id is load-bearing; every other field degrades on its own so
    /// a newer device's profile entry never throws the whole list away.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        label = try? c.decodeIfPresent(String.self, forKey: .label)
        active = try? c.decodeIfPresent(Bool.self, forKey: .active)
        signedIn = try? c.decodeIfPresent(Bool.self, forKey: .signedIn)
        email = try? c.decodeIfPresent(String.self, forKey: .email)
        plan = try? c.decodeIfPresent(String.self, forKey: .plan)
        checkedAt = try? c.decodeIfPresent(String.self, forKey: .checkedAt)
        usage = try? c.decodeIfPresent(AgentUsage.self, forKey: .usage)
        health = try? c.decodeIfPresent(String.self, forKey: .health)
    }
}

/// EXP-484: one rate-limit window of an agent's usage report. `key` is the
/// stable identity (`session`, `weekly`, `model:<name>`, `credits`, a duration
/// in minutes) the per-client window preference is stored under; `label` is
/// what the bar prints. `percent` is 0-100 (server-clamped, and clamped again
/// on decode) and nil when the agent reported none.
public struct AgentUsageWindow: Decodable, Equatable, Sendable, Identifiable {
    public let key: String
    public let label: String
    public let percent: Double?
    /// When the window rolls over; nil for windows that never reset.
    public let resetsAt: String?

    public var id: String { key }

    public init(key: String, label: String, percent: Double? = nil, resetsAt: String? = nil) {
        self.key = key
        self.label = label
        self.percent = percent
        self.resetsAt = resetsAt
    }

    enum CodingKeys: String, CodingKey {
        case key, label, percent, resetsAt
    }

    /// Tolerant like the web/Android mirrors: only `key` is load-bearing (it
    /// is what a window is selected and remembered by), everything else
    /// degrades. The percentage is clamped HERE so every path that decodes a
    /// device's report — the presentation parsers and the `SteerDevice`
    /// mapping alike — can trust 0-100.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        key = try c.decode(String.self, forKey: .key)
        label = (try? c.decode(String.self, forKey: .label)) ?? ""
        // SE-0230 flattens the try?-of-optional; a non-finite number is no
        // percentage at all.
        if let raw = try? c.decodeIfPresent(Double.self, forKey: .percent), raw.isFinite {
            percent = min(max(raw, 0), 100)
        } else {
            percent = nil
        }
        resetsAt = try? c.decodeIfPresent(String.self, forKey: .resetsAt)
    }
}

/// A window that never throws: a malformed entry is dropped instead of
/// blanking the whole agent's report (matching the web/Android parsers).
private struct FailableUsageWindow: Decodable {
    let value: AgentUsageWindow?

    init(from decoder: Decoder) throws {
        value = try? AgentUsageWindow(from: decoder)
    }
}

/// EXP-484: one agent's usage report (`devices.agent_usage[agent]`).
/// `fetchedAt` is when the DEVICE fetched the numbers — the freshness gate
/// every renderer applies — and `stale` marks numbers the device kept after a
/// failed refresh.
public struct AgentUsage: Decodable, Equatable, Sendable {
    public let fetchedAt: String?
    public let stale: Bool?
    public let windows: [AgentUsageWindow]?

    public init(fetchedAt: String? = nil, stale: Bool? = nil, windows: [AgentUsageWindow]? = nil) {
        self.fetchedAt = fetchedAt
        self.stale = stale
        self.windows = windows
    }

    enum CodingKeys: String, CodingKey {
        case fetchedAt, stale, windows
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        fetchedAt = try? c.decodeIfPresent(String.self, forKey: .fetchedAt)
        stale = try? c.decodeIfPresent(Bool.self, forKey: .stale)
        windows = (try? c.decodeIfPresent([FailableUsageWindow].self, forKey: .windows))
            .map { $0.compactMap(\.value) }
    }
}

/// One machine: a registry row (EXP-403 — desktops and headless
/// `exponential` daemon servers, online or not, synced through the devices
/// shape) or a bare relay-presence row. ONE shape for both, mirroring
/// apps/web/src/lib/steer-devices.ts: the registry fields are optional and an
/// absent `online` reads as online, because a presence row is alive by
/// construction. Usually the caller's own — but teammates' shared
/// servers (EXP-432) ride along too, told apart by `owner`.
public struct SteerDevice: Decodable, Sendable, Identifiable {
    public let deviceId: String
    public let deviceLabel: String
    /// Relay presence timestamp — absent on registry rows.
    public let connectedAt: Double?
    /// Coding agents this desktop can RUN — installed AND signed in since
    /// EXP-409 (contract `codingAgentValues`). Absent = an older desktop that
    /// only runs claude; explicitly EMPTY = nothing runnable right now.
    public let agents: [String]?
    /// EXP-409: agents installed on the machine but SIGNED OUT, so unusable.
    /// Never offered in a picker — surfaced as a "not signed in" reason.
    public let unauthedAgents: [String]?
    /// EXP-749: the subset of `agents` the machine's ACP engine can drive.
    /// Every build above the desktop/CLI floor reports it, so the
    /// "absent = assume every runnable agent" fallback is gone. The column is
    /// still NULLABLE server-side (a stale row from a build that never
    /// registered again), and a NULL decodes to EMPTY: nothing starts there.
    /// EXP-773 deleted the PTY fallback, so an agent outside this list cannot
    /// start at all. Never a filter: read it through `agentNotReady(_:)`.
    public let acpAgents: [String]
    /// Feature capabilities the desktop advertised (EXP-253: `actions`).
    /// Absent (old desktop/relay) = none — action starts are strictly gated
    /// on this, unlike the lenient agents fallback.
    public let caps: [String]?
    /// EXP-403 registry fields (registry rows only).
    /// `desktop` | `server`; absent on relay-only rows (always a desktop).
    public let kind: String?
    public let platform: String?
    public let online: Bool?
    /// ISO timestamp of the last register/heartbeat; nil for relay-only rows.
    public let lastSeenAt: String?
    /// Whether a durable registry row backs this machine — an old desktop
    /// build shows up from relay presence alone and can't be renamed/removed.
    public let registered: Bool?
    /// Marketing version as of the last register; nil for old builds.
    public let version: String?
    /// An Update request is pending; the daemon consumes it by re-registering.
    public let updateRequested: Bool?
    /// EXP-411: the pending update is parked behind live coding sessions —
    /// the daemon applies it once they close ("Update queued", no spinner).
    public let updateBlocked: Bool?
    /// EXP-432/FEED-33: the teams this machine is shared with (empty =
    /// private). Carried on the caller's OWN rows too, so it can never stand
    /// in for `owner`.
    public let sharedTeamIds: [String]
    /// EXP-432: set ONLY on a teammate's shared row — the machine's owner.
    /// Absent on the caller's own rows, which is exactly what `isMine` reads.
    public let owner: DeviceOwner?
    /// EXP-622: the caller's DEFAULT machine — pickers prefill it over their
    /// first candidate. Set only on the caller's own rows: the flag lives on
    /// the device row and belongs to its owner, so a teammate's shared server
    /// never prefills off it.
    public let isDefault: Bool?
    /// EXP-484: per-agent sign-in status, keyed by contract `codingAgent`.
    /// Absent on an older desktop and on machines that reported nothing —
    /// never confuse "not reported" with "signed out".
    public let agentAccounts: [String: AgentAccount]?
    /// EXP-484: per-agent rate-limit usage, keyed by contract `codingAgent`.
    /// Read it through `AgentUsagePresentation`, which applies the freshness
    /// gate — numbers older than the window must not be drawn as current.
    public let agentUsage: [String: AgentUsage]?
    /// EXP-484: when the server last stored `agentUsage`.
    public let agentUsageAt: String?
    /// EXP-437: this machine's own per-agent coding defaults, so picking it in
    /// a remote Start-coding sheet pre-fills its settings. Absent on an older
    /// desktop (and on a machine with nothing runnable) — read it through
    /// `defaultLaunchAgent` / `agentDefaults(for:)`, never raw.
    public let launchDefaults: DeviceLaunchDefaults?
    /// EXP-481: the synced devices ROW id — joins `device_worktrees` for the
    /// resume probe. Set only when the value came off the devices shape
    /// (DeviceRows mapping); nil on tRPC/relay rows, which never resume.
    public let rowId: String?

    public var id: String { deviceId }

    public init(
        deviceId: String,
        deviceLabel: String,
        connectedAt: Double? = nil,
        agents: [String]? = nil,
        unauthedAgents: [String]? = nil,
        acpAgents: [String] = [],
        caps: [String]? = nil,
        kind: String? = nil,
        platform: String? = nil,
        online: Bool? = nil,
        lastSeenAt: String? = nil,
        registered: Bool? = nil,
        version: String? = nil,
        updateRequested: Bool? = nil,
        updateBlocked: Bool? = nil,
        sharedTeamIds: [String] = [],
        owner: DeviceOwner? = nil,
        isDefault: Bool? = nil,
        agentAccounts: [String: AgentAccount]? = nil,
        agentUsage: [String: AgentUsage]? = nil,
        agentUsageAt: String? = nil,
        launchDefaults: DeviceLaunchDefaults? = nil,
        rowId: String? = nil
    ) {
        self.deviceId = deviceId
        self.deviceLabel = deviceLabel
        self.connectedAt = connectedAt
        self.agents = agents
        self.unauthedAgents = unauthedAgents
        self.acpAgents = acpAgents
        self.caps = caps
        self.kind = kind
        self.platform = platform
        self.online = online
        self.lastSeenAt = lastSeenAt
        self.registered = registered
        self.version = version
        self.updateRequested = updateRequested
        self.updateBlocked = updateBlocked
        self.sharedTeamIds = sharedTeamIds
        self.owner = owner
        self.isDefault = isDefault
        self.agentAccounts = agentAccounts
        self.agentUsage = agentUsage
        self.agentUsageAt = agentUsageAt
        self.launchDefaults = launchDefaults
        self.rowId = rowId
    }

    private enum CodingKeys: String, CodingKey {
        case deviceId, deviceLabel, connectedAt, agents, unauthedAgents, acpAgents, caps
        case kind, platform, online, lastSeenAt, registered, version, updateRequested
        case updateBlocked, sharedTeamIds, owner, isDefault, agentAccounts, agentUsage
        case agentUsageAt, launchDefaults, rowId
    }

    // Hand-written only because `sharedTeamIds` and `acpAgents` are
    // non-optional: a relay presence row and a pre-FEED-33 server omit the
    // former, and a device row written before EXP-749 carries a NULL
    // `acp_agents`. Both must default, never drop the row. Everything else is
    // decodeIfPresent exactly as the synthesized decoder did it.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        deviceId = try c.decode(String.self, forKey: .deviceId)
        deviceLabel = try c.decode(String.self, forKey: .deviceLabel)
        connectedAt = try c.decodeIfPresent(Double.self, forKey: .connectedAt)
        agents = try c.decodeIfPresent([String].self, forKey: .agents)
        unauthedAgents = try c.decodeIfPresent([String].self, forKey: .unauthedAgents)
        acpAgents = try c.decodeIfPresent([String].self, forKey: .acpAgents) ?? []
        caps = try c.decodeIfPresent([String].self, forKey: .caps)
        kind = try c.decodeIfPresent(String.self, forKey: .kind)
        platform = try c.decodeIfPresent(String.self, forKey: .platform)
        online = try c.decodeIfPresent(Bool.self, forKey: .online)
        lastSeenAt = try c.decodeIfPresent(String.self, forKey: .lastSeenAt)
        registered = try c.decodeIfPresent(Bool.self, forKey: .registered)
        version = try c.decodeIfPresent(String.self, forKey: .version)
        updateRequested = try c.decodeIfPresent(Bool.self, forKey: .updateRequested)
        updateBlocked = try c.decodeIfPresent(Bool.self, forKey: .updateBlocked)
        sharedTeamIds = c.decodeWireStringList(forKey: .sharedTeamIds)
        owner = try c.decodeIfPresent(DeviceOwner.self, forKey: .owner)
        isDefault = try c.decodeIfPresent(Bool.self, forKey: .isDefault)
        agentAccounts = try c.decodeIfPresent([String: AgentAccount].self, forKey: .agentAccounts)
        agentUsage = try c.decodeIfPresent([String: AgentUsage].self, forKey: .agentUsage)
        agentUsageAt = try c.decodeIfPresent(String.self, forKey: .agentUsageAt)
        launchDefaults = try c.decodeIfPresent(DeviceLaunchDefaults.self, forKey: .launchDefaults)
        rowId = try c.decodeIfPresent(String.self, forKey: .rowId)
    }

    /// EXP-432: whether this is one of the caller's OWN machines. A teammate's
    /// shared row carries `owner`; own rows never do (mirrors web's
    /// `deviceIsMine`). The rename/remove/update actions gate on this — a
    /// shared machine is startable but never manageable from here.
    public var isMine: Bool { owner == nil }

    /// Whether the machine is startable right now. Rows straight off the relay
    /// carry no `online` field and are online by construction.
    public var isOnline: Bool { online != false }

    /// EXP-622: whether this is the caller's default machine.
    public var isDefaultDevice: Bool { isDefault == true }

    /// The agents the machine can run right now, in the reported order. An
    /// ABSENT advertisement means claude-only (a pre-EXP-201 sender), but an
    /// explicitly EMPTY one means nothing is runnable (EXP-409: every
    /// installed agent is signed out) — the absent/empty distinction is the
    /// whole point, so never collapse it with `agents ?? []`.
    public var agentIds: [String] {
        guard let agents else { return ["claude"] }
        return agents.filter { DomainContract.codingAgentValues.contains($0) }
    }

    /// EXP-409: agents installed on the machine but signed out.
    public var unauthedAgentIds: [String] {
        (unauthedAgents ?? []).filter { DomainContract.codingAgentValues.contains($0) }
    }

    /// EXP-749: the ACP-drivable agents as contract ids. Empty means nothing
    /// can start on this machine, which is also what a stale NULL column
    /// reads as now that every build above the floor reports the set.
    public var acpAgentIds: [String] {
        acpAgents.filter { DomainContract.codingAgentValues.contains($0) }
    }

    /// EXP-773: whether [agent] CANNOT start on this machine — it is outside
    /// the machine's ACP set and the PTY fallback is gone.
    public func agentNotReady(_ agent: String) -> Bool {
        !acpAgentIds.contains(agent)
    }

    /// Whether anything can be launched here at all (EXP-409). A machine that
    /// is online with nothing runnable is as unstartable as an offline one —
    /// pickers drop it and the machines list shows the sign-in reason.
    public var hasRunnableAgent: Bool { !agentIds.isEmpty }

    /// Online, yet nothing runnable because every installed agent is signed
    /// out — the state the machines row greys out and explains.
    public var needsAgentSignIn: Bool {
        isOnline && !hasRunnableAgent && !unauthedAgentIds.isEmpty
    }

    /// EXP-746: whether this machine can run a session through the in-process
    /// ACP engine (the session screen) at all. A machine without the cap is an
    /// older build.
    public var supportsAcp: Bool { caps?.contains("acp") == true }

    /// EXP-484: whether this machine can run an agent sign-in REMOTELY (the
    /// `agent_login` device command). Strictly cap-gated like the other remote
    /// affordances — an old build would never pick the command up.
    public var canAgentLogin: Bool { caps?.contains("agent-login") == true }

    /// EXP-437: the machine's configured default agent, clamped to what it can
    /// actually RUN. Nil when it advertises none (older desktop) or names an
    /// agent it no longer runs — the caller keeps its own choice then.
    public var defaultLaunchAgent: String? {
        guard let value = launchDefaults?.defaultAgent, agentIds.contains(value) else { return nil }
        return value
    }

    /// EXP-437: the machine's saved launch options for one agent, or nil when
    /// it advertises none for it — the caller falls back to contract defaults.
    public func agentDefaults(for agent: String) -> AgentLaunchDefaults? {
        launchDefaults?.agents?[agent]
    }

    /// A headless `exponential` daemon server rather than the desktop app —
    /// the only kind the self-update request applies to.
    public var isServer: Bool { kind == "server" }

    /// Whether a registry row backs it (the rename/remove targets).
    public var isRegistered: Bool { registered == true }

    // EXP-672: the `actions` / `action-inputs` / `fix-conflicts` / `chat` /
    // `resume` mirrors are GONE. Every desktop and CLI above the version floor
    // advertises all five whenever it advertises a runnable agent, and the
    // server stopped refusing on them (EXP-624/EXP-639) — the mirrors gated on
    // nothing and only made the pickers lie ("update the desktop app") when the
    // real reason was that no machine was online. What remains here mirrors a
    // gate the server still applies.

    /// EXP-637: whether this machine can RESUME an ended run (relaunch the
    /// agent in the run's own worktree, continuing its transcript). Strictly
    /// cap-gated like actions — the server refuses `resumeSessionId` without
    /// it, so a Resume affordance renders only when the machine advertises it.
    public var canResumeRun: Bool { caps?.contains("resume-run") == true }

    /// EXP-849: whether this machine honours `account` on the resume of a LIVE
    /// run (the mid-session account switch). Its own cap, not `resume-run`: an
    /// older machine resumes fine but IGNORES the account and relaunches under
    /// the same login, so the switch would silently do nothing.
    public var canSwitchAccount: Bool { caps?.contains("account-switch") == true }

    /// EXP-530: whether this machine runs action automations locally (watches
    /// its own sync and fires schedule/event triggers). Trigger device pickers
    /// offer only these — an offline-but-capable machine stays pickable (its
    /// missed schedule fires once when it comes back).
    public var canRunAutomations: Bool { caps?.contains("automations") == true }

    /// EXP-420: whether this device's reported version compares below the
    /// given latest. Unknown or unparsable on either side = false — the
    /// Update affordance renders only when a newer version really exists.
    public func updateAvailable(latest: String?) -> Bool {
        guard let have = Self.versionTuple(version), let want = Self.versionTuple(latest) else {
            return false
        }
        return have.0 != want.0 ? have.0 < want.0
            : have.1 != want.1 ? have.1 < want.1
            : have.2 < want.2
    }

    /// `major.minor.patch` with any `-`/`+` suffix ignored (mirrors web's
    /// `parseVersionTuple`).
    private static func versionTuple(_ version: String?) -> (Int, Int, Int)? {
        guard let version,
            let core = version.split(whereSeparator: { $0 == "-" || $0 == "+" }).first
        else { return nil }
        let parts = core.split(separator: ".").map { Int($0) }
        guard parts.count == 3, let major = parts[0], let minor = parts[1],
            let patch = parts[2]
        else { return nil }
        return (major, minor, patch)
    }
}

/// `devices.latestVersions`' result: the instance's latest client versions
/// per channel (`CLIENT_LATEST_VERSION_DESKTOP` / `_CLI`; null = the server
/// doesn't know). EXP-420: gates the server rows' Update affordance.
public struct LatestVersions: Decodable, Sendable {
    public let desktop: String?
    public let cli: String?

    public init(desktop: String?, cli: String?) {
        self.desktop = desktop
        self.cli = cli
    }
}

private struct ViewerTicketInput: Encodable {
    let kind = "viewer"
    // EXP-707: the wire name is `sessionId` (renamed from `codingSessionId`).
    let sessionId: String
}

/// Launch options a remote start may carry (EXP-149) — the Agent page
/// composer's choices. Nil fields are omitted from the wire (synthesized
/// Encodable uses encodeIfPresent) and mean "desktop settings default"
/// (plan mode OFF). `agent` absent = claude (EXP-201). `effort: ""` (and
/// `model: ""` for codex) is an explicit "CLI default".
public struct SteerStartOptions: Sendable {
    public let agent: String?
    public let model: String?
    public let effort: String?
    public let ultracode: Bool?
    public let planMode: Bool?
    /// EXP-481: resume the issue's existing worktree/agent session instead of
    /// starting fresh. SINGLE-ISSUE starts only — the batch/action inputs
    /// never carry it (the server rejects it there).
    public let resume: Bool?
    /// EXP-825 (EXP-792): the agent login profile to launch under — one of
    /// the machine's `agentAccounts[agent].profiles` ids. Nil = the
    /// machine's active login (the `system` profile is never sent).
    public let account: String?

    public init(
        agent: String? = nil,
        model: String? = nil,
        effort: String? = nil,
        ultracode: Bool? = nil,
        planMode: Bool? = nil,
        resume: Bool? = nil,
        account: String? = nil
    ) {
        self.agent = agent
        self.model = model
        self.effort = effort
        self.ultracode = ultracode
        self.planMode = planMode
        self.resume = resume
        self.account = account
    }
}

// Internal (not private) so `SteerStartInputEncodingTests` can pin the wire.
struct StartSessionInput: Encodable {
    let issueId: String
    let deviceId: String
    let agent: String?
    let model: String?
    let effort: String?
    let ultracode: Bool?
    let planMode: Bool?
    // EXP-481: single-issue only — the batch/action inputs deliberately have
    // no such field.
    let resume: Bool?
    let account: String?
    // EXP-825: the composer's free text — additional instructions on an
    // issue start (images embedded, `AgentComposerPrompt`). Absent when
    // blank. It rides fine with `resume` (a worktree resume); the server
    // forbids it only next to `resumeSessionId` (a recorded run keeps its
    // options), which this input never carries.
    let prompt: String?
}

/// Batch remote-start (EXP-156): 2+ issues → ONE Claude session on one pushed
/// `exp/batch-<id8>` branch, ending in ONE combined PR the server links to
/// every listed issue. Same `steer.startSession` endpoint — exactly one of
/// issueId/issueIds is present. Nil options are omitted (synthesized Encodable
/// uses encodeIfPresent) and mean "desktop settings default".
struct StartBatchSessionInput: Encodable {
    let issueIds: [String]
    let deviceId: String
    let agent: String?
    let model: String?
    let effort: String?
    let ultracode: Bool?
    let planMode: Bool?
    let account: String?
    let prompt: String?
}

/// Action remote-start (EXP-253/EXP-257): exactly one of
/// issueId/issueIds/actionId is present on `steer.startSession` — this is the
/// actionId form. Since EXP-257 it accepts the FULL option set with the same
/// per-agent vocabulary as issue runs, plus `inputs` (key → text or picked
/// repo/board uuid) and `teamId` — sent ONLY with the builtin
/// `builtin:create-action` id (the server requires it there and forbids it
/// otherwise). Nil fields are omitted (synthesized Encodable uses
/// encodeIfPresent) and mean "desktop settings default". EXP-825: `prompt`
/// is the chat text for `builtin:chat`, the request for
/// `builtin:create-action` (both REQUIRED server-side) and additional
/// instructions for every other action.
struct StartActionSessionInput: Encodable {
    let actionId: String
    let teamId: String?
    let deviceId: String
    let agent: String?
    let model: String?
    let effort: String?
    let ultracode: Bool?
    let planMode: Bool?
    let inputs: [String: String]?
    let account: String?
    let prompt: String?
}

/// Resume remote-start (EXP-637): the fourth `steer.startSession` subject —
/// exactly one of issueId/issueIds/actionId/resumeSessionId is present. A
/// resumed run keeps the ENDED row's recorded agent and options, so no launch
/// option may ride along (the server rejects them) — with ONE exception since
/// EXP-849: `account`, the login profile the relaunch runs under. A remote
/// "switch account" IS a resume naming a different profile, so the field has
/// to ride here; absent (the plain Resume) keeps the recorded one.
/// Internal (not private) so `SteerStartInputEncodingTests` can pin the wire.
struct ResumeSessionInput: Encodable {
    let resumeSessionId: String
    let deviceId: String
    let account: String?
}

private struct StartSessionResult: Decodable {
    let ok: Bool
}

private struct KillSessionInput: Encodable {
    // EXP-707: the wire name is `sessionId` (renamed from `codingSessionId`).
    let sessionId: String
}

public final class SteerApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Whether the relay is configured on this instance (`steer.config` query).
    public func config(accountId: String) async throws -> SteerConfig {
        try await trpc.query(accountId: accountId, path: "steer.config")
    }

    /// Mint a `viewer` ticket (watch + optional steer, per the ticket's perm)
    /// for a running coding session's relay room.
    public func mintViewerTicket(accountId: String, codingSessionId: String) async throws -> SteerTicket {
        try await trpc.mutation(
            accountId: accountId,
            path: "steer.mintTicket",
            input: ViewerTicketInput(sessionId: codingSessionId)
        )
    }

    /// Remote "Start on my desktop": route a `start_session` to the chosen
    /// online device. Throws `SteerStartError.rejected` with the server's
    /// human-readable reason on PRECONDITION_FAILED (device offline, no repo
    /// linked, relay off) so the UI can surface it verbatim.
    public func startSession(
        accountId: String,
        issueId: String,
        deviceId: String,
        options: SteerStartOptions = SteerStartOptions(),
        prompt: String? = nil
    ) async throws {
        do {
            let _: StartSessionResult = try await trpc.mutation(
                accountId: accountId,
                path: "steer.startSession",
                input: StartSessionInput(
                    issueId: issueId,
                    deviceId: deviceId,
                    agent: options.agent,
                    model: options.model,
                    effort: options.effort,
                    ultracode: options.ultracode,
                    planMode: options.planMode,
                    resume: options.resume,
                    account: options.account,
                    prompt: prompt
                )
            )
        } catch let TrpcError.httpError(status, body) {
            if let message = Self.trpcErrorMessage(fromBody: body) {
                throw SteerStartError.rejected(message)
            }
            throw TrpcError.httpError(status, body)
        }
    }

    /// Batch remote-start (EXP-156): route a `start_session` carrying 2+ issue
    /// ids to the chosen desktop — the launcher runs ONE batch Claude session
    /// and opens ONE combined PR the server links to every issue. Same endpoint
    /// and PRECONDITION_FAILED → `SteerStartError.rejected` mapping as the
    /// single-issue form.
    public func startSession(
        accountId: String,
        issueIds: [String],
        deviceId: String,
        options: SteerStartOptions = SteerStartOptions(),
        prompt: String? = nil
    ) async throws {
        do {
            let _: StartSessionResult = try await trpc.mutation(
                accountId: accountId,
                path: "steer.startSession",
                input: StartBatchSessionInput(
                    issueIds: issueIds,
                    deviceId: deviceId,
                    agent: options.agent,
                    model: options.model,
                    effort: options.effort,
                    ultracode: options.ultracode,
                    planMode: options.planMode,
                    account: options.account,
                    prompt: prompt
                )
            )
        } catch let TrpcError.httpError(status, body) {
            if let message = Self.trpcErrorMessage(fromBody: body) {
                throw SteerStartError.rejected(message)
            }
            throw TrpcError.httpError(status, body)
        }
    }

    /// Action remote-start (EXP-253/EXP-257): route a `start_session` carrying
    /// an actionId to the chosen desktop — an online machine with a runnable
    /// agent is the whole requirement (EXP-672: the action capability refusals
    /// are gone server-side, the agent check is the one that remains).
    /// `teamId` rides ONLY with the builtin
    /// `DomainContract.builtinCreateActionId` (real actions resolve their team
    /// server-side); `inputs` maps input keys to text values or picked
    /// repo/board uuids; `prompt` is the composer's text (EXP-825: the chat
    /// text / creation request for the two hidden builtins, additional
    /// instructions otherwise). Same endpoint and PRECONDITION_FAILED →
    /// `SteerStartError.rejected` mapping as the issue forms.
    public func startSession(
        accountId: String,
        actionId: String,
        deviceId: String,
        teamId: String? = nil,
        options: SteerStartOptions = SteerStartOptions(),
        inputs: [String: String]? = nil,
        prompt: String? = nil
    ) async throws {
        do {
            let _: StartSessionResult = try await trpc.mutation(
                accountId: accountId,
                path: "steer.startSession",
                input: StartActionSessionInput(
                    actionId: actionId,
                    teamId: teamId,
                    deviceId: deviceId,
                    agent: options.agent,
                    model: options.model,
                    effort: options.effort,
                    ultracode: options.ultracode,
                    planMode: options.planMode,
                    inputs: inputs,
                    account: options.account,
                    prompt: prompt
                )
            )
        } catch let TrpcError.httpError(status, body) {
            if let message = Self.trpcErrorMessage(fromBody: body) {
                throw SteerStartError.rejected(message)
            }
            throw TrpcError.httpError(status, body)
        }
    }

    /// Resume an ended run (EXP-637): route a `start_session` carrying the
    /// ended session's id to the machine that ran it — the launcher relaunches
    /// the pinned agent in the run's own worktree, continuing its transcript.
    /// Owner-only server-side, and the target must be the run's OWN device,
    /// online, advertising `resume-run` (`SteerDevice.canResumeRun`). No launch
    /// options: a resumed run keeps the ones the ended row recorded. Same
    /// PRECONDITION_FAILED → `SteerStartError.rejected` mapping as the other
    /// forms, so the refusal reason ("That run lives on another machine") shows
    /// verbatim.
    ///
    /// EXP-849: `account` is the ONE option a resume may carry — the login
    /// profile to relaunch under (one of the host machine's
    /// `agentAccounts[agent].profiles` ids). That is how a "switch account"
    /// works from a phone: resume the run on the same machine under another
    /// login, and follow the new row. Nil = keep the recorded login.
    public func resumeSession(
        accountId: String,
        sessionId: String,
        deviceId: String,
        account: String? = nil
    ) async throws {
        do {
            let _: StartSessionResult = try await trpc.mutation(
                accountId: accountId,
                path: "steer.startSession",
                input: ResumeSessionInput(
                    resumeSessionId: sessionId,
                    deviceId: deviceId,
                    account: account
                )
            )
        } catch let TrpcError.httpError(status, body) {
            if let message = Self.trpcErrorMessage(fromBody: body) {
                throw SteerStartError.rejected(message)
            }
            throw TrpcError.httpError(status, body)
        }
    }

    /// Kill-switch (EXP-268): `steer.killSession` flips the synced
    /// coding_sessions row to `ended` server-side — the desktop watches its
    /// own row over Electric, so this aborts the run even when the relay is
    /// unreachable — and best-effort fans a kill through the relay so the
    /// live terminal tears down immediately. Server-gated to the session
    /// owner or a team owner; idempotent on an already-ended session.
    public func killSession(accountId: String, codingSessionId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "steer.killSession",
            input: KillSessionInput(sessionId: codingSessionId)
        )
    }

    /// Extract the human `message` from a tRPC error envelope
    /// (`{"error":{"message":…}}`, possibly wrapped in a batch array).
    static func trpcErrorMessage(fromBody body: String) -> String? {
        guard let data = body.data(using: .utf8) else { return nil }
        let json = try? JSONSerialization.jsonObject(with: data)
        let obj: [String: Any]? = (json as? [String: Any]) ?? (json as? [[String: Any]])?.first
        guard let error = obj?["error"] as? [String: Any] else { return nil }
        if let message = error["message"] as? String { return message }
        if let inner = error["json"] as? [String: Any] { return inner["message"] as? String }
        return nil
    }
}

/// A remote-start rejection with a server-provided, user-presentable reason.
public enum SteerStartError: Error, LocalizedError, Sendable {
    case rejected(String)

    public var errorDescription: String? {
        switch self {
        case let .rejected(message): message
        }
    }
}
