import ExpCore
import SwiftUI

/// EXP-1014 — the STACKED issue chip: `IssueChip` with a couple of ghost chip
/// outlines peeking out behind it, up and to the right in small steps. It is
/// the ×4 shorthand for "this is several issues as ONE piece of work" — a
/// workflow's compound node (a parent run as one batch with its sub-issues) is
/// drawn with it, exactly where a single-issue node draws the plain chip.
///
/// The ghosts are paid for by the view's OWN padding, so the stack reports the
/// full rect it paints: nothing is clipped by the row, the scroller or the
/// sheet it sits in, and no caller has to reserve room for it.
///
/// ```swift
/// IssueChipStack { IssueChip(identifier: "EXP-14 +3", title: title, status: status) }
/// ```
public struct IssueChipStack<Chip: View>: View {
    /// How far each ghost is offset from the one in front of it. Small on
    /// purpose: the stack has to read as one chip, not as three.
    public static var step: CGFloat { 3 }

    private let depth: Int
    private let chip: Chip

    /// - Parameter depth: how many ghosts sit behind the chip (1...3).
    public init(depth: Int = 2, @ViewBuilder chip: () -> Chip) {
        self.depth = min(max(depth, 1), 3)
        self.chip = chip()
    }

    public var body: some View {
        let inset = Self.step * CGFloat(depth)
        chip
            // A background is proposed the chip's own size, so every ghost is
            // the front chip's rect, moved — they cannot drift out of shape
            // when the identifier or the title changes.
            .background(alignment: .topTrailing) {
                ZStack(alignment: .topTrailing) {
                    // Farthest first: the nearest ghost paints over it.
                    ForEach(Array((1...depth).reversed()), id: \.self) { index in
                        ghost
                            .offset(
                                x: Self.step * CGFloat(index),
                                y: -Self.step * CGFloat(index)
                            )
                    }
                }
            }
            .padding(.top, inset)
            .padding(.trailing, inset)
            .accessibilityElement(children: .combine)
    }

    /// One ghost: the chip's own box, nothing in it — the same paint
    /// `ChipBox` uses, so a stack cannot drift from the chip it stands behind.
    private var ghost: some View {
        RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
            .fill(Color(MarkdownStyle.chipBackground))
            .overlay(
                RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
                    .strokeBorder(
                        Color(MarkdownStyle.chipBorder), lineWidth: IssueChipTokens.borderWidth
                    )
            )
    }
}
