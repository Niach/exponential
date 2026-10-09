import SwiftUI
#if canImport(UIKit)
import UIKit

/// Walks the UIAccessibility tree of the key window the way VoiceOver does
/// (container children in `accessibilityElement(at:)` order, leaves where
/// `isAccessibilityElement`), returning the labels. XCUITest's element
/// snapshot ignores `accessibilitySortPriority`, so it is no VoiceOver
/// proxy; this is (the VAPP-4 finding).
@MainActor
enum AccessibilityWalk {
    static var trace: [String] = []

    static func labels() -> [String] {
        let windows = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).flatMap(\.windows)
        guard let window = windows.first(where: \.isKeyWindow) ?? windows.first else { return [] }
        var out: [String] = []
        trace.removeAll()
        walk(window, into: &out, depth: 0)
        return out
    }

    private static func walk(_ object: NSObject, into out: inout [String], depth: Int) {
        guard depth < 80 else { return }
        if depth < 6 {
            trace.append("\(String(repeating: " ", count: depth))\(type(of: object)) a11y=\(object.isAccessibilityElement) elements=\((object.accessibilityElements as? [NSObject])?.count ?? -1) count=\(object.accessibilityElementCount()) subviews=\((object as? UIView)?.subviews.count ?? -1)")
        }
        if object.accessibilityElementsHidden { return }
        if object.isAccessibilityElement {
            if let label = object.accessibilityLabel, !label.isEmpty { out.append(label) }
            return
        }
        if let elements = object.accessibilityElements as? [NSObject] {
            for element in elements { walk(element, into: &out, depth: depth + 1) }
            return
        }
        let count = object.accessibilityElementCount()
        if count != NSNotFound, count > 0 {
            for index in 0..<count {
                if let element = object.accessibilityElement(at: index) as? NSObject { walk(element, into: &out, depth: depth + 1) }
            }
            return
        }
        if let view = object as? UIView {
            for sub in view.subviews where !sub.isHidden { walk(sub, into: &out, depth: depth + 1) }
        }
    }
}
#else
enum AccessibilityWalk {
    static func labels() -> [String] { [] }
}
#endif
