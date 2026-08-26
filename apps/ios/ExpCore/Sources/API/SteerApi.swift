import Foundation

// Mirrors apps/web/src/lib/trpc/steer.ts (the ticket-minting router) + the relay
// wire contract in apps/steer-relay/src/protocol.ts. The steer relay is the
// data-plane for live terminal bytes (Electric can't carry a PTY). The desktop
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
/// the booleans are serialized only when true (absent = false). Everything is
/// optional here anyway: the sender is a desktop of unknown vintage.
/// Equatable so a settings surface can diff the live row against its drafts
/// (EXP-490: the iOS device-settings sheet re-seeds off `onChange`).
public struct AgentLaunchDefaults: Decodable, Equatable, Sendable {
    public let model: String?
    public let effort: String?
    public let ultracode: Bool?
    public let planMode: Bool?
    public let skipPermissions: Bool?

    public init(
        model: String? = nil,
        effort: String? = nil,
        ultracode: Bool? = nil,
        planMode: Bool? = nil,
        skipPermissions: Bool? = nil
    ) {
        self.model = model
        self.effort = effort
        self.ultracode = ultracode
        self.planMode = planMode
        self.skipPermissions = skipPermissions
    }
}

/// EXP-437: the coding defaults a machine advertises, so a remote Start-coding
/// sheet opens on THAT machine's settings instead of whatever the phone last
/// sent. `agents` covers the RUNNABLE agents only (contract `codingAgent`
/// ids). Absent entirely on an older desktop — every reader falls back to the
/// static contract defaults.
public struct DeviceLaunchDefaults: Decodable, Equatable, Sendable {
    /// The machine's configured default agent. Clamped to what it actually
    /// runs by the reader — a signed-out default must not preselect.
    public let defaultAgent: String?
    public let agents: [String: AgentLaunchDefaults]?

    public init(defaultAgent: String? = nil, agents: [String: AgentLaunchDefaults]? = nil) {
        self.defaultAgent = defaultAgent
        self.agents = agents
    }
}

