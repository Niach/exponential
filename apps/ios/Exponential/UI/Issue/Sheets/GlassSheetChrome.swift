import ExpUI
import SwiftUI

// Shared chrome for the redesigned issue-detail property sheets (EXP-240):
// glass background, rounded top, a title header with a circular ✕ close, and
// rows styled as [leading icon · label · trailing checkmark]. `PickerSheet`
// stays untouched — it still serves CreateIssueSheet and the move-board flow.

/// The sheet shell: header (title + circular close) over caller content, on
/// ultra-thin material with a 24pt corner radius.
struct GlassSheetChrome<Content: View>: View {
    let title: String
    var detents: Set<PresentationDetent> = [.medium]
    /// The caller-measured natural height of `content` (EXP-577). While it
    /// fits in half the screen the sheet sizes to exactly header + content +
    /// home-indicator inset instead of a half-screen of empty glass; nil, 0 or
    /// taller falls back to `detents`.
    var fittedContentHeight: CGFloat? = nil
    @ViewBuilder let content: () -> Content

    @Environment(\.dismiss) private var dismiss
    // The sheet's bottom safe-area inset (home indicator) — part of a
    // `.height` detent, so it must be added to the measured content. Values
    // from a raised keyboard are ignored (they'd inflate the detent).
    @State private var bottomInset: CGFloat = 0

    /// The header's vertical footprint (top 18 + 30pt close circle + bottom 10).
    static var headerHeight: CGFloat { 58 }

    private var resolvedDetents: Set<PresentationDetent> {
        guard let height = fittedContentHeight, height > 0 else { return detents }
        let fitted = height + Self.headerHeight + bottomInset
        guard fitted < UIScreen.main.bounds.height * 0.5 else { return detents }
        return [.height(fitted)]
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(title)
                    .font(.headline)
                    .foregroundStyle(.white)
                Spacer()
                Button {
                    dismiss()
                } label: {
                    AppIcon(AppIcons.uiClose, size: AppIcon.Size.small, weight: .semibold)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 30, height: 30)
                        .background(Color.white.opacity(0.08), in: Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Close")
            }
            .padding(.horizontal, 20)
            .padding(.top, 18)
            .padding(.bottom, 10)

            content()
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .onGeometryChange(for: CGFloat.self, of: { $0.safeAreaInsets.bottom }) { inset in
            if inset < 60 { bottomInset = inset }
        }
        .presentationDetents(resolvedDetents)
        .presentationBackground(.ultraThinMaterial)
        .presentationCornerRadius(24)
        .presentationDragIndicator(.hidden)
    }
}

/// One tap-target row inside a glass sheet: leading slot, label, optional
/// trailing checkmark. 44pt minimum height.
struct GlassSheetRow<Leading: View>: View {
    let label: String
    var selected: Bool = false
    var labelOpacity: Double = 1
    let action: () -> Void
    @ViewBuilder let leading: () -> Leading

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                leading()
                    .frame(width: 24)
                Text(label)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(labelOpacity))
                    .lineLimit(1)
                Spacer(minLength: 0)
                if selected {
                    AppIcon(AppIcons.uiCheck, size: 15, weight: .semibold)
                        .foregroundStyle(Color.white)
                }
            }
            .padding(.horizontal, 14)
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// GlassSheetSearchField moved to ExpUI (GlassControls.swift, EXP-604) — its
// baked-in outer margin moved out to the call sites.

/// Glass twin of `PickerSheet` (same generic signature): scrollable rows with
/// a trailing checkmark, immediate commit + dismiss on tap.
struct GlassPickerSheet<Item, ID: Hashable, Row: View>: View {
    let title: String
    let items: [Item]
    let selectedID: ID?
    let idFor: (Item) -> ID
    let onSelect: (Item) -> Void
    @ViewBuilder let row: (Item) -> Row

    @Environment(\.dismiss) private var dismiss

    private struct IdentifiedItem: Identifiable {
        let id: ID
        let value: Item
    }

    var body: some View {
        let identified = items.map { IdentifiedItem(id: idFor($0), value: $0) }
        GlassSheetChrome(title: title) {
            ScrollView {
                VStack(spacing: 2) {
                    ForEach(identified) { wrapped in
                        Button {
                            onSelect(wrapped.value)
                            dismiss()
                        } label: {
                            HStack(spacing: 10) {
                                row(wrapped.value)
                                Spacer(minLength: 0)
                                if let selectedID, wrapped.id == selectedID {
                                    AppIcon(AppIcons.uiCheck, size: 15, weight: .semibold)
                                        .foregroundStyle(Color.white)
                                }
                            }
                            .padding(.horizontal, 14)
                            .frame(minHeight: 44)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 6)
                .padding(.bottom, 16)
            }
        }
    }
}
