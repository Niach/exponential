import Foundation

// The steer wire types (apps/steer-relay/src/protocol.ts). Since VAPP-43 this
// app watches a session only over eNet (Enet/SteerEnet.swift): the steer relay
// carries no session content, so there is no viewer ticket to mint here.

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
    /// EXP-981: claude only — the model its subagents run on; absent = the
    /// contract default.
    public let subagentModel: String?
    /// The dispatcher root's model (threads + team agents) on this agent;
    /// absent = the contract default (`deviceAgentDefaults*ThreadModel`).
    public let threadModel: String?
    public let effort: String?
    public let ultracode: Bool?
    public let planMode: Bool?
    /// EXP-1082 §6 / EXP-1005: rotate to another logged-in account of this
    /// agent when the active one hits its usage wall. The desktop owns the
    /// toggle; this client only ECHOES the stored value back on a save (see
    /// `AgentLaunchDefaultsInput`). nil = the key is absent on the row.
    public let autoRotateAccounts: Bool?

    public init(
        model: String? = nil,
        subagentModel: String? = nil,
        threadModel: String? = nil,
        effort: String? = nil,
        ultracode: Bool? = nil,
        planMode: Bool? = nil,
        autoRotateAccounts: Bool? = nil
    ) {
        self.model = model
        self.subagentModel = subagentModel
        self.threadModel = threadModel
        self.effort = effort
        self.ultracode = ultracode
        self.planMode = planMode
        self.autoRotateAccounts = autoRotateAccounts
    }
}

/// EXP-437: the coding defaults a machine advertises, so a remote Start-coding
/// sheet opens on THAT machine's settings instead of whatever the phone last
/// sent. `agents` covers the RUNNABLE agents only (contract `codingAgent`
/// ids). Nullable — every reader falls back to the static contract defaults.
///
/// EXP-773 dropped `startInTerminal`: the PTY coding path is gone. Decoding
/// ignores unknown keys, so a stored row still carrying it keeps parsing.
public struct DeviceLaunchDefaults: Decodable, Equatable, Sendable {
    /// EXP-1158: the machine's LAST USED agent — the agent a person last
    /// started or switched a run on there. Only the DEVICE writes it; clients
    /// read it (`AccountOptions.flatten` leads with its active login) and never
    /// send it back. Clamped to what the machine actually runs by the reader.
    public let defaultAgent: String?
    public let agents: [String: AgentLaunchDefaults]?
    /// EXP-1196: the DEVICE-level "Computer use" switch (agents may see the
    /// screen, click and type). Off by default: nil = the key is absent (or
    /// not a boolean) on the row, which every reader treats as off.
    public let computerUse: Bool?
    /// EXP-1236: the model alias the run's screen-driving subagents run on
    /// (contract `computerUseModel`), a top-level key beside the switch.
    /// nil = the key is absent (or not a string): every reader seeds
    /// `DomainContract.deviceComputerUseDefaultsModel`.
    public let computerUseModel: String?
    /// NEW-59/#FEED-161: the work screen, the stable `id` of the display in
    /// the device's own `doctor.displays` list (fixture `work-screen.json`).
    /// nil = Auto (the key absent, null, or not a string).
    public let computerUseDisplayId: String?

    public init(
        defaultAgent: String? = nil,
        agents: [String: AgentLaunchDefaults]? = nil,
        computerUse: Bool? = nil,
        computerUseModel: String? = nil,
        computerUseDisplayId: String? = nil
    ) {
        self.defaultAgent = defaultAgent
        self.agents = agents
        self.computerUse = computerUse
        self.computerUseModel = computerUseModel
        self.computerUseDisplayId = computerUseDisplayId
    }

    private enum CodingKeys: String, CodingKey {
        case defaultAgent, agents, computerUse, computerUseModel, computerUseDisplayId
    }