/// One machine: a registry row (EXP-403 — desktops and headless
/// `exponential` daemon servers, online or not, synced through the devices
/// shape) or a bare relay-presence row from `steer.myDevices`. ONE shape for
/// both, mirroring apps/web/src/lib/steer-devices.ts: the registry fields are
/// optional and an absent `online` reads as online, because a presence row is
/// alive by construction. Usually the caller's own — but teammates' shared
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
    /// EXP-432: the team this machine is shared with (nil = private). Carried
    /// on the caller's OWN rows too, so it can never stand in for `owner`.
    public let sharedTeamId: String?
    /// EXP-432: set ONLY on a teammate's shared row — the machine's owner.
    /// Absent on the caller's own rows, which is exactly what `isMine` reads.
    public let owner: DeviceOwner?
    /// EXP-622: the caller's DEFAULT machine — pickers prefill it over their
    /// first candidate. Set only on the caller's own rows: the flag lives on
    /// the device row and belongs to its owner, so a teammate's shared server
    /// never prefills off it.
    public let isDefault: Bool?
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
        caps: [String]? = nil,
        kind: String? = nil,
        platform: String? = nil,
        online: Bool? = nil,
        lastSeenAt: String? = nil,
        registered: Bool? = nil,
        version: String? = nil,
        updateRequested: Bool? = nil,
        updateBlocked: Bool? = nil,
        sharedTeamId: String? = nil,
        owner: DeviceOwner? = nil,
        isDefault: Bool? = nil,
        launchDefaults: DeviceLaunchDefaults? = nil,
        rowId: String? = nil
    ) {
        self.deviceId = deviceId
        self.deviceLabel = deviceLabel
        self.connectedAt = connectedAt
        self.agents = agents
        self.unauthedAgents = unauthedAgents
        self.caps = caps
        self.kind = kind
        self.platform = platform
        self.online = online
        self.lastSeenAt = lastSeenAt
        self.registered = registered
        self.version = version
        self.updateRequested = updateRequested
        self.updateBlocked = updateBlocked
        self.sharedTeamId = sharedTeamId
        self.owner = owner
        self.isDefault = isDefault
        self.launchDefaults = launchDefaults
        self.rowId = rowId
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

    /// Whether anything can be launched here at all (EXP-409). A machine that
    /// is online with nothing runnable is as unstartable as an offline one —
    /// pickers drop it and the machines list shows the sign-in reason.
    public var hasRunnableAgent: Bool { !agentIds.isEmpty }

    /// Online, yet nothing runnable because every installed agent is signed
    /// out — the state the machines row greys out and explains.
    public var needsAgentSignIn: Bool {
        isOnline && !hasRunnableAgent && !unauthedAgentIds.isEmpty
    }

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

    /// Whether this desktop can run team actions (EXP-253).
    public var canRunActions: Bool { caps?.contains("actions") == true }

    /// Whether this desktop understands typed action inputs + the builtin
    /// "Create action" run (EXP-257). Builtin or inputs-carrying starts are
    /// additionally gated on this (the server enforces it too); a plain
    /// `actions`-capable desktop still runs input-less actions.
    public var canRunActionInputs: Bool { caps?.contains("action-inputs") == true }

    /// EXP-481: whether this machine honors `resume` on a remote start —
    /// strictly cap-gated like actions (the server refuses without it; an old
    /// build would silently start fresh, which reads as losing the session).
    public var canResume: Bool { caps?.contains("resume") == true }

    /// Whether this desktop can run the builtin "Fix merge conflicts" action
    /// (EXP-259). The server rejects that builtin without the cap, so pickers
    /// filter such desktops out instead of failing after submit (EXP-323).
    public var canFixConflicts: Bool { caps?.contains("fix-conflicts") == true }

    /// EXP-615: whether this machine can run the hidden "Chat" builtin — a
    /// free-prompt agent session on a repository's trunk clone. Cap-gated
    /// exactly like fix-conflicts: the server refuses the id without it, so
    /// the Chat tab filters such machines out instead of failing after submit.
    public var canChat: Bool { caps?.contains("chat") == true }

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

/// `steer.myDevices`' envelope — exactly `{ devices }`; unknown keys decode
/// away.
public struct SteerDevicesResult: Decodable, Sendable {
    public let devices: [SteerDevice]

    public init(devices: [SteerDevice]) {
        self.devices = devices
    }
}

private struct ViewerTicketInput: Encodable {
    let kind = "viewer"
    let codingSessionId: String
}

/// Launch options a remote start may carry (EXP-149) — the Start-coding
/// sheet's choices. Nil fields are omitted from the wire (synthesized
/// Encodable uses encodeIfPresent) and mean "desktop settings default"
/// (plan mode OFF). `agent` absent = claude (EXP-201). `effort: ""` (and
/// `model: ""` for codex/pi) is an explicit "CLI default".
public struct SteerStartOptions: Sendable {
    public let agent: String?
    public let model: String?
    public let effort: String?
    public let ultracode: Bool?
    public let planMode: Bool?
    public let skipPermissions: Bool?
    /// EXP-481: resume the issue's existing worktree/agent session instead of
    /// starting fresh. SINGLE-ISSUE starts only — the batch/action inputs
    /// never carry it (the server rejects it there).
    public let resume: Bool?

    public init(
        agent: String? = nil,
        model: String? = nil,
        effort: String? = nil,
        ultracode: Bool? = nil,
        planMode: Bool? = nil,
        skipPermissions: Bool? = nil,
        resume: Bool? = nil
    ) {
        self.agent = agent
        self.model = model
        self.effort = effort
        self.ultracode = ultracode
        self.planMode = planMode
        self.skipPermissions = skipPermissions
        self.resume = resume
    }
}

private struct StartSessionInput: Encodable {
    let issueId: String
    let deviceId: String
    let agent: String?
    let model: String?
    let effort: String?
    let ultracode: Bool?
    let planMode: Bool?
    let skipPermissions: Bool?
    // EXP-481: single-issue only — the batch/action inputs deliberately have
    // no such field.
    let resume: Bool?
}

/// Batch remote-start (EXP-156): 2+ issues → ONE Claude session on one pushed
/// `exp/batch-<id8>` branch, ending in ONE combined PR the server links to
/// every listed issue. Same `steer.startSession` endpoint — exactly one of
/// issueId/issueIds is present. Nil options are omitted (synthesized Encodable
/// uses encodeIfPresent) and mean "desktop settings default".
private struct StartBatchSessionInput: Encodable {
    let issueIds: [String]
    let deviceId: String
    let agent: String?
    let model: String?
    let effort: String?
    let ultracode: Bool?
    let planMode: Bool?
    let skipPermissions: Bool?
}

/// Action remote-start (EXP-253/EXP-257): exactly one of
/// issueId/issueIds/actionId is present on `steer.startSession` — this is the
/// actionId form. Since EXP-257 it accepts the FULL option set with the same
/// per-agent vocabulary as issue runs, plus `inputs` (key → text or picked
/// repo/board uuid) and `teamId` — sent ONLY with the builtin
/// `builtin:create-action` id (the server requires it there and forbids it
/// otherwise). Nil fields are omitted (synthesized Encodable uses
/// encodeIfPresent) and mean "desktop settings default".
private struct StartActionSessionInput: Encodable {
    let actionId: String
    let teamId: String?
    let deviceId: String
    let agent: String?
    let model: String?
    let effort: String?
    let ultracode: Bool?
    let planMode: Bool?
    let skipPermissions: Bool?
    let inputs: [String: String]?
}

private struct StartSessionResult: Decodable {
    let ok: Bool
}

private struct KillSessionInput: Encodable {
    let codingSessionId: String
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
            input: ViewerTicketInput(codingSessionId: codingSessionId)
        )
    }

    /// The caller's online desktops (`steer.myDevices` query) — own relay
    /// presence only. Since EXP-432 the start surfaces use
    /// `DeviceQueries.onlineStartTargets` (the synced devices rows) instead,
    /// so teammates' shared servers count as start targets; this stays as the
    /// presence-only primitive. Relay-off ⇒ empty list.
    public func myDevices(accountId: String) async throws -> [SteerDevice] {
        let result: SteerDevicesResult = try await trpc.query(accountId: accountId, path: "steer.myDevices")
        return result.devices
    }

    /// Remote "Start on my desktop": route a `start_session` to the chosen
    /// online device. Throws `SteerStartError.rejected` with the server's
    /// human-readable reason on PRECONDITION_FAILED (device offline, no repo
    /// linked, relay off) so the UI can surface it verbatim.
    public func startSession(
        accountId: String,
        issueId: String,
        deviceId: String,
        options: SteerStartOptions = SteerStartOptions()
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
                    skipPermissions: options.skipPermissions,
                    resume: options.resume
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
        options: SteerStartOptions = SteerStartOptions()
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
                    skipPermissions: options.skipPermissions
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
    /// an actionId to the chosen desktop — the device must advertise the
    /// `actions` capability (`SteerDevice.canRunActions`; builtin or
    /// inputs-carrying runs additionally need `canRunActionInputs`; the server
    /// enforces both). `teamId` rides ONLY with the builtin
    /// `DomainContract.builtinCreateActionId` (real actions resolve their team
    /// server-side); `inputs` maps input keys to text values or picked
    /// repo/board uuids. Same endpoint and PRECONDITION_FAILED →
    /// `SteerStartError.rejected` mapping as the issue forms.
    public func startSession(
        accountId: String,
        actionId: String,
        deviceId: String,
        teamId: String? = nil,
        options: SteerStartOptions = SteerStartOptions(),
        inputs: [String: String]? = nil
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
                    skipPermissions: options.skipPermissions,
                    inputs: inputs
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
            input: KillSessionInput(codingSessionId: codingSessionId)
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
