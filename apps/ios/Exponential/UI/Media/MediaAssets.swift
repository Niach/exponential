import AVFoundation
import AVKit
import ExpCore
import ExpUI
import Foundation
import UIKit

/// EXP-824 — AVFoundation glue for inline media: the authenticated asset and
/// the native fullscreen presenter. Everything decidable without AVFoundation
/// (which URL, which headers) lives in ExpCore's `AttachmentMediaAuth` and is
/// unit-tested there; this file only hands the result to the framework.
enum MediaAssets {
    /// An `AVURLAsset` for a stored attachment URL. The byte route is
    /// member-only and iOS is bearer-only, so the account's token rides the
    /// asset's HTTP headers — but only on the instance's own origin
    /// (`AttachmentMediaAuth`); a foreign host streams anonymously. HTTP Range
    /// works on the server, so AVPlayer scrubs without a full download.
    @MainActor
    static func makeAsset(urlString: String, baseURL: URL?, auth: AuthRepository, accountId: String) -> AVURLAsset? {
        let token = auth.accounts.first(where: { $0.id == accountId })?.token
        guard let request = AttachmentMediaAuth.request(
            for: urlString, instanceBaseURL: baseURL, token: token
        ) else { return nil }
        var options: [String: Any] = [:]
        if !request.headers.isEmpty {
            options["AVURLAssetHTTPHeaderFieldsKey"] = request.headers
        }
        return AVURLAsset(url: request.url, options: options)
    }

    /// The photo-style fullscreen treatment: the SYSTEM player controller,
    /// presented modally over the topmost view controller so it gets the
    /// native Done button, rotation and picture-in-picture for free. The
    /// same `AVPlayer` keeps playing, so leaving fullscreen resumes inline.
    @MainActor
    static func presentFullscreen(player: AVPlayer) {
        guard let presenter = topViewController() else { return }
        let controller = AVPlayerViewController()
        controller.player = player
        controller.modalPresentationStyle = .fullScreen
        presenter.present(controller, animated: true) {
            player.play()
        }
    }

    @MainActor
    private static func topViewController() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let window = scenes.flatMap(\.windows).first { $0.isKeyWindow }
            ?? scenes.first?.windows.first
        var top = window?.rootViewController
        while let presented = top?.presentedViewController { top = presented }
        return top
    }
}
