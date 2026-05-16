import AuthenticationServices
import Foundation
import UIKit
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "LoginViewModel")

@MainActor @Observable
final class LoginViewModel: NSObject, ASWebAuthenticationPresentationContextProviding {
    var email = ""
    var password = ""
    var loading = false
    var error: String?
    var configLoading = true
    var config: AuthConfig?
    var configError: String?

    private let authApi: AuthApi
    private let auth: AuthRepository
    private var webAuthSession: ASWebAuthenticationSession?

    init(authApi: AuthApi, auth: AuthRepository) {
        self.authApi = authApi
        self.auth = auth
        super.init()
    }

    // MARK: - ASWebAuthenticationPresentationContextProviding

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap(\.windows)
                .first { $0.isKeyWindow } ?? ASPresentationAnchor()
        }
    }

    // MARK: - Auth Config

    func loadConfig() async {
        guard let instanceUrl = auth.instanceUrl else {
            configLoading = false
            configError = "No instance URL"
            return
        }
        configLoading = true
        configError = nil
        do {
            config = try await authApi.fetchAuthConfig(instanceUrl: instanceUrl)
            configLoading = false
        } catch {
            configLoading = false
            configError = error.localizedDescription
        }
    }

    // MARK: - Password Sign In

    func signIn() async {
        guard !loading else { return }
        loading = true
        error = nil
        let result = await authApi.signInWithPassword(email: email, password: password)
        switch result {
        case let .success(token, user):
            auth.setToken(token, email: user.email, name: user.name, userId: user.id, isAdmin: user.isAdmin ?? false)
            loading = false
        case let .failure(message):
            error = message
            loading = false
        }
    }

    // MARK: - OAuth

    func startOAuthFlow(providerId: String) {
        guard let url = authApi.oauthStartUrl(providerId: providerId) else {
            error = "Could not build OAuth URL"
            return
        }
        launchWebAuth(url: url)
    }

    func startGoogleOAuthFlow() {
        guard let url = authApi.googleStartUrl() else {
            error = "Could not build Google OAuth URL"
            return
        }
        launchWebAuth(url: url)
    }

    func handleOAuthToken(_ token: String) async {
        auth.setToken(token, email: nil)
        if let user = await authApi.fetchSession() {
            auth.setToken(token, email: user.email, name: user.name, userId: user.id, isAdmin: user.isAdmin ?? false)
        }
    }

    func goBack() {
        auth.clearInstanceUrl()
    }

    // MARK: - Private

    private func launchWebAuth(url: URL) {
        logger.info("Starting OAuth: \(url.absoluteString)")

        let session = ASWebAuthenticationSession(
            url: url,
            callbackURLScheme: "exp"
        ) { [weak self] callbackURL, authError in
            guard let self else { return }

            Task { @MainActor in
                if let authError {
                    let nsError = authError as NSError
                    if nsError.domain == ASWebAuthenticationSessionErrorDomain,
                       nsError.code == ASWebAuthenticationSessionError.canceledLogin.rawValue {
                        self.error = nil
                    } else {
                        self.error = authError.localizedDescription
                    }
                    self.webAuthSession = nil
                    return
                }

                guard let callbackURL else {
                    self.error = "No callback URL received"
                    self.webAuthSession = nil
                    return
                }

                logger.info("OAuth callback: \(callbackURL.absoluteString)")

                if let fragment = callbackURL.fragment {
                    let params = fragment.split(separator: "&").reduce(into: [String: String]()) { dict, pair in
                        let parts = pair.split(separator: "=", maxSplits: 1)
                        if parts.count == 2 {
                            dict[String(parts[0])] = String(parts[1])
                        }
                    }
                    if let token = params["token"] {
                        self.error = nil
                        await self.handleOAuthToken(token)
                        self.webAuthSession = nil
                        return
                    }
                }

                self.error = "No token in callback: \(callbackURL.absoluteString)"
                self.webAuthSession = nil
            }
        }
        session.presentationContextProvider = self
        session.prefersEphemeralWebBrowserSession = false
        webAuthSession = session

        let started = session.start()
        logger.info("OAuth session.start() returned: \(started)")
        if !started {
            error = "Failed to start authentication session"
            webAuthSession = nil
        }
    }
}
