import SwiftUI
import UIKit

/// The system share sheet, presentable from an ordinary `.sheet` (EXP-603).
///
/// `ShareLink` is a *row* — it only renders as a menu/toolbar item and drives
/// its own presentation, so it cannot live inside `GlassMenu`, whose rows are
/// plain buttons. Dropping to `UIActivityViewController` puts the share flow
/// back under the call site's control: the menu item sets a URL, the host
/// presents this.
struct ActivityShareSheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

/// `.sheet(item:)` needs an `Identifiable`; a bare `URL` is not one.
struct ShareTarget: Identifiable {
    let url: URL
    let text: String

    var id: String { url.absoluteString }
}
