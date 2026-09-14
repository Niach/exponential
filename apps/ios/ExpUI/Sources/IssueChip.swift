import ExpCore
import SwiftUI

/// The chip's own geometry (EXP-885). The PAINT comes from `MarkdownStyle`
/// (`chipBackground` / `chipBorder` / `chipTokenColor` / `chipCornerRadius`),
/// which is what makes this view and the markdown painter one look instead of
/// two: a token changed for the editor's chip moves this one with it.
public enum IssueChipTokens {
    /// Gap between glyph, identifier, title and the ✕. Matches the advance the
    /// markdown chip kerns onto its hidden `#` cell so the glyph clears the
    /// identifier (`MarkdownStyle.chipStatusIconGap`).
    public static let spacing: CGFloat = MarkdownStyle.chipStatusIconGap
    /// The capsule the layout manager paints is the TEXT rect inset by
    /// `(-2, 1)` — a chip that is a real view has no line box to borrow, so it
    /// states the same optical padding directly.
    public static let horizontalPadding: CGFloat = 6
    public static let verticalPadding: CGFloat = 3
    public static let borderWidth: CGFloat = 1
    /// The ✕ of the removable variant, and the clear button sat over it. The
    /// glyph is small by design (it is not the chip's subject); the 28pt target
    /// is what a thumb actually hits.
    public static let removeGlyphSize: CGFloat = 10
    public static let removeHitSize: CGFloat = 28
    /// A 60-character title (`IssueRefs.chipTitle`) is wider than a phone, so
    /// the title truncates at this measure even where nothing else bounds it
    /// (the composer's horizontal scroller).
    public static let titleMaxWidth: CGFloat = 220
}

/// EXP-885 — the ONE issue badge. Every chip-shaped issue reference outside the
/// markdown text painter renders through this view, and it is styled from the
/// SAME `MarkdownStyle` chip tokens that painter uses, so the two cannot drift:
/// a small rounded RECT (never a capsule — `GlassPill` is a different object),
/// a hairline border, and status glyph · muted identifier · title, left to
/// right, tail-truncated.
///
/// `onTap` opens the issue; `onRemove` adds the composer's ✕ INSIDE the chip.
/// Per EXP-827 the two are not meant to combine: where a ✕ exists the chip body
/// is inert, so only the ✕ acts.
public struct IssueChip: View {
    private let identifier: String?
    private let title: String?
    private let iconName: String?
    private let statusColor: Color?
    private let bodySize: CGFloat?
    private let onTap: (() -> Void)?
    private let onRemove: (() -> Void)?

    /// - Parameters:
    ///   - bodySize: the prose measure the chip sits in (EXP-787's seam — the
    ///     transcript reads at `DesignTokens.Transcript.bodySize`). `nil` = the
    ///     interchange body font, exactly like `MarkdownStyle.bodyFont`.
    public init(
        identifier: String?,
        title: String?,
        iconName: String?,
        statusColor: Color?,
        bodySize: CGFloat? = nil,
        onTap: (() -> Void)? = nil,
        onRemove: (() -> Void)? = nil
    ) {
        self.identifier = identifier
        self.title = title
        self.iconName = iconName
        self.statusColor = statusColor
        self.bodySize = bodySize
        self.onTap = onTap
        self.onRemove = onRemove
    }

    /// The resolved-status convenience (EXP-314): a chip never picks a glyph or
    /// a colour itself, it takes the row's.
    public init(
        identifier: String?,
        title: String?,
        status: ResolvedIssueStatus?,
        bodySize: CGFloat? = nil,
        onTap: (() -> Void)? = nil,
        onRemove: (() -> Void)? = nil
    ) {
        self.init(
            identifier: identifier,
            title: title,
            iconName: status?.iconName,
            statusColor: status?.color,
            bodySize: bodySize,
            onTap: onTap,
            onRemove: onRemove
        )
    }

    /// The anchor-enum convenience, for the surfaces that only ever hold the
    /// wire status (the composer's issue options).
    public init(
        identifier: String?,
        title: String?,
        status: IssueStatus,
        bodySize: CGFloat? = nil,
        onTap: (() -> Void)? = nil,
        onRemove: (() -> Void)? = nil
    ) {
        self.init(
            identifier: identifier,
            title: title,
            iconName: status.iconName,
            statusColor: status.color,
            bodySize: bodySize,
            onTap: onTap,
            onRemove: onRemove
        )
    }

    private var pointSize: CGFloat { MarkdownStyle.resolvedBodyFont(bodySize).pointSize }

    /// The same 60-character cut the markdown chip takes, so one issue reads
    /// identically wherever it is chipped.
    private var chipTitle: String? {
        guard let title else { return nil }
        let cut = IssueRefs.chipTitle(title)
        return cut.isEmpty ? nil : cut
    }

    private var accessibilityText: String {
        [identifier, chipTitle].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " ")
    }

    public var body: some View {
        if onRemove == nil {
            // One element: the chip reads as "EXP-42 Fix login flow", and as a
            // button when it opens the issue.
            chip
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(accessibilityText)
        } else {
            // The ✕ is its own control, so the chip must stay a container or
            // VoiceOver loses the only action it has.
            chip
                .overlay(alignment: .trailing) { removeButton }
                .accessibilityElement(children: .contain)
        }
    }

    @ViewBuilder
    private var chip: some View {
        if let onTap {
            Button(action: onTap) { surface.contentShape(Rectangle()) }
                .buttonStyle(.plain)
        } else {
            surface
        }
    }

    private var surface: some View {
        HStack(spacing: IssueChipTokens.spacing) {
            if let iconName {
                AppIcon(iconName, size: MarkdownStyle.chipStatusIconSize)
                    .foregroundStyle(statusColor ?? Color(MarkdownStyle.chipTokenColor))
            }
            if let identifier, !identifier.isEmpty {
                Text(identifier)
                    .font(.system(size: pointSize, design: .monospaced))
                    .foregroundStyle(Color(MarkdownStyle.chipTokenColor))
                    .lineLimit(1)
            }
            if let chipTitle {
                Text(chipTitle)
                    .font(.system(size: pointSize, weight: .medium))
                    .foregroundStyle(Color(MarkdownStyle.textColor))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: IssueChipTokens.titleMaxWidth, alignment: .leading)
            }
            if onRemove != nil {
                AppIcon(AppIcons.uiClose, size: IssueChipTokens.removeGlyphSize)
                    .foregroundStyle(Color(MarkdownStyle.chipTokenColor))
            }
        }
        .padding(.horizontal, IssueChipTokens.horizontalPadding)
        .padding(.vertical, IssueChipTokens.verticalPadding)
        .background(
            RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
                .fill(Color(MarkdownStyle.chipBackground))
        )
        .overlay(
            RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
                .strokeBorder(
                    Color(MarkdownStyle.chipBorder), lineWidth: IssueChipTokens.borderWidth)
        )
    }

    /// The chip's ONE control when it is removable: a clear target over the ✕,
    /// so the glyph can stay 10pt without being unhittable.
    @ViewBuilder
    private var removeButton: some View {
        if let onRemove {
            Button(action: onRemove) {
                Color.clear
                    .frame(
                        width: IssueChipTokens.removeHitSize,
                        height: IssueChipTokens.removeHitSize
                    )
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Remove \(accessibilityText)")
        }
    }
}
