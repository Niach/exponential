import ExpPush
import Foundation
import UserNotifications

/// #EXP-1815 (notification-delivery.json CLEARING): the silent dismiss push
/// `{aps: {content-available: 1}, s: <sealed>}` opens with the push key (the
/// same open the Notification Service Extension uses) to `{type: dismiss,
/// ids | threads | all (+ teamId)}`; every delivered notification
/// `dismissMatches` picks is removed.
enum NotificationDismiss {
    /// The push's data: the sealed `s` opened (the current key first, then
    /// the one it replaced), else its plaintext keys.
    static func payload(_ userInfo: [AnyHashable: Any]) -> [String: String] {
        let opened = PushKeyStore().loadAll().lazy
            .compactMap { SealedPush.open(userInfo: userInfo, secret: $0.secret) }.first
        return NotificationDelivery.data(opened?.userInfo ?? userInfo)
    }

    /// Removes the matching delivered notifications; true = any.
    static func removeDelivered(matching payload: [String: String]) async -> Bool {
        let center = UNUserNotificationCenter.current()
        let ids = await center.deliveredNotifications()
            .filter {
                let content = $0.request.content
                var delivered = NotificationDelivery.Delivered(userInfo: content.userInfo)
                if delivered.thread == nil, !content.threadIdentifier.isEmpty {
                    delivered = .init(
                        notificationId: delivered.notificationId,
                        thread: content.threadIdentifier,
                        teamId: delivered.teamId
                    )
                }
                return NotificationDelivery.dismissMatches(payload, delivered)
            }
            .map(\.request.identifier)
        guard !ids.isEmpty else { return false }
        center.removeDeliveredNotifications(withIdentifiers: ids)
        return true
    }
}
