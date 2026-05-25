import Foundation
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "PushTokenManager")

final class PushTokenManager: @unchecked Sendable {
    private let pushTokensApi: PushTokensApi
    private let auth: AuthRepository
    private var currentToken: String?

    init(pushTokensApi: PushTokensApi, auth: AuthRepository) {
        self.pushTokensApi = pushTokensApi
        self.auth = auth
    }

    func register(fcmToken: String) {
        currentToken = fcmToken
        guard auth.isAuthenticated else { return }
        Task {
            do {
                try await pushTokensApi.register(accountId: auth.activeAccountId ?? "", token: fcmToken)
                logger.info("FCM token registered")
            } catch {
                logger.error("Failed to register FCM token: \(error.localizedDescription)")
            }
        }
    }

    func unregister() {
        guard let token = currentToken else { return }
        Task {
            do {
                try await pushTokensApi.unregister(accountId: auth.activeAccountId ?? "", token: token)
                logger.info("FCM token unregistered")
            } catch {
                logger.error("Failed to unregister FCM token: \(error.localizedDescription)")
            }
        }
        currentToken = nil
    }
}
