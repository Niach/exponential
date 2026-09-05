import ExpUI
import ExpCore
import SwiftUI

/// The Relations block of the Properties sheet (EXP-736). On mobile relations
/// live ONLY here — the detail page keeps its chip box and its timeline — so
/// the block carries both the entry point ("Add relation") and the list of
/// what is already linked, each row read from THIS issue's side ("blocks" vs
/// "blocked by") and leading with the OTHER issue's status glyph.
struct IssueRelationsSection: View {
    let relations: [IssueRelationRow]
    let onAdd: () -> Void
    let onRemove: (IssueRelationRow) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Same plain heading as the Labels block above it (EXP-698 r4).
            Text("Relations")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            VStack(spacing: 0) {
                GlassMetaRow(
                    label: "Add relation",
                    icon: AppIcons.relationSection,
                    iconColor: .white.opacity(TextOpacity.secondary),
                    value: "",
                    action: onAdd
                )

                ForEach(relations) { relation in
                    GlassDivider()
                    row(relation)
                }
            }
            .glassSection()
        }
    }

    private func row(_ relation: IssueRelationRow) -> some View {
        // The counterpart's status comes RESOLVED from the view model (EXP-314):
        // a custom status has no builtin anchor to draw, so reading the enum
        // here would show the wrong glyph and color.
        let status = relation.otherStatus
        return HStack(spacing: 10) {
            AppIcon(status.iconName, size: AppIcon.Size.small)
                .foregroundStyle(status.color)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                Text(relation.other.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                HStack(spacing: 4) {
                    // Which side this issue is on, then the counterpart's
                    // identifier — the caption reads "blocked by EXP-12".
                    Text(relation.label)
                    if let identifier = relation.other.identifier {
                        Text(identifier)
                            .font(.caption.monospaced())
                    }
                }
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }

            Spacer(minLength: 8)

            Button {
                onRemove(relation)
            } label: {
                AppIcon(AppIcons.uiClose, size: 12, weight: .semibold)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Remove relation")
        }
        .padding(.leading, GlassMetaRowTokens.horizontalPadding)
        .padding(.trailing, 6)
        .padding(.vertical, 6)
    }
}
