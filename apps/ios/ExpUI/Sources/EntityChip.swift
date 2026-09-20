import ExpCore
import SwiftUI

// EXP-920 — THE entity chip, presentational half. An Exponential MCP tool's
// settled transcript row names what its answer touched (`preview.refs`,
// contract `entityRefKind`), and every client draws ONE chip per ref: a glyph
// and a short label in the very box the issue chip owns. `IssueChip` renders
// THROUGH `ChipBox` below with its three parts, so an issue chip and a board
// chip on the same row cannot disagree by a pixel.
//
// The glyph is handed IN: an issue passes its resolved status glyph, every
// other kind the concept `EntityPreview.refIcon` names (`EntityChipIcon`); a
// chip stays free of live queries either way. The label and its optional
// detail are the contract's (`EntityPreview.chipLabel` / `chipDetail`),
// fixture-locked ×4.

/// Concept → registry glyph for the entity kinds. `AppIcons` is generated as
/// one static member per concept, not a lookup table, so the entity kinds
/// name theirs here — `EntityChipTests` locks that every contract kind's
/// concept resolves to a shipped imageset.
public enum EntityChipIcon {
    public static let glyphs: [String: String] = [
        "ui-issue": AppIcons.uiIssue,
        "nav-boards": AppIcons.navBoards,
        "nav-actions": AppIcons.navActions,
        "nav-automations": AppIcons.navAutomations,
        "notification-issue-comment": AppIcons.notificationIssueComment,
        "coding-running": AppIcons.codingRunning,
        "settings-labels": AppIcons.settingsLabels,
        "settings-statuses": AppIcons.settingsStatuses,
        "nav-workflows": AppIcons.navWorkflows,
        "ui-device": AppIcons.uiDevice,
        "ui-avatar-placeholder": AppIcons.uiAvatarPlaceholder,
        "ui-repository": AppIcons.uiRepository,
        "ui-team": AppIcons.uiTeam,
        "ui-invite": AppIcons.uiInvite,
        "nav-notifications": AppIcons.navNotifications,
        "nav-support": AppIcons.navSupport,
        "ui-attach": AppIcons.uiAttach,
        "ui-checklist": AppIcons.uiChecklist,
    ]

    /// The glyph a concept draws; an unknown concept wears the list glyph,
    /// like an unknown kind does.
    public static func glyph(forConcept concept: String) -> String {
        glyphs[concept] ?? AppIcons.uiChecklist
    }

    /// The glyph a ref draws (`EntityPreview.refIcon`).
    public static func glyph(for ref: EntityRef) -> String {
        glyph(forConcept: EntityPreview.refIcon(ref))
    }
}

/// The shared chip box (EXP-885 paint, EXP-920 shape): a small rounded RECT
/// from the markdown chip's own tokens, a hairline border, and — left to
/// right — an optional leading glyph, an optional mono muted `identifier`, an
/// optional medium `title` and an optional trailing ✕ glyph. Every chip
/// outside the text painter is this box; the callers only decide the parts.
public struct ChipBox<Leading: View>: View {
    let identifier: String?
    let title: String?
    let titleColor: Color
    let bodySize: CGFloat?
    let showsRemoveGlyph: Bool
    @ViewBuilder let leading: () -> Leading

    public init(
        identifier: String?,
        title: String?,
        titleColor: Color = Color(MarkdownStyle.textColor),
        bodySize: CGFloat? = nil,
        showsRemoveGlyph: Bool = false,
        @ViewBuilder leading: @escaping () -> Leading
    ) {
        self.identifier = identifier
        self.title = title
        self.titleColor = titleColor
        self.bodySize = bodySize
        self.showsRemoveGlyph = showsRemoveGlyph
        self.leading = leading
    }

    private var pointSize: CGFloat { MarkdownStyle.resolvedBodyFont(bodySize).pointSize }

    public var body: some View {
        HStack(spacing: IssueChipTokens.spacing) {
            leading()
            if let identifier, !identifier.isEmpty {
                Text(identifier)
                    .font(.system(size: pointSize, design: .monospaced))
                    .foregroundStyle(Color(MarkdownStyle.chipTokenColor))
                    .lineLimit(1)
            }
            if let title, !title.isEmpty {
                Text(title)
                    .font(.system(size: pointSize, weight: .medium))
                    .foregroundStyle(titleColor)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: IssueChipTokens.titleMaxWidth, alignment: .leading)
            }
            if showsRemoveGlyph {
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
}

/// One entity chip: a glyph, the contract's label and — for an issue that
/// carries both — the identifier as the mono lead-in with the title beside
/// it, exactly the `IssueChip` look. `muted` is the unsynced row: the same
/// box with its text in the token colour and, per the caller, no target.
public struct EntityChip: View {
    private let label: String
    private let detail: String?
    private let iconName: String?
    private let iconColor: Color?
    private let bodySize: CGFloat?
    private let muted: Bool
    private let onTap: (() -> Void)?

    /// - Parameters:
    ///   - label: the chip's text (`EntityPreview.chipLabel`).
    ///   - detail: the secondary text (`EntityPreview.chipDetail`); when it is
    ///     present the label reads as the mono identifier and the detail as
    ///     the title.
    ///   - iconName: the registry glyph (a resolved status glyph for a synced
    ///     issue, else `EntityChipIcon.glyph(for:)`); nil draws none.
    ///   - iconColor: the glyph's tint; nil = the muted token colour.
    ///   - bodySize: the prose measure the chip sits in (the transcript reads
    ///     at `DesignTokens.Transcript.bodySize`).
    public init(
        label: String,
        detail: String? = nil,
        iconName: String?,
        iconColor: Color? = nil,
        bodySize: CGFloat? = nil,
        muted: Bool = false,
        onTap: (() -> Void)? = nil
    ) {
        self.label = label
        self.detail = detail
        self.iconName = iconName
        self.iconColor = iconColor
        self.bodySize = bodySize
        self.muted = muted
        self.onTap = onTap
    }

    /// The contract convenience: label, detail and glyph straight off the ref.
    public init(
        ref: EntityRef,
        iconName: String? = nil,
        iconColor: Color? = nil,
        bodySize: CGFloat? = nil,
        muted: Bool = false,
        onTap: (() -> Void)? = nil
    ) {
        self.init(
            label: EntityPreview.chipLabel(ref),
            detail: EntityPreview.chipDetail(ref),
            iconName: iconName ?? EntityChipIcon.glyph(for: ref),
            iconColor: iconColor,
            bodySize: bodySize,
            muted: muted,
            onTap: onTap
        )
    }

    private var accessibilityText: String {
        [label, detail].compactMap { $0 }.joined(separator: " ")
    }

    public var body: some View {
        if let onTap {
            Button(action: onTap) { surface.contentShape(Rectangle()) }
                .buttonStyle(.plain)
                .accessibilityLabel(accessibilityText)
        } else {
            surface
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(accessibilityText)
        }
    }

    private var surface: some View {
        ChipBox(
            identifier: detail == nil ? nil : label,
            title: detail ?? label,
            titleColor: muted ? Color(MarkdownStyle.chipTokenColor) : Color(MarkdownStyle.textColor),
            bodySize: bodySize
        ) {
            if let iconName {
                AppIcon(iconName, size: MarkdownStyle.chipStatusIconSize)
                    .foregroundStyle(iconColor ?? Color(MarkdownStyle.chipTokenColor))
            }
        }
    }
}
