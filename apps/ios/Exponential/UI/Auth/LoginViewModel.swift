import ExpUI
import ExpCore
import AuthenticationServices
import Foundation
import UIKit
import os

private let logger = Logger(subsystem: "at.exponential", category: "LoginViewModel")

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
        guard !loading, let instanceUrl = auth.instanceUrl else { return }
        loading = true
        error = nil
        let result = await authApi.signInWithPassword(instanceUrl: instanceUrl, email: email, password: password)
        switch result {
        case let .success(token, user):
            await applyLogin(token: token, user: user)
            loading = false
        case let .failure(message):
            error = message
            loading = false
        }
    }

    // Fetch the session (incl. onboardingCompletedAt) with the new token BEFORE
    // persisting it, so the onboarding flag lands together with the token and
    // the nav gate never sees a returning user as "not onboarded". Falls back
    // to the sign-in fields if the session fetch fails.
    private func applyLogin(token: String, user: AuthUser?) async {
        guard let instanceUrl = auth.instanceUrl else { return }
        let session = await authApi.fetchSession(instanceUrl: instanceUrl, token: token)
        auth.setToken(
            token,
            email: session?.email ?? user?.email,
            name: session?.name ?? user?.name,
            userId: session?.id ?? user?.id,
            isAdmin: session?.isAdmin ?? user?.isAdmin ?? false,
            onboardingCompletedAt: session?.onboardingCompletedAt,
            onboardingKnown: session != nil
        )
    }

    // MARK: - OAuth

    func startOAuthFlow(providerId: String) {
        guard let instanceUrl = auth.instanceUrl,
              let url = authApi.oauthStartUrl(instanceUrl: instanceUrl, providerId: providerId) else {
            error = "Could not build OAuth URL"
            return
        }
        launchWebAuth(url: url)
    }

    func startGoogleOAuthFlow() {
        guard let instanceUrl = auth.instanceUrl,
              let url = authApi.googleStartUrl(instanceUrl: instanceUrl) else {
            error = "Could not build Google OAuth URL"
            return
        }
        launchWebAuth(url: url)
    }

    func startAppleOAuthFlow() {
        guard let instanceUrl = auth.instanceUrl,
              let url = authApi.appleStartUrl(instanceUrl: instanceUrl) else {
            error = "Could not build Apple OAuth URL"
            return
        }
        launchWebAuth(url: url)
    }

    func handleOAuthToken(_ token: String) async {
        await applyLogin(token: token, user: nil)
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
