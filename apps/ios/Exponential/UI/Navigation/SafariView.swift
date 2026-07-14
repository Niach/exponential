import SafariServices
import SwiftUI

/// In-app browser sheet for web links the app can't render (EXP-92 universal
/// links: unknown host / unsynced issue). SFSafariViewController never
/// re-triggers Universal Links, so this cannot loop back into the app —
/// unlike UIApplication.open, which would re-open us for our own entitled
/// domains.
struct SafariView: UIViewControllerRepresentable {
    let url: URL

    func makeUIViewController(context: Context) -> SFSafariViewController {
        SFSafariViewController(url: url)
    }

    func updateUIViewController(_ controller: SFSafariViewController, context: Context) {}
}
