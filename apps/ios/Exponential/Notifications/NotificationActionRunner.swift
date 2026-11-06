import ExpCore
import ExpPush
import Foundation
import GRDB
import OSLog
import UserNotifications

private let logger = Logger(subsystem: "at.exponential", category: "NotificationActions")

/// #EXP-1641 (notification-actions.json WHAT THEY DO): a notification
/// action, run in the app's process without opening it. Every mutation goes
/// through THE app's answer / send code (`SessionsApi.answer` / `send`),
/// which re-seals a sealed card and seals into an encrypted chat with the
/// tree key exactly like the session page. A `sealed=1` push never leaves in
/// clear (`NotificationActions.route`): a reply is sealed for the push's
/// `rootId` without the session row, a card not synced here is read first
/// (`MessageSnapshot`) so the answer path can re-seal it. Success removes the
/// notification; a failure replaces it with `copy.failed`, opening the
/// session. The push's `userId` picks the account.
final class NotificationActionRunner: Sendable {
    private let auth: AuthRepository
    private let db: DatabaseManager
    private let sessionsApi: SessionsApi
    private let snapshot: MessageSnapshot

    init(auth: AuthRepository, db: DatabaseManager, sessionsApi: SessionsApi) {
        self.auth = auth
        self.db = db
        self.sessionsApi = sessionsApi
        self.snapshot = MessageSnapshot(auth: auth, db: db)
    }

    /// One tapped action, as the delegate hands it over.
    struct Tap: Sendable {
        let actionId: String
        let text: String?
        let data: [String: String]
        let optionLabels: [String: String]
        let requestId: String
    }

    /// The actions this runner takes (`open` is a tap: foreground).
    static func handles(_ actionId: String) -> Bool {
        actionId == NotificationActions.approve || actionId == NotificationActions.requestChanges
            || actionId == NotificationActions.answer || actionId == NotificationActions.reply
            || actionId.hasPrefix(NotificationActions.optionPrefix)
    }

    static func tap(_ response: UNNotificationResponse) -> Tap {
        let content = response.notification.request.content
        let text = (response as? UNTextInputNotificationResponse)?.userText
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return Tap(
            actionId: response.actionIdentifier,
            text: text,
            data: NotificationActions.data(content.userInfo),
            optionLabels: content.userInfo[NotificationActions.optionLabelsKey] as? [String: String] ?? [:],
            requestId: response.notification.request.identifier
        )
    }

    func run(_ tap: Tap) async {
        let ok: Bool
        do {
            ok = try await perform(tap)
        } catch {
            // Offline, CONFLICT (answered elsewhere), a card it cannot re-seal.
            logger.error("Notification action failed: \(String(describing: error), privacy: .public)")
            ok = false
        }
        let center = UNUserNotificationCenter.current()
        if ok {
            center.removeDeliveredNotifications(withIdentifiers: [tap.requestId])
        } else {
            await showFailed(tap, center: center)
        }
    }

    /// The signed-in account the push's `userId` names.
    func accountId(userId: String?) -> String? {
        guard let userId, !userId.isEmpty else { return nil }
        return auth.accounts.first { $0.userId == userId && $0.token != nil }?.id
    }

    /// True = the server took it; false = refused here (nothing was sent).
    private func perform(_ tap: Tap) async throws -> Bool {
        guard let accountId = accountId(userId: tap.data["userId"]),
              let sessionId = tap.data["sessionId"], !sessionId.isEmpty else { return false }
        let sealed = tap.data[NotificationActions.DataKeys.sealed] == "1"
        let messageId = tap.data[NotificationActions.DataKeys.messageId].flatMap { $0.isEmpty ? nil : $0 }
        let cardLocal = messageId.map { cardIsLocal(accountId: accountId, messageId: $0) } ?? false
        let route = NotificationActions.route(
            actionId: tap.actionId, sealed: sealed, rootId: tap.data[QuestionPush.rootIdKey], cardLocal: cardLocal
        )
        switch route {
        case .refuse:
            return false
        case .send, .sendSealed:
            guard let text = tap.text, !text.isEmpty else { return false }
            let input = SessionsSendInput.message(sessionId: sessionId, text: text)
            if case let .sendSealed(rootId) = route {
                try await sessionsApi.sendSealed(accountId: accountId, input, rootId: rootId)
            } else {
                try await sessionsApi.send(accountId: accountId, input)
            }
            return true
        case .fetchThenAnswer:
            // The card off the shape on demand; still none = nothing to re-seal.
            guard let messageId, try await snapshot.fetch(accountId: accountId, messageId: messageId)?.card != nil else {
                return false
            }
        case .answer:
            break
        }
        guard let messageId else { return false }
        switch tap.actionId {
        case NotificationActions.approve:
            try await sessionsApi.answer(accountId: accountId, messageId: messageId, decision: .approved, sealedOnly: sealed)
        case NotificationActions.requestChanges:
            guard let text = tap.text, !text.isEmpty else { return false }
            try await sessionsApi.answer(
                accountId: accountId, messageId: messageId, decision: .rejected, feedback: text, sealedOnly: sealed
            )
        case NotificationActions.answer:
            guard let text = tap.text, !text.isEmpty else { return false }
            try await sessionsApi.answer(accountId: accountId, messageId: messageId, answer: text, sealedOnly: sealed)
        default:
            guard tap.actionId.hasPrefix(NotificationActions.optionPrefix),
                  let label = tap.optionLabels[tap.actionId] ?? optionLabel(tap) else { return false }
            try await sessionsApi.answer(
                accountId: accountId, messageId: messageId, answer: label, answers: .question([label]), sealedOnly: sealed
            )
        }
        return true
    }

    /// A plaintext card's option label off the push's own `options`.
    private func optionLabel(_ tap: Tap) -> String? {
        guard let index = Int(tap.actionId.dropFirst(NotificationActions.optionPrefix.count)),
              let raw = tap.data[NotificationActions.DataKeys.options],
              let options = try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String],
              options.indices.contains(index) else { return nil }
        return options[index]
    }

    private func cardIsLocal(accountId: String, messageId: String) -> Bool {
        guard let pool = try? db.pool(forAccountId: accountId) else { return false }
        return ((try? pool.read { try MessageEntity.fetchOne($0, key: messageId) })??.card) != nil
    }

    /// Replaces the push with `copy.failed`; a tap opens the session.
    private func showFailed(_ tap: Tap, center: UNUserNotificationCenter) async {
        let content = UNMutableNotificationContent()
        content.title = NotificationActions.Copy.failed
        var info: [String: String] = [:]
        for key in ["sessionId", "userId", "teamId", "type"] { info[key] = tap.data[key] }
        content.userInfo = info
        content.sound = .default
        do {
            try await center.add(UNNotificationRequest(identifier: tap.requestId, content: content, trigger: nil))
        } catch {
            logger.error("Failed notification not shown: \(error.localizedDescription, privacy: .public)")
        }
    }
}
