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
}
