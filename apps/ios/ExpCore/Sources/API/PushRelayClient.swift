import Foundation

// EXP-1435: the push relay's accountless API (apps/push-relay/README.md) as
// the iOS app uses it: ONE install per relay URL (the APNs device token +
// topic + environment), one capability per (relay, signed-in account) that the
// account's server pushes with. Talks to the relay with its OWN URLSession and
// never through HTTPClient: no account bearer may reach the relay, and a
// relay 401 must never trip the account's dead-session gate.

public enum PushRelayError: Error, Equatable, Sendable {
    /// 401/404: the install (or its secret) is unknown to the relay; forget it
    /// and register again.
    case gone
    /// Any other non-2xx (503 = the relay has no APNs credentials yet).
    case status(Int)
    case invalidResponse
    case invalidUrl
    case keychain
}

public struct PushRelayInstall: Codable, Equatable, Sendable {
    public var installId: String
    public var installSecret: String
}

public struct PushRelayCapability: Codable, Equatable, Sendable {
    public var capabilityId: String
    public var capability: String
}

/// The relay's HTTP surface, behind a protocol so the registrar's state
/// machine is testable without a network.
public protocol PushRelayTransport: Sendable {
    func createInstall(relay: String, token: String, topic: String, environment: String) async throws -> PushRelayInstall
    func updateInstall(relay: String, install: PushRelayInstall, token: String, environment: String) async throws
    func mintCapability(relay: String, install: PushRelayInstall, label: String) async throws -> PushRelayCapability
    func revokeCapability(relay: String, install: PushRelayInstall, capabilityId: String) async throws
}

public final class PushRelayClient: PushRelayTransport {
    private let session: URLSession

    public init() {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 15
        config.httpShouldSetCookies = false
        config.httpCookieStorage = nil
        config.urlCache = nil
        config.httpAdditionalHeaders = ["Accept": "application/json"]
        session = URLSession(configuration: config)
    }

    public func createInstall(relay: String, token: String, topic: String, environment: String) async throws -> PushRelayInstall {
        let body: [String: String] = ["platform": "apns", "token": token, "topic": topic, "environment": environment]
        let data = try await send(relay, path: "/installs", method: "POST", bearer: nil, body: body)
        return try decode(PushRelayInstall.self, data)
    }

    public func updateInstall(relay: String, install: PushRelayInstall, token: String, environment: String) async throws {
        let body: [String: String] = ["token": token, "environment": environment]
        _ = try await send(relay, path: "/installs/\(try segment(install.installId))", method: "PUT", bearer: install.installSecret, body: body)
    }

    public func mintCapability(relay: String, install: PushRelayInstall, label: String) async throws -> PushRelayCapability {
        let body: [String: String] = ["label": String(label.prefix(64))]
        let data = try await send(
            relay,
            path: "/installs/\(try segment(install.installId))/capabilities",
            method: "POST",
            bearer: install.installSecret,
            body: body
        )
        return try decode(PushRelayCapability.self, data)
    }

    public func revokeCapability(relay: String, install: PushRelayInstall, capabilityId: String) async throws {
        _ = try await send(
            relay,
            path: "/installs/\(try segment(install.installId))/capabilities/\(try segment(capabilityId))",
            method: "DELETE",
            bearer: install.installSecret,
            body: nil
        )
    }

    // MARK: - Private

    private func segment(_ raw: String) throws -> String {
        guard let encoded = raw.addingPercentEncoding(withAllowedCharacters: .alphanumerics.union(CharacterSet(charactersIn: "-_."))),
              !encoded.isEmpty else { throw PushRelayError.invalidUrl }
        return encoded
    }

    private func decode<T: Decodable>(_ type: T.Type, _ data: Data) throws -> T {
        do { return try JSONDecoder().decode(type, from: data) } catch { throw PushRelayError.invalidResponse }
    }

    private func send(_ relay: String, path: String, method: String, bearer: String?, body: [String: String]?) async throws -> Data {
        let base = relay.hasSuffix("/") ? String(relay.dropLast()) : relay
        guard let url = URL(string: base + path) else { throw PushRelayError.invalidUrl }
        var request = URLRequest(url: url)
        request.httpMethod = method
        if let bearer { request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization") }
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONEncoder().encode(body)
        }
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw PushRelayError.invalidResponse }
        switch http.statusCode {
        case 200...299: return data
        case 401, 404: throw PushRelayError.gone
        default: throw PushRelayError.status(http.statusCode)
        }
    }
}

