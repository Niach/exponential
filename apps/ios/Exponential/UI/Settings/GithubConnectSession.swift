import AuthenticationServices
import ExpCore
import UIKit

/// SLOP-26: the ONE "Connect GitHub" hop on iOS, shared by the Add-repository
/// picker and Settings › Repositories' connection block.
///
/// Connect = the EXP-1126 link-ticket path: `users.mintSignInLinkTicket({
/// provider: "github"})`, then `/api/mobile-oauth-start?provider=github&
/// link=<ticket>` in the same ASWebAuthenticationSession the Sign-in methods
/// section runs (`SignInLinkSession`), returning as
/// `exponential://oauth-return?linked=github`. A server that does not offer
/// the GitHub link (the mint answers BAD_REQUEST) falls back to the guided
/// web page (`connectUrl`, `/integrations/github?return=app`), which hands
/// back through `exponential://github-connected`.
///
/// Installing the app is NOT a hop of this session: `installUrl` opens in the
/// system browser (the App's setup URL lands on the web page, whose "Return
/// to the app" fires the same `github-connected` deep link).
@MainActor
final class GithubConnectSession {
    enum Outcome {
        /// GitHub is linked (or the guided page handed back): re-list.
        case connected
        /// The hop failed; the sentence to show inline.
        case failed(String)
        /// The person closed the sheet — never an error.
        case cancelled
    }

    private let linkSession = SignInLinkSession()
    private let pageSession = InstallWebAuthSession()
    private(set) var isActive = false

    func start(
        deps: AppDependencies,
        accountId: String,
        connectUrl: String?,
        onFinished: @escaping @MainActor (Outcome) -> Void
    ) {
        guard !isActive else { return }
        isActive = true
        let finish: @MainActor (Outcome) -> Void = { [weak self] outcome in
            self?.isActive = false
            onFinished(outcome)
        }
        Task { @MainActor in
            guard let instanceUrl = deps.auth.accounts.first(where: { $0.id == accountId })?.instanceUrl else {
                finish(.failed(OAuthReturn.linkErrorMessage("")))
                return
            }
            let ticket: SignInLinkTicket
            do {
                ticket = try await deps.usersApi.mintSignInLinkTicket(accountId: accountId, provider: "github")
            } catch {
                if GithubConnect.linkNotOffered(code: error.trpcErrorCode),
                   let connectUrl, let url = URL(string: connectUrl) {
                    // The guided page: its done state deep-links back; a
                    // manual close re-lists too (the install may have landed).
                    pageSession.start(url: url) { errorSlug in
                        finish(errorSlug == nil ? .connected : .failed(GithubCopy.linkFailed))
                    }
                } else {
                    finish(.failed(error.trpcUserMessage))
                }
                return
            }
            let pkce = Pkce.generate()
            guard let url = AuthApi.linkStartUrl(
                instanceUrl: instanceUrl, ticket: ticket.ticket, provider: "github", codeChallenge: pkce.challenge
            ) else {
                finish(.failed(OAuthReturn.linkErrorMessage("")))
                return
            }
            linkSession.start(url: url, pkce: pkce) { result in
                guard let result else {
                    finish(.cancelled)
                    return
                }
                switch result {
                case .linked:
                    finish(.connected)
                case let .error(reason):
                    finish(.failed(OAuthReturn.linkErrorMessage(reason)))
                case .code, .none:
                    // A link hop never returns a code; re-list in case it landed.
                    finish(.connected)
                }
            }
        }
    }

    func cancel() {
        linkSession.cancel()
        pageSession.cancel()
        isActive = false
    }
}

/// Presents a web page in an ASWebAuthenticationSession so it (a) renders as
/// a phone-sized in-app page instead of desktop Safari and (b) auto-dismisses
/// when the server's page fires the `exponential://github-connected` deep
/// link (callback scheme `exponential`). The completion fires on callback AND
/// on manual dismissal — the install may have landed either way, so callers
/// re-query regardless. It receives the callback's `?error=` slug — nil on
/// success AND on manual dismissal, so nil means "no error to show", never
/// "connected".
@MainActor
final class InstallWebAuthSession: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?

    func start(url: URL, onFinished: @escaping @MainActor (String?) -> Void) {
        session?.cancel()
        let session = ASWebAuthenticationSession(
            url: url,
            callbackURLScheme: "exponential"
        ) { [weak self] callbackURL, _ in
            let errorSlug = callbackURL.flatMap { GithubConnect.errorSlug(from: $0) }
            Task { @MainActor in
                self?.session = nil
                onFinished(errorSlug)
            }
        }
        session.presentationContextProvider = self
        // The user is signing in to GitHub — share the persistent cookie jar so
        // an existing GitHub session is reused instead of forcing a fresh login.
        session.prefersEphemeralWebBrowserSession = false
        self.session = session
        session.start()
    }

    func cancel() {
        session?.cancel()
        session = nil
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap(\.windows)
                .first { $0.isKeyWindow } ?? ASPresentationAnchor()
        }
    }
}
