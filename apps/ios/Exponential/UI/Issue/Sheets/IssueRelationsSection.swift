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

    @Environment(\.motion) private var motion

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

    @ViewBuilder
    private func bandView(_ band: IssueRelationsView.Band) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(motion.standard) { vm.toggleRelationBand(band.key) }
            } label: {
                GlassSectionBand(band.title) {
                    HStack(spacing: 6) {
                        AppIcon(
                            band.expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight,
                            size: 12,
                            weight: .medium
                        )
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(width: 14)
                        AppIcon(Self.iconName(band.key), size: 14)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                } trailing: {
                    Text("\(band.count)")
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(band.title), \(band.count)")
            .accessibilityValue(band.expanded ? "Expanded" : "Collapsed")
            .accessibilityIdentifier("relation-band-\(band.key.rawValue)")

            if band.expanded {
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
                if let label = band.more ?? band.less {
                    GlassDivider()
                    Button {
                        withAnimation(motion.standard) { vm.toggleRelationBandShowAll(band.key) }
                    } label: {
                        Text(label)
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
