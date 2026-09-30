import ExpUI
import ExpCore
import SwiftUI

/// The Relations block of the Properties sheet (EXP-736, EXP-1097). The
/// detail page draws the parent line and the Sub-issues section; every OTHER
/// relation lives here, as ONE foldable band per side (`IssueRelationsView`
/// ×4): chevron · side glyph · title · count right, then the flat relation
/// rows, capped at 3 behind "Show N more" / "Show less". Blockers open by
/// default while one is still open; duplicates and Related stay folded. The
/// fold state is the screen's view state (the view model), never persisted.
///
/// "Add" opens the two-stage relation picker; a row opens its issue, its
/// long-press menu removes the relation.
struct IssueRelationsSection: View {
    let vm: IssueDetailViewModel
    let onAdd: () -> Void
    let onOpen: (String) -> Void
    let onRemove: (IssueRelationRow) -> Void

    var body: some View {
        let view = vm.relationsView
        VStack(alignment: .leading, spacing: 8) {
            // Same plain heading as the Labels block above it (EXP-698 r4),
            // the entry point on its trailing edge.
            GlassSectionHeader(IssueRelationsView.Copy.relations) {
                GlassPill(
                    IssueRelationsView.Copy.add,
                    icon: AppIcons.uiAdd,
                    mode: .action(onAdd)
                )
                .accessibilityIdentifier("relations-add")
            }
            .padding(.bottom, -8)

            ForEach(view.bands) { band in
                bandView(band)
            }
        }
    }

    private func bandView(_ band: IssueRelationsView.Band) -> some View {
        IssueRelationBand(
            title: band.title,
            icon: Self.iconName(band.key),
            count: band.count,
            expanded: band.expanded,
            moreLabel: band.more ?? band.less,
            identifier: "relation-band-\(band.key.rawValue)",
            onToggle: { vm.toggleRelationBand(band.key) },
            onToggleShowAll: { vm.toggleRelationBandShowAll(band.key) }
        ) {
            IssueRelationRowList(
                rows: band.rows,
                vm: vm,
                removeLabel: "Remove relation",
                onOpen: onOpen,
                onRemove: { otherId in
                    guard let row = vm.relationRow(band: band.key, otherId: otherId) else {
                        return
                    }
                    onRemove(row)
                }
            )
        }
    }

    /// One concept glyph per relation SIDE (EXP-736).
    static func iconName(_ key: IssueRelationsView.BandKey) -> String {
        switch key {
        case .blockedBy: AppIcons.relationBlockedBy
        case .blocking: AppIcons.relationBlocks
        case .duplicateOf, .duplicatedBy: AppIcons.relationDuplicate
        case .related: AppIcons.relationRelated
        }
    }
}

/// THE relations card's foldable band ×4 (EXP-1097): chevron · side glyph ·
/// title · count right, then its flat rows and — when rows exceed the cap —
/// the "Show N more" / "Show less" row. The properties sheet's Relations
/// block and the Related work sheet (SLOP-16 r5) both draw it.
struct IssueRelationBand<Rows: View>: View {
    let title: String
    let icon: String
    let count: Int
    let expanded: Bool
    /// "Show N more" / "Show less", nil when nothing overflows the cap.
    let moreLabel: String?
    let identifier: String
    let onToggle: () -> Void
    let onToggleShowAll: () -> Void
    @ViewBuilder let rows: () -> Rows

    @Environment(\.motion) private var motion

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(motion.standard) { onToggle() }
            } label: {
                GlassSectionBand(title) {
                    HStack(spacing: 6) {
                        AppIcon(
                            expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight,
                            size: 12,
                            weight: .medium
                        )
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(width: 14)
                        AppIcon(icon, size: 14)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                } trailing: {
                    Text("\(count)")
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(title), \(count)")
            .accessibilityValue(expanded ? "Expanded" : "Collapsed")
            .accessibilityIdentifier(identifier)

            if expanded {
                rows()
                if let moreLabel {
                    GlassDivider()
                    Button {
                        withAnimation(motion.standard) { onToggleShowAll() }
                    } label: {
                        Text(moreLabel)
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(.horizontal, 12)
                            .frame(
                                maxWidth: .infinity,
                                minHeight: IssueRelationRowTokens.moreRowHeight,
                                alignment: .leading
                            )
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }
}
