import AuthenticationServices
import ExpCore
import UIKit

/// EXP-1126: the LINK-mode browser handoff. A signed-in account attaches a
/// Google/Apple/OIDC login through `/api/mobile-oauth-start?link=<ticket>`,
/// presented in an ASWebAuthenticationSession exactly like the login hop
/// (callback scheme `exponential`, the persistent cookie jar so an existing
/// provider session is reused). Apple goes through here too — native Sign in
/// with Apple only ever signs IN.
///
/// The completion hands back the parsed `exponential://oauth-return`
/// (`.linked(providerId)` or `.error(reason)`), or nil when the user closed
/// the sheet — a cancel is never an error.
@MainActor
final class SignInLinkSession: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?
    /// Held for the attempt's lifetime like the login hop's verifier. Link mode
    /// returns no code to redeem, but the route still requires the challenge.
    private var pendingPkce: Pkce?

    var isActive: Bool { session != nil }

    func start(url: URL, pkce: Pkce, onFinished: @escaping @MainActor (OAuthReturn?) -> Void) {
        session?.cancel()
        pendingPkce = pkce
        let session = ASWebAuthenticationSession(
            url: url,
            callbackURLScheme: "exponential"
        ) { [weak self] callbackURL, error in
            let result: OAuthReturn? = {
                if let callbackURL { return OAuthReturn.parse(callbackURL) }
                if let nsError = error as NSError?,
                   nsError.domain == ASWebAuthenticationSessionErrorDomain,
                   nsError.code == ASWebAuthenticationSessionError.canceledLogin.rawValue {
                    return nil
                }
                return error == nil ? nil : .error("unable_to_link_account")
            }()
            Task { @MainActor in
                self?.session = nil
                self?.pendingPkce = nil
                onFinished(result)
            }
        }
        session.presentationContextProvider = self
        session.prefersEphemeralWebBrowserSession = false
        self.session = session
        if !session.start() {
            self.session = nil
            pendingPkce = nil
            onFinished(.error("unable_to_link_account"))
        }
    }

    func cancel() {
        session?.cancel()
        session = nil
        pendingPkce = nil
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
