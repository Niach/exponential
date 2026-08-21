import SwiftUI

/// EXP-575: THE icon picker — one slim 36pt swatch showing the current pick
/// that opens the curated grid (`IconSwatchGrid`) in a medium sheet, so the
/// 60-glyph grid never sits inline in a form. Every surface that picks an icon
/// (board form, Start-coding `icon` inputs) renders this; web, desktop and
/// Android ship the same shape.
///
/// The selection is a registry NAME; `""` means nothing picked, which only
/// `allowsNone` hosts can produce — they get a "No icon" reset in the sheet
/// and a dashed placeholder swatch.
public struct IconPicker: View {
    @Binding var selection: String
    let allowsNone: Bool
    /// Tints the picked glyph (the board color) for a live preview.
    let tint: Color?
    @State private var isPresented = false

    public init(selection: Binding<String>, allowsNone: Bool = false, tint: Color? = nil) {
        self._selection = selection
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
            .background(Color.white.opacity(0.06))
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(
                        Color.white.opacity(0.12),
                        style: StrokeStyle(lineWidth: 1, dash: selection.isEmpty ? [3, 3] : [])
                    )
            )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(selection.isEmpty ? "Pick an icon" : "Icon: \(selection)")
        .sheet(isPresented: $isPresented) {
            NavigationStack {
                ScrollView {
                    IconSwatchGrid(
                        selection: Binding(
                            get: { selection },
                            set: { next in
                                selection = next
                                isPresented = false
                            }
                        ),
                        allowsNone: allowsNone
                    )
                    .padding(16)
                }
                .navigationTitle("Icon")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Done") { isPresented = false }
                    }
                }
            }
            .presentationDetents([.medium, .large])
            .presentationBackground(.ultraThinMaterial)
            .preferredColorScheme(.dark)
        }
    }
}
