import ExpCore
import SwiftUI

/// EXP-1014 — the STACKED issue chip: `IssueChip` with ghost chip outlines
/// peeking out behind it, up and to the right in small steps. It is the ×4
/// shorthand for "this is several issues as ONE piece of work" — a batch run
/// is drawn with it, exactly where a single-issue run draws the plain chip.
///
/// SLOP-15/16: ALWAYS two ghosts whatever the count (the `+N` beside the stack
/// says how many), bare OUTLINES (no fill) in 2pt steps, the far one at half
/// strength — the web's `.issue-chip-ghost` (`STACK_OFFSETS = [4, 2]`).
///
/// The ghosts are paid for by the view's OWN padding, so the stack reports the
/// full rect it paints: nothing is clipped by the row, the scroller or the
/// sheet it sits in, and no caller has to reserve room for it.
///
/// ```swift
/// IssueChipStack { IssueChip(identifier: "EXP-14", title: title, status: status, size: .sm) }
/// ```
public struct IssueChipStack<Chip: View>: View {
    /// How far each ghost is offset from the one in front of it. Small on
    /// purpose: the stack has to read as one chip, not as three.
    public static var step: CGFloat { 2 }
    /// Ghost outlines behind the chip — fixed, never the issue count.
    public static var ghosts: Int { 2 }
    /// The far ghost's strength (web `[data-depth="2"] { opacity: 0.5 }`).
    public static var farGhostOpacity: Double { 0.5 }

    private let chip: Chip

    public init(@ViewBuilder chip: () -> Chip) {
        self.chip = chip()
    }

    public var body: some View {
        let inset = Self.step * CGFloat(Self.ghosts)
        chip
            // A background is proposed the chip's own size, so every ghost is
            // the front chip's rect, moved — they cannot drift out of shape
            // when the identifier or the title changes.
            .background(alignment: .topTrailing) {
                ZStack(alignment: .topTrailing) {
                    // Farthest first: the nearest ghost paints over it.
                    ForEach(Array((1...Self.ghosts).reversed()), id: \.self) { index in
                        ghost
                            .opacity(index == Self.ghosts ? Self.farGhostOpacity : 1)
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

    /// One ghost: the chip's own outline, nothing in it and no fill — the same
    /// border `ChipBox` strokes, so a stack cannot drift from its chip.
    private var ghost: some View {
        RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
            .strokeBorder(Color(MarkdownStyle.chipBorder), lineWidth: IssueChipTokens.borderWidth)
    }
}
