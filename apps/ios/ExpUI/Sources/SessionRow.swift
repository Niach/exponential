import ExpCore
import SwiftUI

/// EXP-1248: THE session row, ×4 (web `@exp/ui` `SessionRow`, desktop
/// `run_rows::run_row`, Android `SessionRow.kt`; geometry + caption rule =
/// `list-item.json`). One anatomy in two sizes: [tree guides][run mark at
/// 12 + 14·depth][mono id · title][caption, big only][device glyph]. No fold
/// chevron, no trailing chevron, no buttons: any inline control before the
/// text pushes a parent's mark or title off its child's, the bug this row
/// exists to end. Children always show.
///
/// The row is CONTENT: the host wraps it in its own NavigationLink/Button
/// (ExpUI cannot see the app's routes) and hands the run mark in as `lead`
/// (the app's `AgentRunMark`), sized here to the 14pt lead box.
public struct SessionRow<Lead: View>: View {
    public enum Size: Sendable {
        /// One line, 32pt: mark · id · title · device.
        case small
        /// Two lines, 52pt: + the caption (`SessionRowCaption`).
        case big
    }

    private let size: Size
    private let identifier: String?
    private let title: String
    private let caption: String?
    private let captionTone: SessionStatusTone
    private let guide: TreeGuide
    private let deviceIcon: String?
    private let deviceName: String?
    private let active: Bool
    private let dimmed: Bool
    private let lead: Lead

    /// - Parameters:
    ///   - identifier: an issue run's identifier or a batch's `EXP-874 +2`.
    ///   - caption: the big row's second line; ignored when small.
    ///   - guide: this row's connector (`TreeGuides.compute(depths:)[i]`);
    ///     its depth indents the lead 14pt per level.
    ///   - deviceIcon: the host device's glyph (`DeviceIconDisplay`).
    ///   - dimmed: an offline host.
    public init(
        size: Size = .big,
        identifier: String? = nil,
        title: String,
        caption: String? = nil,
        captionTone: SessionStatusTone = .muted,
        guide: TreeGuide = TreeGuide(),
        deviceIcon: String? = nil,
        deviceName: String? = nil,
        active: Bool = false,
        dimmed: Bool = false,
        @ViewBuilder lead: () -> Lead
    ) {
        self.size = size
        self.identifier = identifier
        self.title = title
        self.caption = caption
        self.captionTone = captionTone
        self.guide = guide
        self.deviceIcon = deviceIcon
        self.deviceName = deviceName
        self.active = active
        self.dimmed = dimmed
        self.lead = lead()
    }

    public var body: some View {
        HStack(alignment: .center, spacing: ListItem.gap) {
            lead
                .frame(width: ListItem.mark, height: ListItem.mark)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    if let identifier, !identifier.isEmpty {
                        Text(identifier)
                            .font(.caption.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                            .fixedSize()
                    }
                    Text(title)
                        .font(.subheadline)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                if size == .big, let caption, !caption.isEmpty {
                    Text(caption)
                        .font(.caption)
                        .foregroundStyle(captionTone.color)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            }
            Spacer(minLength: 0)
            if let deviceIcon {
                AppIcon(deviceIcon, size: 14)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .accessibilityLabel(deviceName ?? "")
            }
        }
        .padding(.leading, ListItem.leadX(depth: guide.depth))
        .padding(.trailing, ListItem.base)
        .frame(height: size == .big ? ListItem.big : ListItem.small)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(TreeGuidesOverlay(guide: guide, base: ListItem.base))
        .opacity(dimmed ? 0.6 : 1)
        .contentShape(Rectangle())
        .flatRow(isActive: active)
    }
}

extension SessionStatusTone {
    /// The caption's ink (`session-display.json` statusTone): muted reads
    /// secondary, the rest the issue-status palette.
    public var color: Color {
        switch self {
        case .muted: .white.opacity(TextOpacity.secondary)
        case .amber: DesignTokens.Semantic.yellow
        case .emerald: DesignTokens.Semantic.green
        case .sky: DesignTokens.Semantic.blue
        }
    }
}

/// The compact list caption ("5m", "2h") off a synced timestamp — the
/// polish pin's LIST wording ×4 (`RelativeTime.compact`); empty when it does
/// not parse. (Moved here with the retired `RunningSessionRow`; drafts and
/// the issue's Runs list still read it.)
public func relativeWireDate(_ s: String) -> String {
    RelativeTime.compact(wire: s)
}
