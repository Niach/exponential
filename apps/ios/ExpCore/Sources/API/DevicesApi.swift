import Foundation

// Mirrors apps/web/src/lib/trpc/devices.ts (EXP-403): the per-user machine
// registry. Desktops and headless `exponential` daemon servers register
// themselves and heartbeat; `list` merges those durable rows with live
// steer-relay presence, so one row carries both identity (label, kind, last
// seen, version) and the live advertisement (online, agents, unauthedAgents,
// caps — EXP-409: `agents` is what the machine can RUN, `unauthedAgents` what
// it has installed but signed out of).
// Deliberately tRPC and NOT an Electric shape — per-user machine state, not
// team product data — so the surfaces that show machines POLL it. EXP-432
// bends that per-user rule exactly once: `list({teamId})` also returns
// teammates' SERVER machines shared with the team, which members may start on
// but never manage (sharing itself is web-only).
//
// The row type is `SteerDevice` (see SteerApi.swift): relay-presence rows and
// registry rows share one shape, exactly like web's lib/steer-devices.ts.

private struct ListDevicesInput: Encodable {
    let teamId: String
}

private struct DeviceIdInput: Encodable {
    let deviceId: String
}

private struct RenameDeviceInput: Encodable {
    let deviceId: String
    let label: String
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
    /// `worktree_remove` (repoFullName + branch required) | `worktree_prune`.
    let kind: String
    let repoFullName: String?
    let branch: String?
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

    /// The caller's machines (`devices.list` query), most recently seen first
    /// — ONLINE AND OFFLINE, unlike `steer.myDevices`, so start affordances
    /// must gate on `SteerDevice.isOnline` themselves. The envelope also
    /// carries `latestVersions` (EXP-420: gates the Update affordance).
    ///
    /// EXP-432: with [teamId] the response APPENDS teammates' server machines
    /// shared with that team (each carrying `owner`; `isMine` tells them
    /// apart). Without it the response is the caller's own machines exactly as
    /// before — which is also what an older server answers either way.
    ///
    /// A rejected team-scoped call RETRIES un-scoped: a self-hosted instance
    /// predating EXP-432 (or a team the caller has just left) must cost the
    /// user their shared machines, never their own list.
    public func list(accountId: String, teamId: String? = nil) async throws -> SteerDevicesResult {
        guard let teamId else {
            return try await trpc.query(accountId: accountId, path: "devices.list")
        }
        do {
            return try await trpc.query(
                accountId: accountId,
                path: "devices.list",
                input: ListDevicesInput(teamId: teamId)
            )
        } catch {
            return try await trpc.query(accountId: accountId, path: "devices.list")
        }
    }

    /// The machines a remote start can reach right now — what the surfaces
    /// that only need a launch target ask for, instead of `steer.myDevices`
    /// (own presence only, which would hide every start affordance when the
    /// sole online machine is a teammate's shared one, EXP-432). Team-scoped
    /// and pre-filtered to `isOnline`, since `list` returns offline rows too.
    public func onlineStartTargets(
        accountId: String,
        teamId: String?
    ) async throws -> [SteerDevice] {
        try await list(accountId: accountId, teamId: teamId).devices.filter(\.isOnline)
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
    public func createCommand(
        accountId: String,
        deviceId: String,
        kind: String,
        repoFullName: String? = nil,
        branch: String? = nil
    ) async throws -> CreatedDeviceCommand {
        try await trpc.mutation(
            accountId: accountId,
            path: "devices.createCommand",
            input: CreateCommandInput(
                deviceId: deviceId, kind: kind, repoFullName: repoFullName, branch: branch
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
