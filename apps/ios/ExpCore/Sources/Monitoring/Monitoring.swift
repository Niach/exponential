import Foundation
import os

/// #LOGS-2: crash, hang and performance reporting, routed THROUGH the server
/// the app is signed in to (never straight to GlitchTip). The server answers
/// `GET /api/monitoring/config?platform=ios` (public) with whether reporting
/// is on and the DSN to stamp envelopes with; envelopes go to the tunnel
/// `POST /api/monitoring`. Self-hosters and dev servers answer off.
///
/// Foundation only: the Sentry SDK lives in the app target
/// (`Observability.swift`); this is the part that can be unit-tested.
public enum Monitoring {
    public static let platform = "ios"
    /// The app-hang threshold (seconds) when the server names none.
    public static let defaultAppHangTimeout: TimeInterval = 2
    public static let defaultTracesSampleRate = 0.1
    public static let environments: Set<String> = ["prod", "staging", "dev"]

    /// The server's answer. `{ "enabled": false }` is the whole answer of an
    /// instance without reporting.
    public struct Config: Codable, Equatable, Sendable {
        public var enabled: Bool
        public var dsn: String?
        public var environment: String?
        public var tracesSampleRate: Double?
        /// Optional override of the app-hang threshold, in seconds.
        public var appHangTimeout: Double?

        public init(
            enabled: Bool,
            dsn: String? = nil,
            environment: String? = nil,
            tracesSampleRate: Double? = nil,
            appHangTimeout: Double? = nil
        ) {
            self.enabled = enabled
            self.dsn = dsn
            self.environment = environment
            self.tracesSampleRate = tracesSampleRate
            self.appHangTimeout = appHangTimeout
        }

        public static let off = Config(enabled: false)

        /// On only with a DSN to start the SDK with.
        public var isActive: Bool { enabled && !(dsn ?? "").isEmpty }

        public var resolvedTracesSampleRate: Double {
            guard let rate = tracesSampleRate, rate >= 0, rate <= 1 else { return Monitoring.defaultTracesSampleRate }
            return rate
        }

        public var resolvedAppHangTimeout: TimeInterval {
            guard let timeout = appHangTimeout, timeout > 0 else { return Monitoring.defaultAppHangTimeout }
            return timeout
        }
    }

    /// The last answer and the server it came from, so the next launch starts
    /// the SDK before any network (early crashes).
    public struct Cached: Codable, Equatable, Sendable {
        public var origin: String
        public var config: Config

        public init(origin: String, config: Config) {
            self.origin = origin
            self.config = config
        }
    }

    // MARK: - URLs

    public static func configUrl(origin: String) -> URL? {
        guard let base = WebLinks.normalizedBase(origin) else { return nil }
        return URL(string: "\(base)/api/monitoring/config?platform=\(platform)")
    }

    public static func tunnelUrl(origin: String) -> URL? {
        guard let base = WebLinks.normalizedBase(origin) else { return nil }
        return URL(string: "\(base)/api/monitoring")
    }

    /// The tunnel's own requests: never a span, a breadcrumb or a failed-
    /// request event (each would feed back into another envelope).
    public static func isTunnelRequest(_ url: String?) -> Bool {
        guard let url, let components = URLComponents(string: url) else { return false }
        return components.path.hasPrefix("/api/monitoring")
    }

    /// Whether `url` is one of our servers' (API + Electric shape) requests:
    /// same scheme, host and port as a signed-in instance.
    public static func isOwnRequest(_ url: String?, origins: [String]) -> Bool {
        guard let target = origin(of: url), !isTunnelRequest(url) else { return false }
        return origins.contains { origin(of: $0) == target }
    }

    /// `scheme://host[:port]`, lowercased; nil for anything without a host.
    static func origin(of url: String?) -> String? {
        guard let url = WebLinks.normalizedBase(url),
              let components = URLComponents(string: url),
              let scheme = components.scheme?.lowercased(),
              let host = components.host?.lowercased(), !host.isEmpty
        else { return nil }
        let port = components.port.map { ":\($0)" } ?? ""
        return "\(scheme)://\(host)\(port)"
    }

    /// The `tracePropagationTargets` / `failedRequestTargets` patterns: one
    /// anchored regex per signed-in instance.
    public static func targetPatterns(origins: [String]) -> [String] {
        Set(origins.compactMap(origin(of:)))
            .sorted()
            .map { "^\(NSRegularExpression.escapedPattern(for: $0))(/|$)" }
    }

    // MARK: - Release + environment

    /// `exponential-ios@<CFBundleShortVersionString>+<CFBundleVersion>`.
    public static func release(info: [String: Any]?) -> String {
        let version = info?["CFBundleShortVersionString"] as? String ?? "0.0.0"
        let build = info?["CFBundleVersion"] as? String ?? "0"
        return "exponential-ios@\(version)+\(build)"
    }

    /// Debug builds are always `dev`; otherwise the server's answer, else
    /// derived from the server (the staging cloud = `staging`).
    public static func environment(config: Config, origin: String, isDebug: Bool) -> String {
        if isDebug { return "dev" }
        if let named = config.environment, environments.contains(named) { return named }
        let base = WebLinks.normalizedBase(origin)
        return base == WebLinks.normalizedBase(AppConstants.stagingCloudUrl) ? "staging" : "prod"
    }

