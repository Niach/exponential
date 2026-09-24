import ExpCore
import ExpUI
import SwiftUI

/// EXP-996: the GROUP row of a session tree — a workflow's runs, or a stack's
/// (web `SessionGroupRow`, desktop `session_tree`, Android `SessionGroupRow`).
///
/// A group row is NOT a run: no state dot, no device, no kill. It only names
/// what the rows below it belong to, says how many they are, and folds them
/// away — a group IS its children, so their count is exactly what the reader is
/// deciding to hide. Its glyph is the whole difference between a workflow and a
/// stack (EXP-996: the two are deliberately not unified).
///
/// The fold is the CHEVRON, not the whole row as on web: a control nested in a
/// link's label has its tap swallowed here (the `RunningSessionRow` trap), so
/// the chevron folds and the rest of the row opens the workflow — a stack has
/// no screen of its own and only folds.
struct SessionGroupRow: View {
    /// A CONCEPT from the registry (`AppIcons.navWorkflows` / `.prStack`).
    let glyph: String
    let title: String
    /// How many nodes sit under this row.
    let count: Int
    /// Where the row's own tap goes — a workflow group opens its workflow.
    let open: RunningSessionRowOpen
    /// The node key, for the row's accessibility id (web `data-testid`).
    let key: String
    var expanded: Bool = true
    var onToggle: (() -> Void)?

    var body: some View {
        HStack(alignment: .center, spacing: 6) {
            foldControl
            primary
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
        .accessibilityIdentifier("session-group-\(key)")
    }

    /// The same fold affordance a run row wears, in a PLAIN Button OUTSIDE the
    /// link's label — with a group's own labels ("child runs" is what a run
    /// has, not a group).
    @ViewBuilder
    private var foldControl: some View {
        Button { onToggle?() } label: {
            AppIcon(expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 12)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .frame(width: 14, height: GlassTokens.controlSize)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(
            expanded ? SessionTree.collapseGroupLabel : SessionTree.expandGroupLabel
        )
        .accessibilityIdentifier("session-fold")
    }

    @ViewBuilder
    private var primary: some View {
        switch open {
        case let .route(route):
            NavigationLink(value: route) { content }
                .buttonStyle(.plain)
        case let .action(action):
            Button(action: action) { content }
                .buttonStyle(.plain)
        case .none:
            content
        }
    }

    private var content: some View {
        HStack(spacing: 10) {
            AppIcon(glyph, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text(title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 0)
            Text("\(count)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .frame(minHeight: GlassTokens.controlSize)
        .contentShape(Rectangle())
    }
}
