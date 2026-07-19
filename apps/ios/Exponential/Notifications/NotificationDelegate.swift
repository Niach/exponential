import FirebaseMessaging
import Foundation
import UIKit
import UserNotifications

final class NotificationDelegate: NSObject, UNUserNotificationCenterDelegate, MessagingDelegate, Sendable {
    private let pushTokenManager: PushTokenManager
    private let deepLinkBus: DeepLinkBus

    init(pushTokenManager: PushTokenManager, deepLinkBus: DeepLinkBus) {
        self.pushTokenManager = pushTokenManager
        self.deepLinkBus = deepLinkBus
        super.init()
    }

    func setup() {
        UNUserNotificationCenter.current().delegate = self
        Messaging.messaging().delegate = self
    }

    func requestPermission() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound]) { granted, _ in
            if granted {
                DispatchQueue.main.async {
                    UIApplication.shared.registerForRemoteNotifications()
                }
            }
        }
    }

    // MARK: - MessagingDelegate

    nonisolated func messaging(_ messaging: Messaging, didReceiveRegistrationToken fcmToken: String?) {
        guard let fcmToken else { return }
        pushTokenManager.register(fcmToken: fcmToken)
    }

    // MARK: - UNUserNotificationCenterDelegate

    // Notification tapped
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let userInfo = response.notification.request.content.userInfo
        if userInfo["type"] as? String == "support_reply",
           let threadId = userInfo["threadId"] as? String {
            // Helpdesk pushes (EXP-180) carry a threadId and NO issue keys —
            // route to the Support thread view instead of an issue.
            deepLinkBus.navigateToSupportThread(threadId, userId: userInfo["userId"] as? String)
        } else if let issueId = userInfo["issueId"] as? String {
            // The payload's userId identifies which signed-in account the
            // push was for; the navigator opens the issue under that account
            // instead of whichever one is active.
            deepLinkBus.navigateToIssue(issueId, userId: userInfo["userId"] as? String)
        }
        completionHandler()
    }

    // Notification received while app in foreground
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .badge, .sound])
    }
}
