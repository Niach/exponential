import AppKit
import AuthenticationServices
import ExpCore
import Foundation
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential.mac", category: "MacLogin")

@MainActor
@Observable
final class MacLoginViewModel: NSObject, ASWebAuthenticationPresentationContextProviding {
    var email = ""
    var password = ""
    var customInstance = ""
    var loading = false
    var error: String?
    var configLoading = false
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

    // MARK: - ASWebAuthenticationPresentationContextProviding (macOS anchor)

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            NSApplication.shared.keyWindow ?? NSApplication.shared.windows.first ?? ASPresentationAnchor()
        }
    }

    // MARK: - Instance selection

    func chooseInstance(_ url: String) {
        let trimmed = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        auth.setInstanceUrl(trimmed)
        Task { await loadConfig() }
    }

    func goBackToInstance() {
        auth.clearInstanceUrl()
        config = nil
        configError = nil
        error = nil
    }

    func loadConfig() async {
        guard let instanceUrl = auth.instanceUrl else { return }
        configLoading = true
        configError = nil
        do {
            config = try await authApi.fetchAuthConfig(instanceUrl: instanceUrl)
        } catch {
            configError = error.localizedDescription
        }
        configLoading = false
    }

    // MARK: - Password

    func signIn() async {
        guard !loading, let instanceUrl = auth.instanceUrl else { return }
        loading = true
        error = nil
        let result = await authApi.signInWithPassword(instanceUrl: instanceUrl, email: email, password: password)
        switch result {
        case let .success(token, user):
            auth.setToken(token, email: user.email, name: user.name, userId: user.id, isAdmin: user.isAdmin ?? false)
        case let .failure(message):
            error = message
        }
        loading = false
    }

    // MARK: - OAuth / OIDC

    func startOAuth(providerId: String) {
        guard let instanceUrl = auth.instanceUrl,
              let url = authApi.oauthStartUrl(instanceUrl: instanceUrl, providerId: providerId) else {
            error = "Could not build sign-in URL"
            return
        }
        launchWebAuth(url: url)
    }

    func startGoogle() {
        guard let instanceUrl = auth.instanceUrl,
              let url = authApi.googleStartUrl(instanceUrl: instanceUrl) else {
            error = "Could not build sign-in URL"
            return
        }
        launchWebAuth(url: url)
    }

    private func handleOAuthToken(_ token: String) async {
        auth.setToken(token, email: nil)
        if let instanceUrl = auth.instanceUrl {
            let accountId = ServerAccount.makeId(for: instanceUrl)
            if let user = await authApi.fetchSession(accountId: accountId) {
                auth.setToken(token, email: user.email, name: user.name, userId: user.id, isAdmin: user.isAdmin ?? false)
            }
        }
    }

    private func launchWebAuth(url: URL) {
        logger.info("Starting OAuth: \(url.absoluteString, privacy: .public)")
        let session = ASWebAuthenticationSession(url: url, callbackURLScheme: "exp") { [weak self] callbackURL, authError in
            guard let self else { return }
            Task { @MainActor in
                if let authError {
                    let ns = authError as NSError
                    if ns.domain == ASWebAuthenticationSessionErrorDomain,
                       ns.code == ASWebAuthenticationSessionError.canceledLogin.rawValue {
                        self.error = nil
                    } else {
                        self.error = authError.localizedDescription
                    }
                    self.webAuthSession = nil
                    return
                }
                guard let callbackURL, let fragment = callbackURL.fragment else {
                    self.error = "No callback URL received"
                    self.webAuthSession = nil
                    return
                }
                let params = fragment.split(separator: "&").reduce(into: [String: String]()) { dict, pair in
                    let parts = pair.split(separator: "=", maxSplits: 1)
                    if parts.count == 2 { dict[String(parts[0])] = String(parts[1]) }
                }
                if let token = params["token"] {
                    self.error = nil
                    await self.handleOAuthToken(token)
                } else {
                    self.error = "No token in callback"
                }
                self.webAuthSession = nil
            }
        }
        session.presentationContextProvider = self
        session.prefersEphemeralWebBrowserSession = false
        webAuthSession = session
        if !session.start() {
            error = "Failed to start authentication session"
            webAuthSession = nil
        }
    }
}
