import ExpCore
import AuthenticationServices
import Foundation
import UIKit
import os

private let logger = Logger(subsystem: "at.exponential", category: "InstanceViewModel")

/// Backs the welcome / instance-picker screen. Cloud is the primary path
/// (EXP-14): this fetches the CLOUD instance's auth-config so the screen can
/// show "Continue with Google/Apple" directly — gated on which providers the
/// cloud actually enables, never hardcoded — and owns the OAuth session for
/// those buttons. Because the instance URL is only committed once the token
/// comes back, `InstanceView` stays mounted (and this view model alive, so the
/// web-auth session survives) for the whole cloud sign-in flow.
@MainActor @Observable
final class InstanceViewModel: NSObject, ASWebAuthenticationPresentationContextProviding {
    var cloudConfig: AuthConfig?
    var error: String?

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

    var googleAvailable: Bool { cloudConfig?.googleLoginEnabled == true }
    var appleAvailable: Bool { cloudConfig?.appleLoginEnabled == true }
    var hasDirectOAuth: Bool { googleAvailable || appleAvailable }

    // MARK: - Cloud auth config

    func loadCloudConfig() async {
        guard cloudConfig == nil else { return }
        cloudConfig = try? await authApi.fetchAuthConfig(instanceUrl: AppConstants.defaultCloudUrl)
    }

    // MARK: - Cloud OAuth (provider preselected via mobile-oauth-start)

    func startCloudGoogle() {
        let pkce = Pkce.generate()
        guard let url = authApi.googleStartUrl(instanceUrl: AppConstants.defaultCloudUrl, codeChallenge: pkce.challenge) else {
            error = "Could not build Google OAuth URL"
            return
        }
        pendingPkce = pkce
        launchWebAuth(url: url)
    }

    func startCloudApple() {
        let pkce = Pkce.generate()
        guard let url = authApi.appleStartUrl(instanceUrl: AppConstants.defaultCloudUrl, codeChallenge: pkce.challenge) else {
            error = "Could not build Apple OAuth URL"
            return
        }
        pendingPkce = pkce
        launchWebAuth(url: url)
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

    // MARK: - Private

    // Fetch the session (incl. onboardingCompletedAt) with the new token BEFORE
    // persisting it, mirroring LoginViewModel.applyLogin. The instance URL is
    // committed to the cloud here — right before the token lands — so the nav
    // gate transitions straight from the welcome screen to the app.
    private func applyCloudLogin(token: String) async {
        let cloud = AppConstants.defaultCloudUrl
        let session = await authApi.fetchSessionRetrying(instanceUrl: cloud, token: token)
        // A login must resolve a stable userId before its token is persisted —
        // per-user account identity is keyed on it. Fail (without committing the
        // instance URL or token) rather than strand an unattributable account.
        guard let userId = session?.id, !userId.isEmpty else {
            error = "Couldn't verify your account. Please try signing in again."
            return
        }
        auth.setInstanceUrl(cloud)
        auth.setToken(
            token,
            email: session?.email,
            name: session?.name,
            userId: userId,
            isAdmin: session?.isAdmin ?? false,
            onboardingCompletedAt: session?.onboardingCompletedAt,
            onboardingKnown: session != nil
        )
    }

    private func launchWebAuth(url: URL) {
        logger.info("Starting cloud OAuth: \(url.absoluteString)")
        error = nil

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

                logger.info("Cloud OAuth callback: \(callbackURL.absoluteString)")

                let params = LoginViewModel.callbackParams(callbackURL)

                // PKCE code (REV-13): redeem via /api/mobile-oauth-exchange
                // with the in-memory verifier — never a raw token on the wire.
                if let code = params["code"] {
                    guard let pkce = self.pendingPkce else {
                        self.error = "Couldn't verify your sign-in. Please try again."
                        self.webAuthSession = nil
                        return
                    }
                    self.pendingPkce = nil
                    guard let token = await self.authApi.exchangeOauthCode(
                        instanceUrl: AppConstants.defaultCloudUrl, code: code, codeVerifier: pkce.verifier
                    ) else {
                        self.error = "Couldn't verify your sign-in. Please try again."
                        self.webAuthSession = nil
                        return
                    }
                    self.error = nil
                    await self.applyCloudLogin(token: token)
                    self.webAuthSession = nil
                    return
                }

                // Legacy raw-token form (pre-PKCE servers, self-hosted lag).
                if let token = params["token"] {
                    self.error = nil
                    await self.applyCloudLogin(token: token)
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
        logger.info("Cloud OAuth session.start() returned: \(started)")
        if !started {
            error = "Failed to start authentication session"
            webAuthSession = nil
        }
    }
}
