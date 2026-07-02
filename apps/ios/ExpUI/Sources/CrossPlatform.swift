import os
import SwiftUI
import UIKit

public typealias PlatformColor = UIColor
public typealias PlatformImage = UIImage
public typealias PlatformFont = UIFont

/// UIKit⇄SwiftUI bridging shims so shared SwiftUI code stays declarative.
public enum Platform {
    /// Open a URL in the user's default handler (browser / app). Failures are
    /// logged (URLs here are server-derived, so they should never be invalid —
    /// a failure means a misconfigured instance URL worth surfacing in logs).
    public static func open(_ url: URL) {
        UIApplication.shared.open(url) { ok in
            if !ok {
                Logger(subsystem: "at.exponential", category: "Platform")
                    .error("Failed to open URL: \(url.absoluteString, privacy: .public)")
            }
        }
    }

    /// Copy a string to the system pasteboard.
    public static func copyToPasteboard(_ string: String) {
        UIPasteboard.general.string = string
    }
}

extension View {
    /// Shorthand for `.navigationBarTitleDisplayMode(.inline)`.
    public func inlineNavigationTitle() -> some View {
        self.navigationBarTitleDisplayMode(.inline)
    }
}
