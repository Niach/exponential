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
            let loaded = try await authApi.fetchAuthConfig(instanceUrl: instanceUrl)
            // The user may have gone back and chosen a different instance while
            // this request was in flight — drop a result for a stale instance.
            guard auth.instanceUrl == instanceUrl else { return }
            config = loaded
        } catch {
            guard auth.instanceUrl == instanceUrl else { return }
            configError = error.localizedDescription
        }
        guard auth.instanceUrl == instanceUrl else { return }
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
        // ASWebAuthenticationSession invokes its completion handler on a background
        // XPC queue. The closure must be non-isolated (@Sendable) — otherwise Swift
        // infers it @MainActor (it's written in a @MainActor class) and inserts an
        // executor-isolation check at the closure's entry that traps when it runs
        // off the main queue. Hop to the main actor explicitly for all UI state.
        let session = ASWebAuthenticationSession(url: url, callbackURLScheme: "exp") { @Sendable callbackURL, authError in
            Task { @MainActor [weak self] in
                guard let self else { return }
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
