import Foundation

@Observable
final class SyncDebug: @unchecked Sendable {
    static let shared = SyncDebug()

    var lastMessages: [String] = []

    func log(_ message: String) {
        Task { @MainActor in
            self.lastMessages.append(message)
            if self.lastMessages.count > 15 {
                self.lastMessages.removeFirst()
            }
        }
    }
}
