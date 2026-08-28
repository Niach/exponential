import Foundation

// Mirrors apps/web/src/lib/trpc/devices.ts (EXP-403): the per-user machine
// registry. Desktops and headless `exponential` daemon servers register
// themselves and heartbeat, and the durable rows themselves reach the clients
// through the `devices` shape (EXP-481) — read them via `DeviceQueries`
// (Domain/DeviceRows.swift), never by polling.
// What is left here is what a shape cannot carry: the registry MUTATIONS, the
// owner→device command queue, and the instance-wide latest-version hint.

private struct DeviceIdInput: Encodable {
    let deviceId: String
}

private struct RenameDeviceInput: Encodable {
    let deviceId: String
    let label: String
}

/// EXP-622: `devices.setDefault` — flag/unflag the caller's default machine.
private struct SetDefaultInput: Encodable {
    let deviceId: String
    let isDefault: Bool
}

/// EXP-481: `devices.setShared` — the server input is `teamId: string | null`
/// where the key must ALWAYS be present (null clears the share). Synthesized
/// Encodable drops nil via encodeIfPresent, so this encodes by hand.
private struct SetSharedInput: Encodable {
    let deviceId: String
    let teamId: String?

    enum CodingKeys: String, CodingKey {
        case deviceId, teamId
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(deviceId, forKey: .deviceId)
        // Explicit null, never an absent key.
        try c.encode(teamId, forKey: .teamId)
    }
}

/// EXP-481: one agent's launch defaults in `devices.setLaunchDefaults` wire
/// form. Only set fields ride (synthesized encodeIfPresent) — the server
/// clamps vocabulary field-wise either way.
public struct AgentLaunchDefaultsInput: Encodable, Sendable {
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

/// EXP-481: the whole-object `launchDefaults` payload — the device settings
/// sheet sends the full edited struct (UI edits omit `expectedUpdatedAt`
/// server-side: unconditional last-write-wins between humans).
public struct DeviceLaunchDefaultsInput: Encodable, Sendable {
    public let defaultAgent: String?
    public let agents: [String: AgentLaunchDefaultsInput]?

    public init(defaultAgent: String? = nil, agents: [String: AgentLaunchDefaultsInput]? = nil) {
        self.defaultAgent = defaultAgent
        self.agents = agents
    }
}

private struct SetLaunchDefaultsInput: Encodable {
    let deviceId: String
    let launchDefaults: DeviceLaunchDefaultsInput
}

private struct CreateCommandInput: Encodable {
    let deviceId: String
    /// `worktree_remove` (repoFullName + branch required) | `worktree_prune` |
    /// `agent_login` (EXP-484: `agent` required, `switch` optional).
    let kind: String
    let repoFullName: String?
    let branch: String?
    /// EXP-484: contract `codingAgent` id for `agent_login` (`pi` is refused
    /// server-side — it has no remote sign-in).
    let agent: String?
    /// EXP-484: sign out first, then sign in as somebody else. `switch` is a
    /// Swift keyword, so the property is renamed and the wire key restored via
    /// CodingKeys; the server takes a real JSON boolean.
    let switchAccount: Bool?

    enum CodingKeys: String, CodingKey {
        case deviceId, kind, repoFullName, branch, agent
        case switchAccount = "switch"
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

    /// EXP-481: share / unshare one of the caller's SERVER machines with a
    /// team (nil clears — encoded as an explicit JSON null, the key is
    /// required). Sharing is the consent that lets teammates remote-start on
    /// the box; moving/clearing it ends their hosted runs server-side.
    public func setShared(accountId: String, deviceId: String, teamId: String?) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "devices.setShared",
            input: SetSharedInput(deviceId: deviceId, teamId: teamId)
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

    /// EXP-481: queue a worktree command for the device (owner-only). Runs on
    /// its next heartbeat — immediately when online (relay nudge), on return
    /// when offline. `worktree_remove` needs repoFullName + branch.
    /// EXP-484: `agent_login` needs `agent` (and optionally `switchAccount`) —
    /// the device runs the agent's own sign-in and completes the command early
    /// with the URL/code as its `result`.
    public func createCommand(
        accountId: String,
        deviceId: String,
        kind: String,
        repoFullName: String? = nil,
        branch: String? = nil,
        agent: String? = nil,
        switchAccount: Bool? = nil
    ) async throws -> CreatedDeviceCommand {
        try await trpc.mutation(
            accountId: accountId,
            path: "devices.createCommand",
            input: CreateCommandInput(
                deviceId: deviceId, kind: kind, repoFullName: repoFullName, branch: branch,
                agent: agent, switchAccount: switchAccount
            )
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
