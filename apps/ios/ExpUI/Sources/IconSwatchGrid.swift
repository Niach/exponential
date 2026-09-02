import SwiftUI

/// Tap-to-select grid over the shared registry's PICKABLE glyphs
/// (`AppIcons.pickable` — byte-equal to `DomainContract.boardIconValues`, so
/// every name it emits is a storable board icon / action-input value, EXP-273).
/// Deliberately unfiltered: 60 glyphs scan faster than they search (EXP-390
/// dropped the query field on every platform).
///
/// The selection is a plain registry NAME. `""` means nothing picked, which only
/// the `allowsNone` hosts can produce — the board form always carries a glyph,
/// while an optional `icon` action input starts and can return to none
/// (the desktop's `action_icon_picks` entry simply stays absent).
public struct IconSwatchGrid: View {
    @Binding var selection: String
    let icons: [String]
    let columns: Int
    let allowsNone: Bool

    public init(
        selection: Binding<String>,
        icons: [String] = AppIcons.pickable,
        columns: Int = 8,
        allowsNone: Bool = false
    ) {
        self._selection = selection
        self.icons = icons
        self.columns = columns
        self.allowsNone = allowsNone
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            LazyVGrid(
                columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: columns),
                spacing: 8
            ) {
                ForEach(icons, id: \.self) { name in
                    swatch(name)
                }
            }
            if allowsNone, !selection.isEmpty {
                Button("No icon") { selection = "" }
                    .font(.caption)
                    .buttonStyle(.plain)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
    }

    private func swatch(_ name: String) -> some View {
        let selected = selection == name
        return Button {
            selection = name
        } label: {
            AppIcon(name, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(selected ? 1 : TextOpacity.secondary))
                .frame(maxWidth: .infinity)
                .frame(height: 32)
                .background(selected ? GlassTokens.fillActive : GlassTokens.fillSection)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(
                            // The selected ring is deliberately brighter than
                            // any glass rung — a picker needs one unmistakable
                            // "this one"; the resting hairline is a token.
                            selected ? .white.opacity(0.6) : GlassTokens.strokeSection,
                            lineWidth: 1
                        )
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(name)
        .accessibilityAddTraits(selected ? [.isSelected] : [])
    }
}
