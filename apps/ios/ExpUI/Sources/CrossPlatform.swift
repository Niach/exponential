import os
import SwiftUI

#if os(iOS)
import UIKit
public typealias PlatformColor = UIColor
public typealias PlatformImage = UIImage
public typealias PlatformFont = UIFont
#elseif os(macOS)
import AppKit
public typealias PlatformColor = NSColor
public typealias PlatformImage = NSImage
public typealias PlatformFont = NSFont
#endif

/// Cross-platform shims so shared SwiftUI code doesn't `#if os(...)` inline.
public enum Platform {
    /// Open a URL in the user's default handler (browser / app). Failures are
    /// logged (URLs here are server-derived, so they should never be invalid —
    /// a failure means a misconfigured instance URL worth surfacing in logs).
    public static func open(_ url: URL) {
        #if os(iOS)
        UIApplication.shared.open(url) { ok in
            if !ok {
                Logger(subsystem: "at.exponential", category: "Platform")
                    .error("Failed to open URL: \(url.absoluteString, privacy: .public)")
            }
        }
        #elseif os(macOS)
        if !NSWorkspace.shared.open(url) {
            Logger(subsystem: "at.exponential", category: "Platform")
                .error("Failed to open URL: \(url.absoluteString, privacy: .public)")
        }
        #endif
    }

    /// Copy a string to the system pasteboard.
    public static func copyToPasteboard(_ string: String) {
        #if os(iOS)
        UIPasteboard.general.string = string
        #elseif os(macOS)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(string, forType: .string)
        #endif
    }
}

extension View {
    /// `.navigationBarTitleDisplayMode(.inline)` is iOS-only; this no-ops on macOS
    /// so shared views can call it unconditionally.
    public func inlineNavigationTitle() -> some View {
        #if os(iOS)
        return self.navigationBarTitleDisplayMode(.inline)
        #else
        return self
        #endif
    }
}
