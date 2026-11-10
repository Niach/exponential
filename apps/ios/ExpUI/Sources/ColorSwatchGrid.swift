import SwiftUI

/// Canonical label/board color palette — identical to the web app's
/// `packages/ui/src/label-colors.ts` so all clients pick from the same set.
public let LABEL_COLORS: [String] = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308",
    "#84cc16", "#22c55e", "#10b981", "#14b8a6", "#06b6d4",
    "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7",
    "#ec4899", "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
]

/// Default color the server applies to new labels/boards when none is chosen.
public let DEFAULT_LABEL_COLOR = "#6366f1"

/// EXP-862: THE colour picker — `IconPicker`'s twin, and the second trigger of
/// the board form's one row: the same 36pt rounded square (a rounded square is
/// a PICKER, a circle is an ACTION) filled with the current colour, opening the
/// palette in a sheet instead of spilling a swatch grid into the form.
/// Web's `ui/color-picker.tsx` and desktop's `board_form::color_picker` are the
/// same control.
public struct ColorSwatchPicker: View {
    @Binding var selection: String
    @State private var isPresented = false

    public init(selection: Binding<String>) {
        self._selection = selection
    }

    public var body: some View {
        Button {
            isPresented = true
        } label: {
            Circle()
                .fill(Color(hex: selection) ?? .gray)
                .frame(width: 18, height: 18)
                .frame(width: 36, height: 36)
                .background(GlassTokens.fillCard)
                .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
                .overlay(
                    RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                        .strokeBorder(GlassTokens.strokeStrong, lineWidth: 1)
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Color")
        .sheet(isPresented: $isPresented) {
            GlassSheetChrome(title: "Color") {
                ColorSwatchGrid(
                    selection: Binding(
                        get: { selection },
                        set: { next in
                            selection = next
                            isPresented = false
                        }
                    )
                )
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
            }
            .preferredColorScheme(.dark)
        }
    }
}

/// A tap-to-select swatch grid bound to a hex string — web's
/// `ui/color-swatch-grid.tsx` cell for cell (#EXP-1774): a 28pt cell with a
/// 2pt ring (foreground when picked, clear otherwise) around a 20pt dot,
/// wrapping with a 6pt gap. The ring sits INSIDE the cell, never a halo.
public struct ColorSwatchGrid: View {
    @Binding var selection: String
    let colors: [String]

    public init(selection: Binding<String>, colors: [String] = LABEL_COLORS) {
        self._selection = selection
        self.colors = colors
    }

    public var body: some View {
        FlowLayout(spacing: 6) {
            ForEach(colors, id: \.self) { color in
                let picked = selection.caseInsensitiveCompare(color) == .orderedSame
                Button {
                    selection = color
                } label: {
                    Circle()
                        .fill(Color(hex: color) ?? .gray)
                        .frame(width: 20, height: 20)
                        .frame(width: 28, height: 28)
                        .overlay(
                            Circle().strokeBorder(
                                picked ? DesignTokens.Palette.foreground : Color.clear,
                                lineWidth: 2
                            )
                        )
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(color)
                .accessibilityAddTraits(picked ? .isSelected : [])
            }
        }
    }
}
