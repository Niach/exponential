import Foundation

struct ShapeStatus: Sendable {
    var lastHttpStatus: Int = 0
    var requestCount: Int = 0
    var errorCount: Int = 0
    var lastActivityAt: Date = .now
    var isLive: Bool = false
}

@Observable
final class SyncDebug: @unchecked Sendable {
    static let shared = SyncDebug()

    var shapes: [String: ShapeStatus] = [:]
    var lastMessages: [String] = []

    func log(_ message: String) {
        Task { @MainActor in
            self.lastMessages.append(message)
            if self.lastMessages.count > 50 {
                self.lastMessages.removeFirst()
            }
        }
    }

    func reportShape(name: String, httpStatus: Int, isLive: Bool) {
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
