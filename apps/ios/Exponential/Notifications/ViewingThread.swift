import ExpCore
import ExpPush
import Foundation
import GRDB

/// #EXP-1815 (notification-delivery.json THREAD, `viewing`): the current
/// screen's thread, the key `willPresent` compares a foreground push with.
/// An issue's page = `issue:<id>`, a chat or session page = `session:<rootId>`,
/// a device's settings = `device:<rowId>`, the Inbox = `inbox`; any other
/// screen = nil. The navigator publishes its top route's, the Devices page
/// its open settings sheet's.
@MainActor
final class ViewingThread {
    static let shared = ViewingThread()

    /// The navigator's top route's thread.
    private(set) var route: String?
    /// The open device settings sheet (it covers the route).
    var device: String?
    private var generation = 0

    var current: String? { device ?? route }

    /// Resolves `route` (a session's root and an agent's conversation read
    /// off the store) and publishes it unless a later route won meanwhile.
    func publish(_ route: AppRoute?, deps: AppDependencies) {
        generation += 1
        let mine = generation
        guard let route else {
            self.route = nil
            return
        }
        if let thread = Self.thread(route) {
            self.route = thread
            return
        }
        self.route = nil
        Task {
            let thread = await Self.storedThread(route, deps: deps)
            if mine == generation { self.route = thread }
        }
    }

    /// The thread a route names outright (nil = none or a store read).
    static func thread(_ route: AppRoute) -> String? {
        switch route {
        case let .issue(_, id), let .issueComment(_, id, _): "issue:\(id)"
        case let .chat(_, chatId, _): "session:\(chatId)"
        case .myWork: NotificationDelivery.inboxThread
        default: nil
        }
    }

    private static func storedThread(_ route: AppRoute, deps: AppDependencies) async -> String? {
        switch route {
        case let .agentSession(accountId, sessionId):
            let rootId = try? await deps.db.pool(forAccountId: accountId).read { db in
                try SessionEntity.fetchOne(db, key: sessionId).map { $0.rootId.isEmpty ? $0.id : $0.rootId }
            }
            return "session:\(rootId ?? sessionId)"
        case let .agentChannel(accountId, agentId, _):
            guard let userId = deps.auth.accounts.first(where: { $0.id == accountId })?.userId else { return nil }
            let rootId = try? await deps.db.pool(forAccountId: accountId).read { db in
                try SessionQueries.conversation(db: db, agentId: agentId, userId: userId)?.id
            }
            return rootId.map { "session:\($0)" }
        default:
            return nil
        }
    }
}
