import BackgroundTasks
import ExpCore
import Foundation
import os

private let logger = Logger(subsystem: "at.exponential", category: "ChatCatchUp")

/// #EDE-114 THE PHONE WALK in the background (chat-store.json): only inside
/// the OS's own window, a `BGProcessingTask` that asks for the network AND
/// external power; the OS ending the window stops the walk where it is (the
/// store's watermarks keep the place). The identifier is listed in
/// Info.plist's `BGTaskSchedulerPermittedIdentifiers` (Project.swift).
enum ChatCatchUpBackground {
    static let identifier = "at.exponential.chat-store.catch-up"
    /// The earliest a window is asked for after the last one.
    static let interval: TimeInterval = 15 * 60
    /// How long a window waits for the node and the stores to come up.
    static let readyWait: Duration = .seconds(20)

    /// Before the app finishes launching (ExponentialApp.init).
    static func register() {
        guard ChatStoreHub.shared.isAvailable else { return }
        BGTaskScheduler.shared.register(forTaskWithIdentifier: identifier, using: nil) { task in
            guard let task = task as? BGProcessingTask else {
                task.setTaskCompleted(success: false)
                return
            }
            run(task)
        }
    }

    /// Asks for the next window (on the way to the background).
    static func schedule() {
        guard ChatStoreHub.shared.isAvailable else { return }
        let request = BGProcessingTaskRequest(identifier: identifier)
        request.requiresNetworkConnectivity = true
        request.requiresExternalPower = true
        request.earliestBeginDate = Date(timeIntervalSinceNow: interval)
        do {
            try BGTaskScheduler.shared.submit(request)
        } catch {
            logger.info("catch-up window not scheduled: \(error.localizedDescription, privacy: .public)")
        }
    }

    private static func run(_ task: BGProcessingTask) {
        schedule()
        let hub = ChatStoreHub.shared
        let work = Task.detached(priority: .utility) {
            await hub.waitUntilReady(timeout: readyWait)
            await hub.runPasses()
        }
        task.expirationHandler = {
            hub.stopCatchUp()
            work.cancel()
        }
        nonisolated(unsafe) let task = task
        Task.detached {
            await work.value
            task.setTaskCompleted(success: !work.isCancelled)
        }
    }
}
