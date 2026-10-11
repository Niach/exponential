import Foundation

// Mirrors apps/web/src/lib/trpc/devices.ts (EXP-403): the per-user machine
// registry. Desktops and headless `exponential` daemon servers register
// themselves and heartbeat, and the durable rows themselves reach the clients
// through the `devices` shape (EXP-481) — read them via `DeviceQueries`
// (Domain/DeviceRows.swift), never by polling.
// What is left here is what a shape cannot carry: the registry MUTATIONS, the
// owner→device command queue, and the instance-wide latest-version hint.

private struct EmptyInput: Encodable {}

private struct DeviceIdInput: Encodable {
    let deviceId: String
}

private struct RenameDeviceInput: Encodable {
    let deviceId: String
    let label: String
}

/// EXP-924: `devices.setIcon` — the owner-picked display glyph. Hand-encoded
/// because the server takes `deviceIcon | null` and NOT an absent key: a reset
/// to the kind default must ride as an explicit JSON null, which the
/// synthesized encoder would drop instead.
private struct SetIconInput: Encodable {
    let deviceId: String
    let icon: String?

    enum CodingKeys: String, CodingKey {
        case deviceId, icon
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(deviceId, forKey: .deviceId)
        if let icon {
            try c.encode(icon, forKey: .icon)
        } else {
            try c.encodeNil(forKey: .icon)
        }
    }
}

/// EXP-622: `devices.setDefault` — flag/unflag the caller's default machine.
private struct SetDefaultInput: Encodable {
    let deviceId: String
    let isDefault: Bool
}

/// FEED-33: `devices.setShared` toggles ONE team in or out of the machine's
/// shared set (`{deviceId, teamId, shared}`); the pre-FEED-33 single-team
/// `teamId: string | null` form is legacy.
private struct SetSharedInput: Encodable {
    let deviceId: String
    let teamId: String
    let shared: Bool
}

/// EXP-481: one agent's launch defaults in `devices.setLaunchDefaults` wire
/// form. Only set fields ride (synthesized encodeIfPresent) — the server
/// clamps vocabulary field-wise either way.
public struct AgentLaunchDefaultsInput: Encodable, Sendable {
    public let model: String?
    /// EXP-981: claude only — the model its subagents run on. `""` is a real
    /// choice here (the CLI's own default), unlike on a start, where the
    /// server takes contract values only and "default" means OMITTED.
    public let subagentModel: String?
    public let effort: String?
    public let ultracode: Bool?
    public let planMode: Bool?
    /// EXP-1082 §6: the desktop's auto-rotate toggle, ECHOED from the synced
    /// row on every whole-object save. The server keeps a stored value only
    /// while the KEY is absent (a compat carry-forward slated for removal),
    /// so a sender that omitted it would clear the toggle once that goes.
    /// Never invented here: absent stays absent, present rides verbatim.
    public let autoRotateAccounts: Bool?

    public init(
        model: String? = nil,
        subagentModel: String? = nil,
        effort: String? = nil,
        ultracode: Bool? = nil,
        planMode: Bool? = nil,
        autoRotateAccounts: Bool? = nil
    ) {
        self.model = model
        self.subagentModel = subagentModel
        self.effort = effort
        self.ultracode = ultracode
        self.planMode = planMode
        self.autoRotateAccounts = autoRotateAccounts
    }
}

/// EXP-481: the whole-object `launchDefaults` payload — the device settings
/// sheet sends the full edited struct (UI edits omit `expectedUpdatedAt`
/// server-side: unconditional last-write-wins between humans).
///
/// EXP-1158: no `defaultAgent` and no account. The stored `defaultAgent` is
/// the machine's LAST USED agent, which only the DEVICE writes; the server
/// carries it forward when a save omits it. Fields ride only when set.
///
/// EXP-1196: `computerUse` = the DEVICE-level switch, a top-level key beside
/// `agents`. The sheet sends what it shows (seeded from the row, explicit once
/// toggled); nil writes no key and the server carries the stored value forward.
/// EXP-1236: `computerUseModel` rides the same way beside it (a contract
/// `computerUseModel` alias; the server clamps unknown ones).
public struct DeviceLaunchDefaultsInput: Encodable, Sendable {
    public let agents: [String: AgentLaunchDefaultsInput]?
    public let computerUse: Bool?
    public let computerUseModel: String?

    public init(
        agents: [String: AgentLaunchDefaultsInput]? = nil,
        computerUse: Bool? = nil,
        computerUseModel: String? = nil
    ) {
        self.agents = agents
        self.computerUse = computerUse
        self.computerUseModel = computerUseModel
    }
}

