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
    // The in-flight OAuth attempt's PKCE pair (REV-13). In-memory only —
    // never persisted; last-start-wins (a new attempt replaces the old).
    private var pendingPkce: Pkce?

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
        let session = await authApi.fetchSessionRetrying(instanceUrl: instanceUrl, token: token)
        // A login must resolve a stable userId before its token is persisted —
        // per-user account identity (and the DB file) is keyed on it. If neither
        // the session read nor the sign-in body yields one, fail the login
        // rather than persist an unattributable token (the old userId==nil
        // read-only bug class).
        guard let userId = session?.id ?? user?.id, !userId.isEmpty else {
            error = "Couldn't verify your account. Please try signing in again."
            return
        }
        auth.setToken(
            token,
            email: session?.email ?? user?.email,
            name: session?.name ?? user?.name,
            userId: userId,
            isAdmin: session?.isAdmin ?? user?.isAdmin ?? false,
            onboardingCompletedAt: session?.onboardingCompletedAt,
            onboardingKnown: session != nil
        )
    }

    // MARK: - OAuth

    func startOAuthFlow(providerId: String) {
        let pkce = Pkce.generate()
        guard let instanceUrl = auth.instanceUrl,
              let url = authApi.oauthStartUrl(instanceUrl: instanceUrl, providerId: providerId, codeChallenge: pkce.challenge) else {
            error = "Could not build OAuth URL"
            return
        }
        pendingPkce = pkce
        launchWebAuth(url: url)
    }

    func startGoogleOAuthFlow() {
        let pkce = Pkce.generate()
        guard let instanceUrl = auth.instanceUrl,
              let url = authApi.googleStartUrl(instanceUrl: instanceUrl, codeChallenge: pkce.challenge) else {
            error = "Could not build Google OAuth URL"
            return
        }
        pendingPkce = pkce
        launchWebAuth(url: url)
    }

    func startAppleOAuthFlow() {
        let pkce = Pkce.generate()
        guard let instanceUrl = auth.instanceUrl,
              let url = authApi.appleStartUrl(instanceUrl: instanceUrl, codeChallenge: pkce.challenge) else {
            error = "Could not build Apple OAuth URL"
            return
        }
        pendingPkce = pkce
        launchWebAuth(url: url)
    }

    func handleOAuthToken(_ token: String) async {
        await applyLogin(token: token, user: nil)
    }

    func goBack() {
        auth.clearInstanceUrl()
    }

    // MARK: - Private

    /// Callback params from the fragment (primary — ASWebAuthenticationSession
    /// keeps the whole URL) merged with the query (EXP-21 fallback form; the
    /// server doubles the payload into both). Fragment wins on key collision.
    static func callbackParams(_ url: URL) -> [String: String] {
        var params = [String: String]()
        if let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems {
            for item in items where item.value?.isEmpty == false {
                params[item.name] = item.value
            }
        }
        if let fragment = url.fragment {
            for pair in fragment.split(separator: "&") {
                let parts = pair.split(separator: "=", maxSplits: 1)
                if parts.count == 2 {
                    params[String(parts[0])] = String(parts[1])
                }
            }
        }
        return params
    }

    private func launchWebAuth(url: URL) {
        logger.info("Starting OAuth: \(url.absoluteString)")

        let session = ASWebAuthenticationSession(
            url: url,
            callbackURLScheme: "exponential"
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

                let params = Self.callbackParams(callbackURL)

                // PKCE code (REV-13): redeem via /api/mobile-oauth-exchange
                // with the in-memory verifier — never a raw token on the wire.
                if let code = params["code"] {
                    guard let pkce = self.pendingPkce else {
                        self.error = "Couldn't verify your sign-in. Please try again."
                        self.webAuthSession = nil
                        return
                    }
                    self.pendingPkce = nil
                    guard let instanceUrl = self.auth.instanceUrl,
                          let token = await self.authApi.exchangeOauthCode(
                              instanceUrl: instanceUrl, code: code, codeVerifier: pkce.verifier
                          ) else {
                        self.error = "Couldn't verify your sign-in. Please try again."
                        self.webAuthSession = nil
                        return
                    }
                    self.error = nil
                    await self.handleOAuthToken(token)
                    self.webAuthSession = nil
                    return
                }

                // Legacy raw-token form (pre-PKCE servers, self-hosted lag).
                if let token = params["token"] {
                    self.error = nil
                    await self.handleOAuthToken(token)
                    self.webAuthSession = nil
                    return
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
