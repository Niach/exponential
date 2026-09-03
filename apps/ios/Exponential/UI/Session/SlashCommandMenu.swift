import ExpCore
import ExpUI
import SwiftUI

/// EXP-724: the steer composer's `/` command menu — the curated builtins the
/// session's agent can run, filtered by what has been typed so far. Same
/// chrome as the markdown editor's `@`/`#` autocomplete (`GlassMenuSurface`,
/// `EditorAutocompleteMenu`'s row layout) and the same row content as the web
/// `steer-command-menu.tsx` and Android's `SlashCommandMenu`:
///
///   mono `/name` · muted argument hint · muted description
///
/// Rows are plain `Button`s: tapping routes back through the host, which
/// rewrites the draft and keeps the field first responder, so the keyboard
/// never drops and NOTHING is ever sent by accepting a row.
///
/// The HOST mounts it (above the composer card, gated on focus + matches +
/// not-dismissed) — the menu itself owns no state.
struct SlashCommandMenu: View {
    let commands: [SlashCommand]
    /// Index of the keyboard-highlighted row (hardware keyboards only; ↑/↓
    /// wrap). Out-of-range highlights nothing.
    let highlighted: Int
    let onSelect: (SlashCommand) -> Void

    /// Rows visible before the list scrolls — the catalog is six commands, so
    /// this only ever bites on the unfiltered list.
    private static let visibleRows: CGFloat = 5
    private static let rowHeight: CGFloat = 40

    var body: some View {
        GlassMenuSurface {
            list
        }
    }

    private var list: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(commands.enumerated()), id: \.element.id) { index, command in
                    row(command, isHighlighted: index == highlighted)
                }
            }
            .padding(.vertical, 4)
        }
        .frame(maxHeight: Self.rowHeight * Self.visibleRows + 8)
        .fixedSize(horizontal: false, vertical: true)
    }

    private func row(_ command: SlashCommand, isHighlighted: Bool) -> some View {
        Button {
            onSelect(command)
        } label: {
            HStack(spacing: 8) {
                Text(command.token)
                    .font(.caption.monospaced())
                    // A confirm command discards the conversation — the same
                    // semantic red the destructive menu items wear (×4).
                    .foregroundStyle(
                        command.confirm ? DesignTokens.Semantic.red : Color.white
                    )
                    .lineLimit(1)
                    .layoutPriority(1)
                if !command.argHint.isEmpty {
                    Text(command.argHint)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
                Text(command.description)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .frame(minHeight: Self.rowHeight)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.white.opacity(isHighlighted ? 0.08 : 0))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(command.token) \(command.description)")
    }
}
