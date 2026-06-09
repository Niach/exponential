import Foundation

public struct ShapeStatus: Sendable {
    public var lastHttpStatus: Int = 0
    public var requestCount: Int = 0
    public var errorCount: Int = 0
    public var lastActivityAt: Date = .now
    public var isLive: Bool = false

    public init(
        lastHttpStatus: Int = 0,
        requestCount: Int = 0,
        errorCount: Int = 0,
        lastActivityAt: Date = .now,
        isLive: Bool = false
    ) {
        self.lastHttpStatus = lastHttpStatus
        self.requestCount = requestCount
        self.errorCount = errorCount
        self.lastActivityAt = lastActivityAt
        self.isLive = isLive
    }
}

@Observable
public final class SyncDebug: @unchecked Sendable {
    public static let shared = SyncDebug()

    public var shapes: [String: ShapeStatus] = [:]
    public var lastMessages: [String] = []
    // Aggregate health for the UI's sync banner: any 2xx response counts as a
    // success; transport failures (no HTTP response at all) and non-2xx mark
    // the last error. Healthy = the most recent signal was a success.
    public var lastSuccessAt: Date?
    public var lastErrorAt: Date?
    public var lastErrorWasUnauthorized = false

    /// What the sync banner should say, if anything. Degraded only after the
    /// errors persist a moment (a single failed long-poll mustn't flash a
    /// banner — ShapeClient retries with backoff).
    public enum Health: Equatable, Sendable {
        case ok
        case offline
        case unauthorized
    }

    public var health: Health {
        guard let err = lastErrorAt else { return .ok }
        if let ok = lastSuccessAt, ok > err { return .ok }
        // Require a few seconds of sustained failure before alarming.
        guard Date().timeIntervalSince(err) < 300 else { return .ok }
        if let ok = lastSuccessAt, Date().timeIntervalSince(ok) < 8 { return .ok }
        return lastErrorWasUnauthorized ? .unauthorized : .offline
    }

    public func log(_ message: String) {
        Task { @MainActor in
            self.lastMessages.append(message)
            if self.lastMessages.count > 50 {
                self.lastMessages.removeFirst()
            }
        }
    }

    public func reportShape(name: String, httpStatus: Int, isLive: Bool) {
        Task { @MainActor in
            var status = self.shapes[name] ?? ShapeStatus()
            status.lastHttpStatus = httpStatus
            status.requestCount += 1
            status.lastActivityAt = .now
            status.isLive = isLive
            if (200...299).contains(httpStatus) {
                self.lastSuccessAt = .now
            } else {
                status.errorCount += 1
                self.lastErrorAt = .now
                self.lastErrorWasUnauthorized = httpStatus == 401
            }
            self.shapes[name] = status
        }
    }

    /// A request that never produced an HTTP response (network down, DNS, TLS).
    public func reportTransportError(name _: String) {
        Task { @MainActor in
            self.lastErrorAt = .now
            self.lastErrorWasUnauthorized = false
        }
    }
}
