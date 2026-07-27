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
/// It also restarts when the path CHANGES while staying satisfied (EXP-304): a
/// VPN establishing its tunnel, or wifi ⇄ cellular, swaps the interface set and
/// the DNS resolver under us without the status ever leaving `.satisfied`. That
/// window is when shapes burn DNS/connect failures, and nothing used to wake
/// them afterwards because no network had "returned".
///
/// `@unchecked Sendable` for the same reason PushTokenManager is: the class is
/// held for the app's lifetime and handed to escaping callbacks. Its only
/// mutable state is confined to `queue` — NWPathMonitor delivers every update
/// there, serially — so there is nothing to synchronize.
final class NetworkPathWatcher: @unchecked Sendable {
    private let syncManager: SyncManager
    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "at.exponential.network-path")
    // Both confined to `queue`: only the path-update handler ever touches them.
    private var wasSatisfied: Bool?
    private var lastInterfaces: String?

    init(syncManager: SyncManager) {
        self.syncManager = syncManager
    }

    func start() {
        monitor.pathUpdateHandler = { [weak self] path in
            guard let self else { return }
            let satisfied = path.status == .satisfied
            let interfaces = path.availableInterfaces.map(\.name).sorted().joined(separator: ",")
            let previous = self.wasSatisfied
            let previousInterfaces = self.lastInterfaces
            self.wasSatisfied = satisfied
            self.lastInterfaces = interfaces
            // The first callback (previous == nil) just reports the state at
            // start — never a wake signal.
            guard satisfied else { return }
            let regained = previous == false
            let pathChanged = previous == true && previousInterfaces != interfaces
            guard regained || pathChanged else { return }
            let reason = regained ? "network regained" : "network path changed"
            logger.info("\(reason) — restarting shape pipelines")
            let syncManager = self.syncManager
            Task {
                // restartAllPipelines has its own 5s floor, so even a burst of
                // path updates during a VPN handshake costs one restart.
                await syncManager.restartAllPipelines(reason: reason)
            }
        }
        monitor.start(queue: queue)
    }

    func stop() {
        monitor.cancel()
    }
}
