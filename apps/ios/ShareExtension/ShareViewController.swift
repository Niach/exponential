import Intents
import SwiftUI
import UIKit

/// `NSExtensionPrincipalClass` for the share extension. Builds the lightweight
/// dependency graph, then hosts the SwiftUI compose surface. The extension
/// process is torn down once `completeRequest`/`cancelRequest` returns, so all
/// uploads finish before we complete (see `ShareSubmitter`).
final class ShareViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()

        let deps = ShareDependencies()
        let items = (extensionContext?.inputItems as? [NSExtensionItem]) ?? []

        let root = ShareRootView(
            deps: deps,
            extensionItems: items,
            directShareId: (extensionContext?.intent as? INSendMessageIntent)?.conversationIdentifier,
            onComplete: { [weak self] in
                self?.extensionContext?.completeRequest(returningItems: [], completionHandler: nil)
            },
            onCancel: { [weak self] in
                self?.extensionContext?.cancelRequest(
                    withError: NSError(domain: "at.exponential.shareextension", code: 0)
                )
            }
        )

        // #EXP-2031: the extension's own chrome (navigation titles, the
        // Cancel pill) is dark like the app's window; `preferredColorScheme`
        // alone leaves the bar on the host app's light scheme.
        overrideUserInterfaceStyle = .dark
        let host = UIHostingController(rootView: root)
        host.overrideUserInterfaceStyle = .dark
        addChild(host)
        host.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(host.view)
        NSLayoutConstraint.activate([
            host.view.topAnchor.constraint(equalTo: view.topAnchor),
            host.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            host.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            host.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
        ])
        host.didMove(toParent: self)
    }
}
