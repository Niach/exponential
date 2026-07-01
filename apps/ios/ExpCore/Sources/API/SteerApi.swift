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

    /// Dial URL with `?ticket=` appended — the relay reads the ticket from the
    /// query string (browsers can't set WS headers; the desktop mirrors that).
    /// Assumes `url` is the full `wss://<relay>/ws` endpoint.
    public func connectURL() -> URL? {
        guard let url, let ticket, var comps = URLComponents(string: url) else { return nil }
        var items = comps.queryItems ?? []
        items.append(URLQueryItem(name: "ticket", value: ticket))
        comps.queryItems = items
        return comps.url
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
}
