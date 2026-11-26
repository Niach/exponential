import AuthenticationServices
import ExpCore
import UIKit

/// #NEW-146 (elicitation-card.json OPEN LINK): the url card's Open link, on
/// the press only (no prefetch): an OAuth URL in ASWebAuthenticationSession
/// (the persistent cookie jar, no callback: the person closes it and presses
/// I'm done), any other http(s) URL in THE Safari sheet
/// (SFSafariViewController through `DeepLinkBus.openExternal`). Never an
/// in-app WebView; it answers nothing.
@MainActor
final class ElicitationLinkOpener: NSObject, ASWebAuthenticationPresentationContextProviding {
    static let shared = ElicitationLinkOpener()

    private var session: ASWebAuthenticationSession?

    func open(_ raw: String?, deps: AppDependencies) {
        switch ElicitationCard.openTarget(raw) {
        case let .safari(url):
            deps.deepLinkBus.openExternal(url)
        case let .oauth(url):
            session?.cancel()
            let session = ASWebAuthenticationSession(url: url, callbackURLScheme: nil) { [weak self] _, _ in
                Task { @MainActor in self?.session = nil }
            }
            session.presentationContextProvider = self
            session.prefersEphemeralWebBrowserSession = false
            self.session = session
            if !session.start() {
                self.session = nil
                deps.deepLinkBus.openExternal(url)
            }
        case nil:
            break
        }
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
