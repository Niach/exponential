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
    @State private var isPresented = false

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

    public var body: some View {
        Button {
            isPresented = true
        } label: {
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
        }
        .buttonStyle(.plain)
        .accessibilityLabel(selection.isEmpty ? "Pick an icon" : "Icon: \(selection)")
        .sheet(isPresented: $isPresented) {
            GlassSheetChrome(title: "Icon") {
                IconSwatchGrid(
                    selection: Binding(
                        get: { selection },
                        set: { next in
                            selection = next
                            isPresented = false
                        }
                    ),
                    icons: icons,
                    allowsNone: allowsNone
                )
                .padding(16)
            }
            .preferredColorScheme(.dark)
        }
    }
}
