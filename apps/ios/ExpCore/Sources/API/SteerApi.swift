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

/// One online desktop of the current user (relay presence, no DB row).
public struct SteerDevice: Decodable, Sendable, Identifiable {
    public let deviceId: String
    public let deviceLabel: String
    public let connectedAt: Double

    public var id: String { deviceId }

    public init(deviceId: String, deviceLabel: String, connectedAt: Double) {
        self.deviceId = deviceId
        self.deviceLabel = deviceLabel
        self.connectedAt = connectedAt
    }
}

public struct SteerDevicesResult: Decodable, Sendable {
    public let devices: [SteerDevice]

    public init(devices: [SteerDevice]) {
        self.devices = devices
    }
}

private struct ControlTicketInput: Encodable {
    let kind = "control"
    let deviceLabel: String?
}

private struct PublisherTicketInput: Encodable {
    let kind = "publisher"
    let codingSessionId: String
}

private struct ViewerTicketInput: Encodable {
    let kind = "viewer"
    let codingSessionId: String
}

private struct StartSessionInput: Encodable {
    let issueId: String
    let deviceId: String
}

private struct StartSessionResult: Decodable {
    let ok: Bool
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

    /// Mint a `control` ticket for the desktop's device-presence socket.
    public func mintControlTicket(accountId: String, deviceLabel: String?) async throws -> SteerTicket {
        try await trpc.mutation(
            accountId: accountId,
            path: "steer.mintTicket",
            input: ControlTicketInput(deviceLabel: deviceLabel)
        )
    }

    /// Mint a `publisher` ticket for a specific coding session's data socket.
    public func mintPublisherTicket(accountId: String, codingSessionId: String) async throws -> SteerTicket {
        try await trpc.mutation(
            accountId: accountId,
            path: "steer.mintTicket",
            input: PublisherTicketInput(codingSessionId: codingSessionId)
        )
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

    /// The caller's online desktops (`steer.myDevices` query) — powers the
    /// "Start on my desktop" button/picker. Relay-off ⇒ empty list.
    public func myDevices(accountId: String) async throws -> [SteerDevice] {
        let result: SteerDevicesResult = try await trpc.query(accountId: accountId, path: "steer.myDevices")
        return result.devices
    }

    /// Remote "Start on my desktop": route a `start_session` to the chosen
    /// online device. Throws `SteerStartError.rejected` with the server's
    /// human-readable reason on PRECONDITION_FAILED (device offline, no repo
    /// linked, relay off) so the UI can surface it verbatim.
    public func startSession(accountId: String, issueId: String, deviceId: String) async throws {
        do {
            let _: StartSessionResult = try await trpc.mutation(
                accountId: accountId,
                path: "steer.startSession",
                input: StartSessionInput(issueId: issueId, deviceId: deviceId)
            )
        } catch let TrpcError.httpError(status, body) {
            if let message = Self.trpcErrorMessage(fromBody: body) {
                throw SteerStartError.rejected(message)
            }
            throw TrpcError.httpError(status, body)
        }
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
