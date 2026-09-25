import SwiftUI

/// EXP-575: THE icon picker — one slim 36pt swatch showing the current pick
/// that opens the curated grid (`IconSwatchGrid`) in a sheet, so a 96-glyph
/// grid never sits inline in a form. Every surface that picks an icon (board
/// form, Start-coding `icon` inputs, EXP-924's device settings) renders this;
/// `icons` names the SET to offer (the board one unless a surface says
/// otherwise), never a fork of this view.
///
/// EXP-771, the shape rule: a circle is an ACTION and a rounded square is a
/// PICKER, so the trigger and the grid's cells are rounded squares at the
/// radius ladder's MD step (`GlassTokens.rowRadius`, 10) while every icon-only
/// action button stays a circle. Web (`rounded-md`), desktop (the theme radius)
/// and Android (`GlassTokens.RowRadius`) draw the same corner.
///
/// The selection is a registry NAME; `""` means nothing picked, which only
/// `allowsNone` hosts can produce — they get a "No icon" reset in the sheet
/// and a dashed placeholder swatch.
public struct IconPicker: View {
    @Binding var selection: String
    /// The set the sheet offers. EXP-924: `AppIcons.devicePickable` for a
    /// machine, the board set for everything else.
    let icons: [String]
    let allowsNone: Bool
    /// Tints the picked glyph (the board color) for a live preview.
    let tint: Color?

    public init(
        selection: Binding<String>,
        icons: [String] = AppIcons.pickable,
        allowsNone: Bool = false,
        tint: Color? = nil
    ) {
        self._selection = selection
        self.icons = icons
        self.allowsNone = allowsNone
        self.tint = tint
    }

    // EXP-1021 re-homed this onto `GlassPicker`: the trigger and the grid are
    // unchanged, but the SHEET is now the shared primitive's, so the icon
    // picker cannot drift from the other nine. A 96-glyph grid is not a row
    // list, so it rides as the primitive's `panel` — the one caller that has
    // one.
    public var body: some View {
        GlassPicker(
            items: [PickerItem<String>](),
            mode: .single,
            value: [selection],
            onChange: { _ in },
            title: "Icon",
            panel: {
                AnyView(
                    IconPickerPanel(
                        selection: $selection, icons: icons, allowsNone: allowsNone
                    )
                )
            },
            trigger: {
                Group {
                    if selection.isEmpty {
                        AppIcon(AppIcons.uiIconPlaceholder, size: AppIcon.Size.medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    } else {
                        AppIcon(selection, size: AppIcon.Size.medium)
                            .foregroundStyle(tint ?? .white)
                    }
                }
                .frame(width: 36, height: 36)
                .background(GlassTokens.fillCard)
                .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
                .overlay(
                    RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                        .strokeBorder(
                            GlassTokens.strokeStrong,
                            style: StrokeStyle(lineWidth: 1, dash: selection.isEmpty ? [3, 3] : [])
                        )
                )
                .accessibilityLabel(selection.isEmpty ? "Pick an icon" : "Icon: \(selection)")
            }
        )
    }
}

/// The grid inside the picker's sheet. Its own view so it can read the sheet's
/// `dismiss` — a pick closes, exactly like a row pick in `.single` mode does.
private struct IconPickerPanel: View {
    @Binding var selection: String
    let icons: [String]
    let allowsNone: Bool

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        IconSwatchGrid(
            selection: Binding(
                get: { selection },
                set: { next in
                    selection = next
                    dismiss()
                }
            ),
            icons: icons,
            allowsNone: allowsNone
        )
        .padding(16)
    }
}
