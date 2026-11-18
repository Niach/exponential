import ExpCore
import ExpUI
import ImageIO
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// #EXP-1926 / #EXP-1949 (image-actions.json): THE image menu on a rendered
/// content picture: a long press (`glassLongPressMenu` over GlassMenu, never
/// the system `.contextMenu`) offers Copy image, then Share. Both act on the
/// ORIGINAL bytes as PNG (`ImageBytes.png`): Copy puts them on the pasteboard
/// (`toasts.copied`, any failure = `copyFailed`), Share hands a PNG file to
/// THE share sheet (`ActivityShareSheet`; cancelling shows nothing, a failure
/// = `shareFailed`). The picture's tap stays `onTap`'s.
extension View {
    /// - Parameters:
    ///   - enabled: false = no menu (a picture still loading, failed or
    ///     blocked: the press falls through).
    ///   - name: the attachment's file name, for the shared file
    ///     (`shareFileName`); nil = `image.png`.
    ///   - onTap: the picture's own tap; nil = taps pass through.
    ///   - bytes: the picture's original bytes (cache first).
    func imageActionsMenu(
        enabled: Bool = true,
        name: String? = nil,
        onTap: (() -> Void)? = nil,
        bytes: @escaping @MainActor () async throws -> Data
    ) -> some View {
        modifier(ImageActionsMenu(enabled: enabled, name: name, onTap: onTap, bytes: bytes))
    }
}

/// #EXP-1949 (`pressMenu`): an element with a menu of its own around
/// pictures (the own bubble's system menu) hands this down; a long press that
/// began on a rendered picture claims it, and the element's menu stands down
/// (the image menu, never merged).
@MainActor
@Observable
final class ImagePressClaim {
    private(set) var active = false
    private var serial = 0

    func claim() {
        active = true
        serial += 1
        let mine = serial
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(1.5))
            guard let self, self.serial == mine else { return }
            self.active = false
        }
    }
}

private struct ImagePressClaimKey: EnvironmentKey {
    static let defaultValue: ImagePressClaim? = nil
}

extension EnvironmentValues {
    var imagePressClaim: ImagePressClaim? {
        get { self[ImagePressClaimKey.self] }
        set { self[ImagePressClaimKey.self] = newValue }
    }
}

private struct ImageActionsMenu: ViewModifier {
    let enabled: Bool
    let name: String?
    let onTap: (() -> Void)?
    let bytes: @MainActor () async throws -> Data

    @Environment(\.toaster) private var toaster
    @Environment(\.imagePressClaim) private var claim
    @State private var share: ImageShareFile?
    /// The file the sheet shares, removed once it closes.
    @State private var shared: URL?

    func body(content: Content) -> some View {
        content
            .glassLongPressMenu(enabled: enabled, onTap: onTap) {
                ForEach(ImageActions.imageMenu(.touch, canShareFile: true), id: \.self) { item in
                    switch item {
                    case .copyImage:
                        GlassMenuItem(ImageActions.Copy.copyImage, icon: AppIcons.uiCopy) { copy() }
                            .accessibilityIdentifier(ImageActions.TestIds.copyImage)
                    case .share:
                        GlassMenuItem(ImageActions.Copy.share, icon: AppIcons.uiShare) { startShare() }
                            .accessibilityIdentifier(ImageActions.TestIds.share)
                    }
                }
            }
            // Sooner than any menu around it opens: that one stands down.
            // Outside the hold, which it would otherwise win over.
            .simultaneousGesture(LongPressGesture(minimumDuration: 0.15).onEnded { _ in
                if enabled { claim?.claim() }
            }, including: claim == nil ? .subviews : .all)
            // VoiceOver has no hold: the items are actions of the picture.
            .accessibilityAction(named: ImageActions.Copy.copyImage) { if enabled { copy() } }
            .accessibilityAction(named: ImageActions.Copy.share) { if enabled { startShare() } }
            .sheet(item: $share, onDismiss: discardShared) { target in
                ActivityShareSheet(items: [target.url])
            }
    }

    private func copy() {
        Task { @MainActor in
            do {
                let png = try ImageBytes.png(try await bytes())
                UIPasteboard.general.setData(png, forPasteboardType: UTType.png.identifier)
                toaster.success(ImageActions.Copy.copied)
            } catch {
                toaster.error(ImageActions.Copy.copyFailed)
            }
        }
    }

    private func discardShared() {
        guard let shared else { return }
        ImageBytes.discard(shared)
        self.shared = nil
    }

    private func startShare() {
        Task { @MainActor in
            do {
                let png = try ImageBytes.png(try await bytes())
                let url = try ImageBytes.shareFile(png, name: name)
                shared = url
                share = ImageShareFile(url: url)
            } catch {
                toaster.error(ImageActions.Copy.shareFailed)
            }
        }
    }
}

private struct ImageShareFile: Identifiable {
    let url: URL
    var id: String { url.path }
}

/// The copy rule (`copyFormat`): PNG bytes as they are, anything else decoded
/// and re-encoded to PNG (an animated GIF / WebP = its first frame), never
/// downscaled.
enum ImageBytes {
    enum Failure: Error { case undecodable, unencodable }

    static func png(_ data: Data) throws -> Data {
        if ImageActions.copyEncoding(data) == .passthrough { return data }
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let frame = CGImageSourceCreateImageAtIndex(source, 0, nil) else { throw Failure.undecodable }
        let out = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(out, UTType.png.identifier as CFString, 1, nil) else { throw Failure.unencodable }
        CGImageDestinationAddImage(destination, frame, nil)
        guard CGImageDestinationFinalize(destination) else { throw Failure.unencodable }
        return out as Data
    }

    /// `png` as a temporary file named `shareFileName(name)` (its own
    /// directory, so two shares of one name never collide).
    static func shareFile(_ png: Data, name: String?) throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("image-share", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent(ImageActions.shareFileName(name))
        try png.write(to: url, options: .atomic)
        return url
    }

    static func discard(_ url: URL) {
        try? FileManager.default.removeItem(at: url.deletingLastPathComponent())
    }
}