    /// Lenient like the rest of the device payload: a field of a shape this
    /// build does not expect degrades to nil instead of throwing the whole
    /// machine's defaults away.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        defaultAgent = try? c.decodeIfPresent(String.self, forKey: .defaultAgent)
        agents = try? c.decodeIfPresent([String: AgentLaunchDefaults].self, forKey: .agents)
        computerUse = (try? c.decodeIfPresent(Bool.self, forKey: .computerUse)) ?? nil
        computerUseModel = (try? c.decodeIfPresent(String.self, forKey: .computerUseModel)) ?? nil
        computerUseDisplayId = (try? c.decodeIfPresent(String.self, forKey: .computerUseDisplayId)) ?? nil
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
    /// away (nil = the device reported none). These are the ONLY logins a
    /// client lists: the top-level fields mirror the active one, and no row is
    /// ever synthesized from them.
    public let profiles: [AgentAccountProfile]?
    /// The agent CLI's AMBIENT login (~/.claude, ~/.codex), present only
    /// while it is signed in. Never used for a run: the doctor offers to
    /// IMPORT it into a profile.
    public let importable: AgentImportable?

    public init(
        signedIn: Bool? = nil,
        email: String? = nil,
        plan: String? = nil,
        checkedAt: String? = nil,
        profiles: [AgentAccountProfile]? = nil,
        health: String? = nil,
        importable: AgentImportable? = nil
    ) {
        self.signedIn = signedIn
        self.email = email
        self.plan = plan
        self.checkedAt = checkedAt
        self.profiles = profiles
        self.health = health
        self.importable = importable
    }

    private enum CodingKeys: String, CodingKey {
        case signedIn, email, plan, checkedAt, profiles, health, importable
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        signedIn = try c.decodeIfPresent(Bool.self, forKey: .signedIn)
        email = try c.decodeIfPresent(String.self, forKey: .email)
        plan = try c.decodeIfPresent(String.self, forKey: .plan)
        checkedAt = try c.decodeIfPresent(String.self, forKey: .checkedAt)
        profiles = (try? c.decodeIfPresent([AgentAccountProfile].self, forKey: .profiles)) ?? nil
        health = try? c.decodeIfPresent(String.self, forKey: .health)
        importable = (try? c.decodeIfPresent(AgentImportable.self, forKey: .importable)) ?? nil
    }
}

/// The ambient login a machine could import (`agentAccounts[agent].importable`).
public struct AgentImportable: Decodable, Equatable, Sendable {
    public let email: String?
    public let plan: String?

    public init(email: String? = nil, plan: String? = nil) {
        self.email = email
        self.plan = plan
    }

    private enum CodingKeys: String, CodingKey { case email, plan }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        email = try? c.decodeIfPresent(String.self, forKey: .email)
        plan = try? c.decodeIfPresent(String.self, forKey: .plan)
    }
}

/// EXP-825: one login profile of an agent on a machine (`id` is what a start
/// sends as `account`; `active` marks the machine's current login; `email`
/// names it — a profile has no other name). Every field but the id optional
/// — the sender's vintage varies. A legacy `label` key is ignored.
public struct AgentAccountProfile: Decodable, Equatable, Sendable, Identifiable {
    public let id: String
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
    /// When a sign-in or an import last COMMITTED into this profile (ISO
    /// string, device-stamped) — what a sign-in sheet watches to see its
    /// login land (`AgentAccountsRows.loginLanding`).
    public let lastLoginAt: String?

    public init(
        id: String,
        active: Bool? = nil,
        signedIn: Bool? = nil,
        email: String? = nil,
        plan: String? = nil,
        checkedAt: String? = nil,
        usage: AgentUsage? = nil,
        health: String? = nil,
        lastLoginAt: String? = nil
    ) {
        self.id = id
        self.active = active
        self.signedIn = signedIn
        self.email = email
        self.plan = plan
        self.checkedAt = checkedAt
        self.usage = usage
        self.health = health
        self.lastLoginAt = lastLoginAt
    }

    private enum CodingKeys: String, CodingKey {
        case id, active, signedIn, email, plan, checkedAt, usage, health, lastLoginAt
    }

