import Foundation

// Mirrors apps/web/src/lib/trpc/devices.ts (EXP-403): the per-user machine
// registry. Desktops and headless `exponential` daemon servers register
// themselves and heartbeat; `list` merges those durable rows with live
// steer-relay presence, so one row carries both identity (label, kind, last
// seen, version) and the live advertisement (online, agents, caps).
// Deliberately tRPC and NOT an Electric shape — per-user machine state, not
// team product data — so the surfaces that show machines POLL it.
//
// The row type is `SteerDevice` (see SteerApi.swift): relay-presence rows and
// registry rows share one shape, exactly like web's lib/steer-devices.ts.

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
    /// must gate on `SteerDevice.isOnline` themselves.
    public func list(accountId: String) async throws -> [SteerDevice] {
        let result: SteerDevicesResult = try await trpc.query(accountId: accountId, path: "devices.list")
        return result.devices
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
