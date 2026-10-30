import ExpPush
import Foundation
import UserNotifications

// EXP-1435: the Notification Service Extension. A push through the relay's
// accountless mode arrives as a placeholder alert ("New message") plus `s`,
// the payload sealed to this device's X25519 key; the extension opens it and
// shows the real title and body, merging `data` into `userInfo` exactly like
// a plaintext FCM push so a tap routes identically (NotificationDelegate →
// NotificationRouting). Anything it cannot open (no `s`, no key, no binding,
// a tampered or foreign payload) is delivered unchanged. A question push's
// sealed text opens with the tree key in the shared keychain (#VAPP-45). No Firebase, no GRDB:
// the extension's memory cap is ~24 MB.
final class NotificationService: UNNotificationServiceExtension {
    private var contentHandler: ((UNNotificationContent) -> Void)?
    private var bestAttempt: UNMutableNotificationContent?

    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        self.contentHandler = contentHandler
        guard let content = request.content.mutableCopy() as? UNMutableNotificationContent else {
            contentHandler(request.content)
            return
        }
        bestAttempt = content
        // The current key first, then the one it replaced (EXP-1457: the key
        // moved to the identity-derived one; pushes sealed before the server
        // learned it still open).
        if let opened = PushKeyStore().loadAll().lazy
            .compactMap({ SealedPush.open(userInfo: content.userInfo, secret: $0.secret) }).first {
            // A degraded over-budget payload may carry an empty title: keep
            // the placeholder rather than show a blank one.
            if !opened.title.isEmpty { content.title = opened.title }
            // #VAPP-45: a question push carries its text sealed to the
            // session tree; opened with the tree key the app holds, else the
            // push's own (generic) body. The title stays.
            content.body = QuestionPush.body(
                data: opened.userInfo,
                keysFor: { TreeKeyStore().keys(rootId: $0) },
                fallback: opened.body ?? ""
            )
            content.userInfo = opened.userInfo
            // Plaintext pushes ring with the default sound; the relay's
            // placeholder carries none.
            content.sound = .default
        }
        deliver()
    }

    override func serviceExtensionTimeWillExpire() {
        deliver()
    }

    private func deliver() {
        guard let handler = contentHandler else { return }
        contentHandler = nil
        handler(bestAttempt ?? UNNotificationContent())
    }
}
