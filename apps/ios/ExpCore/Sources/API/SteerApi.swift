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
    /// Coding agents this desktop can launch (contract `codingAgentValues`).
    /// Absent = an older desktop that only runs claude.
    public let agents: [String]?
    /// Feature capabilities the desktop advertised (EXP-253: `actions`).
    /// Absent (old desktop/relay) = none — action starts are strictly gated
    /// on this, unlike the lenient agents fallback.
    public let caps: [String]?

    public var id: String { deviceId }

    public init(
        deviceId: String,
        deviceLabel: String,
        connectedAt: Double,
        agents: [String]? = nil,
        caps: [String]? = nil
    ) {
        self.deviceId = deviceId
        self.deviceLabel = deviceLabel
        self.connectedAt = connectedAt
        self.agents = agents
        self.caps = caps
    }

    /// Whether this desktop can run team actions (EXP-253).
    public var canRunActions: Bool { caps?.contains("actions") == true }
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

    public init(
        agent: String? = nil,
        model: String? = nil,
        effort: String? = nil,
        ultracode: Bool? = nil,
        planMode: Bool? = nil,
        skipPermissions: Bool? = nil
    ) {
        self.agent = agent
        self.model = model
        self.effort = effort
        self.ultracode = ultracode
        self.planMode = planMode
        self.skipPermissions = skipPermissions
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

/// Action remote-start (EXP-253): exactly one of issueId/issueIds/actionId is
/// present on `steer.startSession` — this is the actionId form. Action runs
/// are Claude-only v1, so model/effort are the ONLY options that may ride
/// (the server rejects agent/ultracode/planMode/skipPermissions here). Nil
/// fields are omitted (synthesized Encodable uses encodeIfPresent) and mean
/// "desktop settings default".
private struct StartActionSessionInput: Encodable {
    let actionId: String
    let deviceId: String
    let model: String?
    let effort: String?
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

    /// Mint a `control` ticket for a device-presence socket.
    /// Retained for a future phone→desktop remote-input surface; not yet wired to UI.
    public func mintControlTicket(accountId: String, deviceLabel: String?) async throws -> SteerTicket {
        try await trpc.mutation(
            accountId: accountId,
            path: "steer.mintTicket",
            input: ControlTicketInput(deviceLabel: deviceLabel)
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

    /// Action remote-start (EXP-253): route a `start_session` carrying an
    /// actionId to the chosen desktop — the device must advertise the
    /// `actions` capability (`SteerDevice.canRunActions`; the server enforces
    /// it too). Claude-only v1: model/effort are the only options. Same
    /// endpoint and PRECONDITION_FAILED → `SteerStartError.rejected` mapping
    /// as the issue forms.
    public func startSession(
        accountId: String,
        actionId: String,
        deviceId: String,
        model: String? = nil,
        effort: String? = nil
    ) async throws {
        do {
            let _: StartSessionResult = try await trpc.mutation(
                accountId: accountId,
                path: "steer.startSession",
                input: StartActionSessionInput(
                    actionId: actionId,
                    deviceId: deviceId,
                    model: model,
                    effort: effort
                )
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