/// The device's relay installs and capabilities, persisted in the keychain
/// (they are bearer secrets). Serialized by the actor; every network step
/// re-reads nothing it did not just write.
public actor PushRelayRegistrar {
    struct Install: Codable, Equatable {
        var install: PushRelayInstall
        var token: String
        var environment: String
        /// accountId → its capability on this install.
        var capabilities: [String: PushRelayCapability]
    }

    struct State: Codable, Equatable {
        /// relay URL → its install.
        var installs: [String: Install] = [:]
    }

    public struct Result: Sendable, Equatable {
        public let capability: String
        /// Accounts whose capability died with a forgotten install: their
        /// server rows hold a dead capability and must register again.
        public let resetAccounts: [String]
    }

    static let keychainKey = "pushRelay.v1"

    private let keychain: KeychainStoring
    private let transport: PushRelayTransport
    private let topic: String
    private let environment: String
    /// Relays whose install this process has confirmed with a PUT: a 410
    /// deletes the install on the relay, so the first use per launch checks.
    private var verified: Set<String> = []
    /// Accounts whose capability died with a forgotten install, reported
    /// (and cleared) by the next successful `capability` call.
    private var orphaned: Set<String> = []

    public init(keychain: KeychainStoring, transport: PushRelayTransport, topic: String, environment: String) {
        self.keychain = keychain
        self.transport = transport
        self.topic = topic
        self.environment = environment
    }

    /// The capability `accountId`'s server pushes with through `relay`,
    /// creating/refreshing the install for `apnsToken` and minting the
    /// capability on first use. Throws on any relay failure (the caller
    /// registers legacy this pass).
    public func capability(relay rawRelay: String, accountId: String, label: String, apnsToken: String) async throws -> Result {
        let relay = Self.normalize(rawRelay)
        var state = try load()

        if var current = state.installs[relay],
           !verified.contains(relay) || current.token != apnsToken || current.environment != environment {
            do {
                try await transport.updateInstall(relay: relay, install: current.install, token: apnsToken, environment: environment)
                current.token = apnsToken
                current.environment = environment
                state.installs[relay] = current
                try save(state)
                verified.insert(relay)
            } catch PushRelayError.gone {
                orphaned.formUnion(current.capabilities.keys)
                state.installs[relay] = nil
                try save(state)
            }
        }

        var install: Install
        if let existing = state.installs[relay] {
            install = existing
        } else {
            let created = try await transport.createInstall(relay: relay, token: apnsToken, topic: topic, environment: environment)
            install = Install(install: created, token: apnsToken, environment: environment, capabilities: [:])
            state.installs[relay] = install
            try save(state)
            verified.insert(relay)
        }

        if let existing = install.capabilities[accountId] {
            return result(existing.capability, for: accountId)
        }
        do {
            let minted = try await transport.mintCapability(relay: relay, install: install.install, label: label)
            install.capabilities[accountId] = minted
            state.installs[relay] = install
            try save(state)
            return result(minted.capability, for: accountId)
        } catch PushRelayError.gone {
            // The install vanished between the PUT and the mint: forget it;
            // the next pass registers a fresh one.
            orphaned.formUnion(install.capabilities.keys)
            state.installs[relay] = nil
            verified.remove(relay)
            try? save(state)
            throw PushRelayError.gone
        }
    }

    /// Sign-out: revokes `accountId`'s capabilities (best effort) and forgets
    /// them, whatever the relay answers.
    public func revoke(accountId: String) async {
        guard var state = try? load() else { return }
        var revocations: [(String, PushRelayInstall, String)] = []
        for (relay, install) in state.installs {
            guard let cap = install.capabilities[accountId] else { continue }
            revocations.append((relay, install.install, cap.capabilityId))
            state.installs[relay]?.capabilities[accountId] = nil
        }
        guard !revocations.isEmpty else { return }
        try? save(state)
        for (relay, install, capabilityId) in revocations {
            try? await transport.revokeCapability(relay: relay, install: install, capabilityId: capabilityId)
        }
    }

    // MARK: - Private

    private func result(_ capability: String, for accountId: String) -> Result {
        orphaned.remove(accountId)
        let reset = orphaned.sorted()
        orphaned.removeAll()
        return Result(capability: capability, resetAccounts: reset)
    }

    static func normalize(_ relay: String) -> String {
        var url = relay.trimmingCharacters(in: .whitespacesAndNewlines)
        while url.hasSuffix("/") { url.removeLast() }
        return url
    }

    private func load() throws -> State {
        let raw: String?
        do { raw = try keychain.get(Self.keychainKey) } catch { throw PushRelayError.keychain }
        guard let raw, let data = raw.data(using: .utf8) else { return State() }
        // A corrupt blob is dropped: worst case a fresh install.
        return (try? JSONDecoder().decode(State.self, from: data)) ?? State()
    }

    private func save(_ state: State) throws {
        guard let data = try? JSONEncoder().encode(state),
              let string = String(data: data, encoding: .utf8),
              keychain.set(Self.keychainKey, value: string) else { throw PushRelayError.keychain }
    }
}
