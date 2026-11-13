import ExpCore
import ExpPush
import ExpUI
import FirebaseMessaging
import Foundation
import UIKit
import UserNotifications

final class NotificationDelegate: NSObject, UNUserNotificationCenterDelegate, MessagingDelegate, Sendable {
    private let pushTokenManager: PushTokenManager
    private let deepLinkBus: DeepLinkBus
    /// #EXP-1641: the push's own actions (approve, answer, reply…).
    private let actions: NotificationActionRunner

    init(pushTokenManager: PushTokenManager, deepLinkBus: DeepLinkBus, actions: NotificationActionRunner) {
        self.pushTokenManager = pushTokenManager
        self.deepLinkBus = deepLinkBus
        self.actions = actions
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
        // #EXP-1641: an action button / reply runs in place, the app not
        // opened; `open` (foreground) and a tap route below.
        if NotificationActionRunner.handles(response.actionIdentifier) {
            let tap = NotificationActionRunner.tap(response)
            let actions = actions
            nonisolated(unsafe) let done = completionHandler
            Task {
                await actions.run(tap)
                done()
            }
            return
        }
        open(response.notification.request.content.userInfo)
        completionHandler()
    }

    // Notification received while app in foreground
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        // #EXP-1815 (notification-delivery.json PRESENTATION): in the
        // foreground nothing pops from the system; the app shows THE toast
        // (kind info, the row's title + body, `Open` = the tap's route)
        // unless the screen on top is the push's thread or the Inbox.
        completionHandler([])
        let content = notification.request.content
        let title = content.title
        let body = content.body
        let thread = (content.userInfo[NotificationDelivery.DataKeys.thread] as? String)
            .flatMap { $0.isEmpty ? nil : $0 } ?? content.threadIdentifier
        // The push's string data (all a route reads), Sendable.
        let data = NotificationDelivery.data(content.userInfo)
        Task { @MainActor in
            let presentation = NotificationDelivery.presentation(
                platform: .ios, active: true, viewing: ViewingThread.shared.current, thread: thread, muted: false
            )
            guard presentation == .toast, !title.isEmpty else { return }
            Toaster.shared.info(
                title,
                description: body.isEmpty ? nil : body,
                action: ToastAction(label: NotificationDelivery.Copy.toastAction) { [self] in
                    open(data as [AnyHashable: Any])
                }
            )
        }
    }

    /// A tap's route (the notification or the toast's `Open`).
    nonisolated func open(_ userInfo: [AnyHashable: Any]) {
        // The payload's userId identifies which signed-in account the push
        // was for; the navigator opens it under that account instead of
        // whichever one is active.
        let userId = userInfo["userId"] as? String
        switch NotificationRouting.pushTarget(userInfo) {
        case let .session(sessionId, card):
            // A push naming a session (EXP-980 wall, #EXP-1456 an agent's
            // decision, a mention): the navigator opens it by inbox-route.json;
            // a question / plan push lands on its card (waiting-cards.json
            // DEEP LINKS).
            deepLinkBus.navigateToSession(sessionId, userId: userId, card: card)
        case .inbox:
            // An issue-less agent message without a session (EXP-801), or a
            // blocked session whose row was pruned: the row lives in the inbox alone.
            let teamId = (userInfo["teamId"] as? String).flatMap { $0.isEmpty ? nil : $0 }
            deepLinkBus.navigateToInbox(userId: userId, teamId: teamId)
        case let .issue(issueId):
            deepLinkBus.navigateToIssue(issueId, userId: userId)
        case let .device(deviceRowId):
            // #EXP-1534: a signed-out login opens that device's settings.
            deepLinkBus.navigateToDevice(deviceRowId, userId: userId)
        case .none:
            break
        }
    }
}