    /// Only the id is load-bearing; every other field degrades on its own so
    /// a newer device's profile entry never throws the whole list away.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        active = try? c.decodeIfPresent(Bool.self, forKey: .active)
        signedIn = try? c.decodeIfPresent(Bool.self, forKey: .signedIn)
        email = try? c.decodeIfPresent(String.self, forKey: .email)
        plan = try? c.decodeIfPresent(String.self, forKey: .plan)
        checkedAt = try? c.decodeIfPresent(String.self, forKey: .checkedAt)
        usage = try? c.decodeIfPresent(AgentUsage.self, forKey: .usage)
        health = try? c.decodeIfPresent(String.self, forKey: .health)
        lastLoginAt = try? c.decodeIfPresent(String.self, forKey: .lastLoginAt)
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
    /// EXP-924: the owner-picked display icon (contract `deviceIcon`); nil on
    /// relay-only rows and until somebody picks one. Never drawn raw — the
    /// glyph comes from `DeviceIconDisplay`, which falls back to the kind.
    public let icon: String?
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
    /// a remote Start-coding sheet pre-fills its settings. Absent on a machine
    /// with nothing runnable — read it through
    /// `defaultLaunchAgent` / `agentDefaults(for:)`, never raw.
    public let launchDefaults: DeviceLaunchDefaults?
    /// EXP-1196: the machine's readiness report (the synced `doctor` column),
    /// rendered through `DeviceReadiness`. Nil = no report yet (no readiness
    /// block), and on tRPC/relay rows.
    public let doctor: DeviceDoctor?
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
        icon: String? = nil,
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
        doctor: DeviceDoctor? = nil,
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
        self.icon = icon
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
        self.doctor = doctor
        self.rowId = rowId
    }

    private enum CodingKeys: String, CodingKey {
        case deviceId, deviceLabel, connectedAt, agents, unauthedAgents, acpAgents, caps
        case kind, icon, platform, online, lastSeenAt, registered, version, updateRequested
        case updateBlocked, sharedTeamIds, owner, isDefault, agentAccounts, agentUsage
        case agentUsageAt, launchDefaults, doctor, rowId
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
        icon = try c.decodeIfPresent(String.self, forKey: .icon)
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
        // EXP-1196: lenient — a malformed report drops alone, never the row.
        doctor = (try? c.decodeIfPresent(DeviceDoctor.self, forKey: .doctor)) ?? nil
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

    /// Every agent whose DEFAULTS are editable for this machine: runnable ∪
    /// signed-out installs ∪ the agents the stored defaults, the reported
    /// ACCOUNTS or the reported USAGE already name — an offline machine's
    /// defaults stay editable even though nothing is advertised as runnable
    /// right now, and an agent known only from a login still gets a tab.
    /// A machine that names NOTHING offers every contract agent.
    ///
    /// The twin of web's `editorAgents` (device-settings dialog); contract
    /// order, so the tabs never shuffle with the report order.
    public var editableAgentIds: [String] {
        var set = Set(agentIds)
        set.formUnion(unauthedAgentIds)
        if let stored = launchDefaults?.agents?.keys { set.formUnion(stored) }
        if let accounts = agentAccounts?.keys { set.formUnion(accounts) }
        if let usage = agentUsage?.keys { set.formUnion(usage) }
        let known = DomainContract.codingAgentValues.filter { set.contains($0) }
        return known.isEmpty ? DomainContract.codingAgentValues : known
    }

    /// EXP-749: the ACP-drivable agents as contract ids. Empty means nothing
    /// can start on this machine, which is also what a stale NULL column
    /// reads as now that every build above the floor reports the set.
    public var acpAgentIds: [String] {
        acpAgents.filter { DomainContract.codingAgentValues.contains($0) }
    }

    /// EXP-773: whether [agent] CANNOT start on this machine — it is outside
    /// the machine's ACP set and the PTY fallback is gone.
    ///
    /// EXP-1196: a device that reports its readiness is judged by it (Git ok
    /// and the agent's own row ok), like the web composer; a row without a
    /// report keeps the ACP rule.
    public func agentNotReady(_ agent: String) -> Bool {
        if let doctor {
            return !DeviceReadiness.runnableAgents(doctor).contains(agent)
        }
        return !acpAgentIds.contains(agent)
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

    /// EXP-484: whether this machine can run an agent sign-in REMOTELY (the
    /// `agent_login` device command). The server refuses the command without
    /// the cap.
    public var canAgentLogin: Bool { caps?.contains("agent-login") == true }

    /// EXP-437/EXP-1158: the machine's LAST USED agent, clamped to what it can
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

    /// EXP-1249: whether this machine reads the per-run `computerUse` flag
    /// (contract `codingSessionComputerUseCap`, a RUN cap: advertised only
    /// while an agent there is runnable); without it the toggle hides and the
    /// key never rides out.
    public var canToggleComputerUse: Bool {
        caps?.contains(DomainContract.codingSessionComputerUseCap) == true
    }

    /// The per-run toggle's seed: the machine's `launch_defaults.computerUse`.
    public var computerUseDefault: Bool { launchDefaults?.computerUse ?? false }

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