    // MARK: - Local state (per device)

    public static let sendDiagnosticsKey = "monitoring.sendDiagnostics"
    static let cachedConfigKey = "monitoring.config.v1"

    /// The Settings › Account › Privacy toggle; default on.
    public static func sendDiagnostics(_ defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: sendDiagnosticsKey) as? Bool ?? true
    }

    public static func cached(_ defaults: UserDefaults = .standard) -> Cached? {
        guard let data = defaults.data(forKey: cachedConfigKey) else { return nil }
        return try? JSONDecoder().decode(Cached.self, from: data)
    }

    public static func store(_ cached: Cached, _ defaults: UserDefaults = .standard) {
        guard let data = try? JSONEncoder().encode(cached) else { return }
        defaults.set(data, forKey: cachedConfigKey)
    }

    // MARK: - Fetch

    /// The server's answer. A server without the route (404, an older or a
    /// self-hosted build) answers off; a transport failure throws so the
    /// cached answer stays.
    public static func fetchConfig(origin: String, session: URLSession) async throws -> Config {
        guard let url = configUrl(origin: origin) else { return .off }
        var request = URLRequest(url: url)
        request.timeoutInterval = 15
        request.setValue(AppConstants.clientVersionHeaderValue, forHTTPHeaderField: "x-client-version")
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else { return .off }
        if (500..<600).contains(http.statusCode) { throw URLError(.badServerResponse) }
        guard (200..<300).contains(http.statusCode) else { return .off }
        return decodeConfig(data)
    }

    public static func decodeConfig(_ data: Data) -> Config {
        (try? JSONDecoder().decode(Config.self, from: data)) ?? .off
    }
}

// MARK: - Scrubber

extension Monitoring {
    /// beforeSend / beforeBreadcrumb / beforeSendSpan: never ship an
    /// `Authorization`, a cookie or anything keyed like a credential.
    public enum Scrubber {
        public static let filtered = "[Filtered]"

        private static let sensitivePattern = try! NSRegularExpression(
            pattern: "token|secret|password|key|ticket|session|dsn|authorization|cookie",
            options: [.caseInsensitive]
        )

        public static func isSensitive(_ key: String) -> Bool {
            sensitivePattern.firstMatch(in: key, range: NSRange(key.startIndex..., in: key)) != nil
        }

        /// Recursively filters every value under a sensitive key (dictionaries
        /// and arrays of them, as the SDK's `data`/`extra` carry).
        public static func scrub(_ value: Any) -> Any {
            if let dictionary = value as? [String: Any] { return scrub(dictionary) }
            if let array = value as? [Any] { return array.map(scrub) }
            if let string = value as? String, string.contains("://") || string.contains("=") {
                return scrubUrl(string)
            }
            return value
        }

        public static func scrub(_ dictionary: [String: Any]) -> [String: Any] {
            var out: [String: Any] = [:]
            for (key, value) in dictionary {
                out[key] = isSensitive(key) ? filtered : scrub(value)
            }
            return out
        }

        public static func scrub(headers: [String: String]) -> [String: String] {
            var out: [String: String] = [:]
            for (key, value) in headers {
                out[key] = isSensitive(key) ? filtered : value
            }
            return out
        }

        /// A URL keeps its path; its credentials and sensitive query values go.
        public static func scrubUrl(_ string: String) -> String {
            if !string.contains("://") { return scrubQuery(string) }
            guard var components = URLComponents(string: string) else { return string }
            let hadCredentials = components.user != nil || components.password != nil
            components.user = nil
            components.password = nil
            let items = components.queryItems
            let scrubbedItems = items?.map { item in
                isSensitive(item.name) ? URLQueryItem(name: item.name, value: filtered) : item
            }
            if !hadCredentials, scrubbedItems == items { return string }
            components.queryItems = scrubbedItems
            return components.string ?? string
        }

        /// `a=1&token=x` → `a=1&token=[Filtered]`.
        public static func scrubQuery(_ query: String) -> String {
            query
                .split(separator: "&", omittingEmptySubsequences: false)
                .map { pair -> String in
                    let parts = pair.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
                    guard parts.count == 2, isSensitive(String(parts[0])) else { return String(pair) }
                    return "\(parts[0])=\(filtered)"
                }
                .joined(separator: "&")
        }
    }
}

/// The instances whose requests count as ours, read from the SDK's callbacks
/// on any thread.
public final class MonitoringState: Sendable {
    public static let shared = MonitoringState()

    private struct State {
        var origins: [String] = []
        var tunnelUrl: URL?
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    public var origins: [String] { state.withLock { $0.origins } }
    public var tunnelUrl: URL? { state.withLock { $0.tunnelUrl } }

    public func setOrigins(_ origins: [String]) { state.withLock { $0.origins = origins } }
    public func setTunnelUrl(_ url: URL?) { state.withLock { $0.tunnelUrl = url } }
}
