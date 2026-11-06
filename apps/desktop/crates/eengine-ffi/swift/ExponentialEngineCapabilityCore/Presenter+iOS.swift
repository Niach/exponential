// #VAPP-265: the iOS seams on UIKit and PhotoKit. Pickers, sheets and
// dialogs are the platform's own, presented on the main queue from
// `CapabilityConfig.presenter` while the calling worker thread waits.
#if os(iOS)
import Foundation
import ObjectiveC
import UIKit

/// Where the capabilities present their UI.
public struct CapabilityConfig {
    /// The view controller to present from (called on the main thread); nil
    /// answers `failed: no view controller to present from`.
    public var presenter: @MainActor () -> UIViewController?

    public init(presenter: @escaping @MainActor () -> UIViewController? = { CapabilityConfig.topViewController() }) {
        self.presenter = presenter
    }

    /// The foreground scene's key window's topmost presented view controller.
    @MainActor public static func topViewController() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let scene = scenes.first { $0.activationState == .foregroundActive } ?? scenes.first
        let window = scene?.windows.first { $0.isKeyWindow } ?? scene?.windows.first
        var top = window?.rootViewController
        while let next = top?.presentedViewController, !next.isBeingDismissed { top = next }
        return top
    }
}

private final class RetainKey {}
private let retainKey = RetainKey()

/// Keeps a (weakly held) delegate alive as long as its controller.
@MainActor package func keep(_ object: AnyObject, alive owner: AnyObject) {
    objc_setAssociatedObject(owner, Unmanaged.passUnretained(retainKey).toOpaque(), object, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
}

@MainActor package func present<T>(_ config: CapabilityConfig, _ controller: UIViewController, _ finish: @escaping (Result<T, Error>) -> Void) {
    guard let top = config.presenter() else {
        finish(.failure(CapabilityFailure.failed("no view controller to present from")))
        return
    }
    if let popover = controller.popoverPresentationController { // iPad: a centred popover
        popover.sourceView = top.view
        popover.sourceRect = CGRect(x: top.view.bounds.midX, y: top.view.bounds.midY, width: 0, height: 0)
        popover.permittedArrowDirections = []
    }
    top.present(controller, animated: true)
}

/// A temp copy of `file` for the duration of `body` (save, share).
package func withTempFile<T>(_ file: PickedFile?, _ body: (URL?) throws -> T) throws -> T {
    guard let file else { return try body(nil) }
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("exponential-engine-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }
    let url = dir.appendingPathComponent(FileNames.safe(file.name))
    try file.data.write(to: url)
    return try body(url)
}
#endif