private struct SetLaunchDefaultsInput: Encodable {
    let deviceId: String
    let launchDefaults: DeviceLaunchDefaultsInput
}

private struct CreateCommandInput: Encodable {
    let deviceId: String
    /// `agent_login` (EXP-484: `agent` required, `switch` optional) |
    /// `agent_login_code` (EXP-765: `agent` + `code` required) |
    /// `agent_update` (EXP-1196: `agent` required — the agent CLI's own
    /// self-updater, the readiness block's Update pill).
    /// EXP-1042: this client no longer emits `worktree_remove` /
    /// `worktree_prune` — the worktree inventory is an IDE surface now, and
    /// EXP-1060 (compat round 26) retired those kinds server-side along with
    /// the `repoFullName`/`branch` keys they alone needed.
    let kind: String
    /// EXP-484: contract `codingAgent` id for `agent_login` (an id outside
    /// the contract is refused
    /// server-side — it has no remote sign-in).
    let agent: String?
    /// EXP-484: sign out first, then sign in as somebody else. `switch` is a
    /// Swift keyword, so the property is renamed and the wire key restored via
    /// CodingKeys; the server takes a real JSON boolean.
    let switchAccount: Bool?
    /// EXP-765: the authorization code the browser showed, for
    /// `agent_login_code` — the machine types it into the sign-in still
    /// waiting on its own screen. The server trims it and refuses an empty
    /// one; nil is simply omitted like the rest.
    let code: String?
    /// EXP-829 (`agent_usage_refresh`, EXP-747 C4): which login profile to
    /// re-read. EXP-862's `agent_profile_remove` names its target with it
    /// too. On `agent_login` it is the INTENDED profile (the row whose Sign in
    /// was tapped; nil = "Add account"): the machine lands the login on the
    /// profile matching its EMAIL regardless, and uses this only to tell a
    /// duplicate apart.
    let profileId: String?
    /// `agent_login` only: MOVE the machine's ambient login into a profile
    /// instead of signing in. Needs the `agent-import` cap; never together
    /// with `profileId` (the server refuses the pair). `import` is a Swift
    /// keyword, so the wire key is restored via CodingKeys.
    let importLogin: Bool?

    enum CodingKeys: String, CodingKey {
        case deviceId, kind, agent, code, profileId
        case switchAccount = "switch"
        case importLogin = "import"
    }
}

/// EXP-481: `devices.createCommand`'s result — the id the issuing UI polls.
public struct CreatedDeviceCommand: Decodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

private struct GetCommandInput: Encodable {
    let commandId: String
}

/// EXP-481: one queued owner→device command (`devices.getCommand`). The
/// issuing sheet polls `status` (`pending` → `done` | `failed`) and renders
/// `result` — the device-reported message — on failure (and as the prune
/// summary on success).
public struct DeviceCommand: Decodable, Sendable {
    public let id: String
    public let kind: String
    public let status: String
    public let result: String?
    public let completedAt: String?

    public init(id: String, kind: String, status: String, result: String?, completedAt: String?) {
        self.id = id
        self.kind = kind
        self.status = status
        self.result = result
        self.completedAt = completedAt
    }

    public var isPending: Bool { status == "pending" }
    public var isFailed: Bool { status == "failed" }
}

