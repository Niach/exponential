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
            if !(200...299).contains(httpStatus) {
                status.errorCount += 1
            }
            self.shapes[name] = status
        }
    }
}
