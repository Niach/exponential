import ExpCore
import SwiftUI

/// #FEED-232 THE count badge ×4 (fixtures/nav-badges.json; web `CountBadge`,
/// Android `CountBadge`, desktop `count_badge`): a 16 pt capsule, min width
/// 16, 4 pt side padding, 10 pt semibold monospaced figures. Nothing renders
/// at zero or less; `99+` past the cap. `attention` = the needs-you yellow
/// with black figures, `muted` = the muted fill with muted-foreground figures.
public struct CountBadge: View {
    let count: Int
    let tone: NavBadges.Tone

    public init(count: Int, tone: NavBadges.Tone = .muted) {
        self.count = count
        self.tone = tone
    }

    public init(_ value: NavBadges.Value) {
        self.init(count: value.count, tone: value.tone)
    }

    public var body: some View {
        let label = NavBadges.countBadgeLabel(count)
        if !label.isEmpty {
            Text(label)
                .font(.system(size: 10, weight: .semibold))
                .monospacedDigit()
                .lineLimit(1)
                .foregroundStyle(tone == .attention ? Color.black : DesignTokens.Palette.mutedForeground)
                .padding(.horizontal, 4)
                .frame(minWidth: 16, minHeight: 16, maxHeight: 16)
                .background(tone == .attention ? DesignTokens.Semantic.yellow : DesignTokens.Palette.muted, in: Capsule())
                .fixedSize()
        }
    }
}