public final class DevicesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// EXP-420: the instance's latest client versions per channel
    /// (`devices.latestVersions` query) — instance config, not machine state,
    /// so it rides tRPC while the machines themselves stream off the devices
    /// shape. Gates the Update affordance on an actually-newer build; either
    /// channel is nil when the server doesn't know.
    public func latestVersions(accountId: String) async throws -> LatestVersions {
        try await trpc.query(accountId: accountId, path: "devices.latestVersions")
    }

    /// Rename a registered machine. The REGISTRY label is authoritative, so
    /// the new name shows immediately whether the machine is online or not.
    public func rename(accountId: String, deviceId: String, label: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "devices.rename",
            input: RenameDeviceInput(deviceId: deviceId, label: label)
        )
    }

    /// EXP-924: pick a machine's display glyph from the DEVICE icon set
    /// (contract `deviceIcon`); nil resets it to the kind default. Like the
    /// rename, the registry row is authoritative and the result arrives back
    /// through the devices shape, so it works while the machine is offline.
    public func setIcon(accountId: String, deviceId: String, icon: String?) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "devices.setIcon",
            input: SetIconInput(deviceId: deviceId, icon: icon)
        )
    }

    /// EXP-622: make this machine the caller's default — the row every device
    /// picker prefills. The server clears the flag on their other machines in
    /// the same transaction, so the result arrives through the devices shape.
    public func setDefault(accountId: String, deviceId: String, isDefault: Bool) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "devices.setDefault",
            input: SetDefaultInput(deviceId: deviceId, isDefault: isDefault)
        )
    }

    /// Forget a machine — drops the registry row only. A still-running daemon
    /// re-registers itself on its next heartbeat, and a live relay connection
    /// is untouched.
    public func remove(accountId: String, deviceId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "devices.remove",
            input: DeviceIdInput(deviceId: deviceId)
        )
    }

    /// Ask a daemon server to self-update: the flag rides its next heartbeat,
    /// and the row's `updateRequested` stays true until the daemon
    /// re-registers after acting on it (whether or not a newer build existed).
    public func requestUpdate(accountId: String, deviceId: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "devices.requestUpdate",
            input: DeviceIdInput(deviceId: deviceId)
        )
    }

    /// EXP-481/FEED-33: share / unshare one of the caller's SERVER machines
    /// with ONE team; the other teams in its set are untouched. Sharing is
    /// the consent that lets that team's members remote-start on the box;
    /// removing a team ends its hosted runs server-side.
    public func setShared(accountId: String, deviceId: String, teamId: String, shared: Bool) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "devices.setShared",
            input: SetSharedInput(deviceId: deviceId, teamId: teamId, shared: shared)
        )
    }

    /// EXP-481: edit a machine's SERVER-AUTHORITATIVE launch defaults —
    /// applies immediately server-side (an offline machine converges on its
    /// next heartbeat, so this needs no online gate).
    public func setLaunchDefaults(
        accountId: String,
        deviceId: String,
        launchDefaults: DeviceLaunchDefaultsInput
    ) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "devices.setLaunchDefaults",
            input: SetLaunchDefaultsInput(deviceId: deviceId, launchDefaults: launchDefaults)
        )
    }

    /// EXP-481: queue a command for the device (owner-only). Runs on its next
    /// heartbeat — immediately when online (relay nudge), on return when
    /// offline.
    /// EXP-1042: the app sends only the `agent_login*` kinds. It no longer
    /// emits `worktree_remove` / `worktree_prune` (the inventory left the
    /// phone for the IDE); EXP-1060 (compat round 26) retired those kinds
    /// server-side along with their `repoFullName`/`branch` keys.
    /// EXP-484: `agent_login` needs `agent` (and optionally `switchAccount`) —
    /// the device runs the agent's own sign-in and completes the command early
    /// with the URL/code as its `result`.
    /// EXP-765: `agent_login_code` needs `agent` + `code` — the way BACK from
    /// that link, typed into the sign-in still waiting on the machine.
    /// `importLogin: true` on `agent_login` = the doctor's Import (cap
    /// `agent-import`).
    public func createCommand(
        accountId: String,
        deviceId: String,
        kind: String,
        agent: String? = nil,
        switchAccount: Bool? = nil,
        code: String? = nil,
        profileId: String? = nil,
        importLogin: Bool? = nil
    ) async throws -> CreatedDeviceCommand {
        try await trpc.mutation(
            accountId: accountId,
            path: "devices.createCommand",
            input: CreateCommandInput(
                deviceId: deviceId, kind: kind, agent: agent, switchAccount: switchAccount,
                code: code, profileId: profileId, importLogin: importLogin
            )
        )
    }

    /// EXP-1196: the readiness block's remote Update pill — queue
    /// `agent_update {agent}`; the machine runs that agent CLI's own
    /// self-updater and its next doctor report shows the result.
    @discardableResult
    public func requestAgentUpdate(
        accountId: String,
        deviceId: String,
        agent: String
    ) async throws -> CreatedDeviceCommand {
        try await createCommand(
            accountId: accountId, deviceId: deviceId, kind: Self.agentUpdateKind, agent: agent
        )
    }

    public static let agentUpdateKind = "agent_update"

    /// EXP-1111/1169: mint the one-time `EXP_INSTALL_TOKEN` the device-setup
    /// card puts into its install command (one live token per user; a remint
    /// revokes the earlier one). Rate limited server-side: callers treat every
    /// failure as "no token" and show the plain command.
    public func createInstallToken(accountId: String) async throws -> CliInstallToken {
        try await trpc.mutation(
            accountId: accountId,
            path: "devices.createInstallToken",
            input: EmptyInput()
        )
    }

    /// EXP-481: the issuing UI's poll target while a command is in flight
    /// (the material outcome also lands via the device_worktrees shape when
    /// the device re-reports).
    public func getCommand(accountId: String, commandId: String) async throws -> DeviceCommand {
        try await trpc.query(
            accountId: accountId,
            path: "devices.getCommand",
            input: GetCommandInput(commandId: commandId)
        )
    }
}
