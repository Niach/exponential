import ExpCore
import SwiftUI

/// EXP-1248/EXP-1291: THE session row, ×4 (web `@exp/ui` `SessionRow`,
/// desktop `run_rows::run_row`, Android `SessionRow.kt`; geometry =
/// `list-item.json`, the row facts = `threads-list.json`). ONE line:
/// [tree guides][lead at 12 + 14·depth][mono id · title][state]. The state
/// is label-free (no captions, threads-list.json): a lock while the thread
/// is private, the inline spinner while it works, the amber dot while it
/// needs you. No fold chevron, no trailing chevron, no buttons.
///
/// The row is CONTENT: the host wraps it in its own NavigationLink/Button
/// (ExpUI cannot see the app's routes) and hands the lead in (a thread's
/// glyph, an agent's icon in its colour), sized here to the 14pt lead box.
public struct SessionRow<Lead: View>: View {
    private let identifier: String?
    private let title: String
    private let guide: TreeGuide
    private let isPrivate: Bool
    private let busy: Bool
    private let needsYou: Bool
    private let active: Bool
    private let lead: Lead

    /// - Parameters:
    ///   - identifier: the thread's first linked issue (`APP-14`), nil = none.
    ///   - guide: this row's connector (`TreeGuides.compute(depths:)[i]`);
    ///     its depth indents the lead 14pt per level.
    ///   - isPrivate: started on a device not shared with the team (`!shared`).
    ///   - busy: a session of the tree works — the inline spinner.
    ///   - needsYou: a live session waits on a person — the amber dot.
    public init(
        identifier: String? = nil,
        title: String,
        guide: TreeGuide = TreeGuide(),
        isPrivate: Bool = false,
        busy: Bool = false,
        needsYou: Bool = false,
        active: Bool = false,
        @ViewBuilder lead: () -> Lead
    ) {
        self.identifier = identifier
        self.title = title
        self.guide = guide
        self.isPrivate = isPrivate
        self.busy = busy
        self.needsYou = needsYou
        self.active = active
        self.lead = lead()
    }

    public var body: some View {
        HStack(alignment: .center, spacing: ListItem.gap) {
            lead
                .frame(width: ListItem.mark, height: ListItem.mark)
                .accessibilityHidden(true)
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
            Spacer(minLength: 0)
            if isPrivate {
                AppIcon(AppIcons.uiPrivate, size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .accessibilityLabel("Private")
            }
            if busy {
                InlineSpinner()
                    .accessibilityLabel("Working")
            }
            if needsYou {
                Circle()
                    .fill(DesignTokens.Semantic.yellow)
                    .frame(width: 8, height: 8)
                    .accessibilityLabel("Needs you")
            }
        }
        .padding(.leading, ListItem.leadX(depth: guide.depth))
        .padding(.trailing, ListItem.base)
        .frame(height: ListItem.small)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(TreeGuidesOverlay(guide: guide, base: ListItem.base))
        .contentShape(Rectangle())
        .flatRow(isActive: active)
    }
}

/// The compact list caption ("5m", "2h") off a synced timestamp — the
/// polish pin's LIST wording ×4 (`RelativeTime.compact`); empty when it does
/// not parse. Drafts still read it.
public func relativeWireDate(_ s: String) -> String {
    RelativeTime.compact(wire: s)
}
