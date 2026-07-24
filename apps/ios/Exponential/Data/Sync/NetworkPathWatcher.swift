import ExpCore
import Foundation
import Network
import os

private let logger = Logger(subsystem: "at.exponential", category: "NetworkPathWatcher")

/// Restarts every account's shape pipeline the moment the device REGAINS
/// connectivity (EXP-264). Without this, a tunnel/airplane-mode/dead-wifi gap
/// leaves each shape loop backed off at its 30s ceiling and the app looks
/// frozen for far longer than the outage itself.
///
/// `@unchecked Sendable` for the same reason PushTokenManager is: the class is
/// held for the app's lifetime and handed to escaping callbacks. Its only
/// mutable state (`wasSatisfied`) is confined to `queue` — NWPathMonitor
/// delivers every update there, serially — so there is nothing to synchronize.
final class NetworkPathWatcher: @unchecked Sendable {
    private let syncManager: SyncManager
    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "at.exponential.network-path")
    // Confined to `queue`: only the path-update handler ever touches it.
    private var wasSatisfied: Bool?

    init(syncManager: SyncManager) {
        self.syncManager = syncManager
    }

    func start() {
        monitor.pathUpdateHandler = { [weak self] path in
            guard let self else { return }
            let satisfied = path.status == .satisfied
            let previous = self.wasSatisfied
            self.wasSatisfied = satisfied
            // Only the unsatisfied → satisfied EDGE is a wake signal. The first
            // callback (previous == nil) just reports the state at start, and
            // satisfied → satisfied churn (wifi ⇄ cellular, interface changes)
            // is noise — the restart's own 5s floor absorbs the rest.
            guard previous == false, satisfied else { return }
            logger.info("Network regained — restarting shape pipelines")
            let syncManager = self.syncManager
            Task {
                await syncManager.restartAllPipelines(reason: "network regained")
            }
        }
        monitor.start(queue: queue)
    }

    func stop() {
        monitor.cancel()
    }
}
