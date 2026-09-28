import UIKit

/// Walks the UIAccessibility tree of the key window the way VoiceOver does
/// (container children in `accessibilityElement(at:)` order, leaves where
/// `isAccessibilityElement`), returning the labels. XCUITest's element
/// snapshot ignores `accessibilityHidden`/`accessibilitySortPriority`, so it
/// is not a VoiceOver proxy; this is closer.
@MainActor
enum VappA11yDump {
    static func labels() -> [String] {
        guard let window = UIApplication.shared.connectedScenes
            .compactMap({ $0 as? UIWindowScene })
            .flatMap(\.windows)
            .first(where: \.isKeyWindow)
        else { return [] }
        var out: [String] = []
        walk(window, into: &out, depth: 0)
        return out
    }

    private static func walk(_ object: NSObject, into out: inout [String], depth: Int) {
        guard depth < 60 else { return }
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
                if let element = object.accessibilityElement(at: index) as? NSObject {
                    walk(element, into: &out, depth: depth + 1)
                }
            }
            return
        }
        if let view = object as? UIView {
            for sub in view.subviews where !sub.isHidden { walk(sub, into: &out, depth: depth + 1) }
        }
    }
}
