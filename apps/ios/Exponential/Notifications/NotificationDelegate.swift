import ExpCore
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
        // The payload's userId identifies which signed-in account the push
        // was for; the navigator opens it under that account instead of
        // whichever one is active.
        let userId = userInfo["userId"] as? String
        switch NotificationRouting.pushTarget(userInfo) {
        case let .session(sessionId):
            // A blocked run (EXP-980): its `sessionId` is the run to open.
            deepLinkBus.navigateToSession(sessionId, userId: userId)
        case .inbox:
            // An issue-less agent message (EXP-801), or a blocked run whose
            // row was pruned: the row lives in the inbox and nowhere else.
            deepLinkBus.navigateToInbox(userId: userId)
        case let .issue(issueId, face):
            // EXP-933: a targeted agent message opens the issue's Results.
            deepLinkBus.navigateToIssue(issueId, userId: userId, face: face)
        case .none:
            break
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
