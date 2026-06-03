import SwiftUI

/// Canonical label/project color palette — identical to the web app's
/// `apps/web/src/lib/label-colors.ts` so the three clients pick from the same set.
public let LABEL_COLORS: [String] = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308",
    "#84cc16", "#22c55e", "#10b981", "#14b8a6", "#06b6d4",
    "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7",
    "#ec4899", "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
]

/// Default color the server applies to new labels/projects when none is chosen.
public let DEFAULT_LABEL_COLOR = "#6366f1"

/// A tap-to-select swatch grid bound to a hex string. Shared by the iOS and
/// macOS label/project color pickers (ports the iOS `WorkspaceLabelsSection`
/// palette so both platforms stay in sync).
public struct ColorSwatchGrid: View {
    @Binding var selection: String
    let colors: [String]
    let swatchSize: CGFloat

    public init(selection: Binding<String>, colors: [String] = LABEL_COLORS, swatchSize: CGFloat = 22) {
        self._selection = selection
        self.colors = colors
        self.swatchSize = swatchSize
    }

    public var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: swatchSize + 6), spacing: 6)], spacing: 6) {
            ForEach(colors, id: \.self) { color in
                Button {
                    selection = color
                } label: {
                    Circle()
                        .fill(Color(hex: color) ?? .gray)
                        .frame(width: swatchSize, height: swatchSize)
                        .overlay(
                            Circle().stroke(Color.white, lineWidth: selection == color ? 2 : 0)
                        )
                }
                .buttonStyle(.plain)
            }
        }
    }
}
