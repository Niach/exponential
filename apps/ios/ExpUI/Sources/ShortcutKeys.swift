import ExpCore
import SwiftUI
internal import ExponentialUIPrimitives

/// S-3: a push-to-talk binding as THE keycaps (the web `ShortcutKeys`): one
/// SDK `KbdView` per `shortcutLabel` part in a `KbdGroup`, in the app's
/// tokens; empty = `copy.shortcutNone` as muted words.
public struct ShortcutKeys: View {
    let value: String

    public init(_ value: String) {
        self.value = value
    }

    public var body: some View {
        if value.isEmpty {
            Text(Voice.shortcutLabel(value))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
        } else {
            KbdGroup {
                ForEach(Array(Voice.shortcutLabel(value).components(separatedBy: Voice.ShortcutGrammar.separator).enumerated()), id: \.offset) { _, part in
                    KbdView(part)
                }
            }
            .primitiveTokens(PrimitiveTokens(
                mutedForeground: DesignTokens.Palette.mutedForeground,
                muted: GlassTokens.fillActive,
                radiusSm: DesignTokens.Radius.sm
            ))
            .accessibilityElement(children: .combine)
            .accessibilityLabel(Voice.shortcutLabel(value))
        }
    }
}
