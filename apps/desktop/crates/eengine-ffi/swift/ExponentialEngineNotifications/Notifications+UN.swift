// #VAPP-265: the UserNotifications seam (iOS and macOS). Request identifier
// = the Engine's id (`<app>/<id>`), so posting it again replaces it and
// cancel finds it; userInfo carries `NotificationTaps.userInfo`.
#if canImport(UserNotifications)
import Foundation
import UserNotifications
import ExponentialEngineCapabilityCore

public final class UserNotificationScheduler: NotificationScheduler, @unchecked Sendable {
    let center: UNUserNotificationCenter

    /// `center` defaults to `.current()` (needs an app bundle).
    public init(center: UNUserNotificationCenter = .current()) { self.center = center }

    public func authorization() throws -> NotificationAuthorization {
        let status: UNAuthorizationStatus = try Blocking.wait { finish in
            center.getNotificationSettings { finish(.success($0.authorizationStatus)) }
        }
        switch status {
        case .notDetermined: return .notDetermined
        case .denied: return .denied
        default: return .granted // authorized, provisional, ephemeral
        }
    }

    public func requestAuthorization() throws -> Bool {
        try Blocking.wait { finish in
            center.requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
                finish(error.map { .failure($0) } ?? .success(granted))
            }
        }
    }

    public func schedule(_ notification: PlannedNotification) throws {
        let content = UNMutableNotificationContent()
        content.title = notification.title
        content.body = notification.body ?? ""
        content.sound = .default
        content.userInfo = notification.userInfo
        let trigger: UNNotificationTrigger?
        switch notification.trigger {
        case .now: trigger = nil
        case let .after(seconds): trigger = UNTimeIntervalNotificationTrigger(timeInterval: max(seconds, 1), repeats: false)
        case let .on(components): trigger = UNCalendarNotificationTrigger(dateMatching: components, repeats: false)
        }
        let request = UNNotificationRequest(identifier: notification.id, content: content, trigger: trigger)
        let _: Void = try Blocking.wait { finish in
            center.add(request) { error in finish(error.map { .failure($0) } ?? .success(())) }
        }
    }

    public func pendingIds() throws -> [String] {
        try Blocking.wait { finish in
            center.getPendingNotificationRequests { finish(.success($0.map(\.identifier))) }
        }
    }

    public func deliveredIds() throws -> [String] {
        try Blocking.wait { finish in
            center.getDeliveredNotifications { finish(.success($0.map(\.request.identifier))) }
        }
    }

    public func remove(ids: [String]) throws {
        center.removePendingNotificationRequests(withIdentifiers: ids)
        center.removeDeliveredNotifications(withIdentifiers: ids)
    }
}

extension NotificationTaps {
    /// From the host's `userNotificationCenter(_:didReceive:withCompletionHandler:)`:
    /// the tap JSON to send as `app.event(eventJson:)`, nil when the person
    /// dismissed it or it is not the Engine's (or carries no tap).
    public static func tapJson(response: UNNotificationResponse) -> String? {
        guard response.actionIdentifier == UNNotificationDefaultActionIdentifier else { return nil }
        return tapJson(userInfo: response.notification.request.content.userInfo)
    }
}
#endif
