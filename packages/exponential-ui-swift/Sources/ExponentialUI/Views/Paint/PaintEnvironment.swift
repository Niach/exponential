import SwiftUI

/// The text keys a node's resolved visual carries that CSS INHERITS
/// (`letterSpacing`, `textTransform`, `fontStyle`; `textDecoration`
/// propagates): a node sets the ones its visual defines and every
/// `TextLabel` beneath shapes its runs with them. `userSelect` and the
/// direction ride along.
struct TextPaint: Equatable {
    var letterSpacing: CGFloat?
    var decoration: TextDecorationPaint?
    var transform: String?
    var italic: Bool?
    var userSelect: String?

    /// `style`'s text keys over the inherited ones.
    func merged(_ s: PaintStyle) -> TextPaint {
        var t = self
        if let l = s.letterSpacing { t.letterSpacing = l }
        if let d = s.textDecoration { t.decoration = d }
        if let x = s.textTransform { t.transform = x }
        if s.italic { t.italic = true }
        if let u = s.userSelect { t.userSelect = u }
        return t
    }

    var isPlain: Bool { (letterSpacing ?? 0) == 0 && decoration == nil && transform == nil && italic != true }

    /// CSS `text-transform` on a string.
    static func transform(_ text: String, _ how: String?) -> String {
        switch how {
        case "uppercase": text.uppercased()
        case "lowercase": text.lowercased()
        case "capitalize":
            // CSS capitalizes the first letter of each word only (the rest
            // stays as written), unlike `capitalized`.
            {
                var out = ""
                var wordStart = true
                for ch in text {
                    out += wordStart ? String(ch).uppercased() : String(ch)
                    wordStart = ch.isWhitespace || (ch.isPunctuation && ch != "'" && ch != "’")
                }
                return out
            }()
        default: text
        }
    }
}

private struct TextPaintKey: EnvironmentKey {
    static let defaultValue = TextPaint()
}

private struct RTLKey: EnvironmentKey {
    static let defaultValue = false
}

private struct GlyphMirroredKey: EnvironmentKey {
    static let defaultValue = false
}

private struct ReducedMotionKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    /// The inherited text keys of the enclosing nodes.
    var xuiTextPaint: TextPaint {
        get { self[TextPaintKey.self] }
        set { self[TextPaintKey.self] = newValue }
    }

    /// The surface lays out right-to-left (contract §4). The SwiftUI
    /// `layoutDirection` stays pinned to left-to-right (frames come
    /// mirrored from the core); text and directional glyphs read this.
    var xuiRTL: Bool {
        get { self[RTLKey.self] }
        set { self[RTLKey.self] = newValue }
    }

    /// The enclosing Icon node already mirrored its box for rtl.
    var xuiGlyphMirrored: Bool {
        get { self[GlyphMirroredKey.self] }
        set { self[GlyphMirroredKey.self] = newValue }
    }

    /// The surface's reduced-motion setting (the platform's is
    /// `accessibilityReduceMotion`; either stops motion).
    var xuiReducedMotion: Bool {
        get { self[ReducedMotionKey.self] }
        set { self[ReducedMotionKey.self] = newValue }
    }
}
