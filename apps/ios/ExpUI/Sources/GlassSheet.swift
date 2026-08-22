import SwiftUI

// Shared chrome for the glass property sheets (EXP-240) — the iOS twin of
// Android's ui/components/GlassSheet.kt: glass background, rounded top, a title
// header with a circular ✕ close, and rows styled as
// [leading icon · label · trailing checkmark].
//
// Moved here from Exponential/UI/Issue/Sheets/GlassSheetChrome.swift by
// EXP-603, which retired the last stock `PickerSheet` call sites: a sheet is
// now the app's ONE answer to "pick one of these", so it belongs in ExpUI next
// to the glass controls (EXP-604) rather than under the issue feature folder.
//
// Absent twins, deliberately: Android's `GlassSheet` takes a `BackgroundBottom`
// gradient stop because a Compose bottom sheet paints its own container; here
// `.presentationBackground(.ultraThinMaterial)` continues the app background
// through the system's own presentation host.

/// The sheet shell: header (title + circular close) over caller content, on
/// ultra-thin material with a 24pt corner radius.
public struct GlassSheetChrome<Content: View>: View {
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

    public init(
        title: String,
        detents: Set<PresentationDetent> = [.medium],
        fittedContentHeight: CGFloat? = nil,
        @ViewBuilder content: @escaping () -> Content
    ) {
        self.title = title
        self.detents = detents
        self.fittedContentHeight = fittedContentHeight
        self.content = content
    }

    /// The header's vertical footprint (top 18 + 30pt close circle + bottom 10).
    public static var headerHeight: CGFloat { 58 }

    private var resolvedDetents: Set<PresentationDetent> {
        guard let height = fittedContentHeight, height > 0 else { return detents }
        let fitted = height + Self.headerHeight + bottomInset
        guard fitted < UIScreen.main.bounds.height * 0.5 else { return detents }
        return [.height(fitted)]
    }

    public var body: some View {
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
public struct GlassSheetRow<Leading: View>: View {
    let label: String
    var selected: Bool = false
    var labelOpacity: Double = 1
    let action: () -> Void
    @ViewBuilder let leading: () -> Leading

    public init(
        label: String,
        selected: Bool = false,
        labelOpacity: Double = 1,
        action: @escaping () -> Void,
        @ViewBuilder leading: @escaping () -> Leading
    ) {
        self.label = label
        self.selected = selected
        self.labelOpacity = labelOpacity
        self.action = action
        self.leading = leading
    }

    public var body: some View {
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

/// The ONE "pick one of these" sheet (EXP-603 retired the stock `PickerSheet`
/// it was cloned from): scrollable rows with a trailing checkmark, immediate
/// commit + dismiss on tap. It measures its own rows, so a three-option list
/// presents as a short sheet instead of a half-screen of empty glass.
public struct GlassPickerSheet<Item, ID: Hashable, Row: View>: View {
    let title: String
    let items: [Item]
    let selectedID: ID?
    let idFor: (Item) -> ID
    let onSelect: (Item) -> Void
    @ViewBuilder let row: (Item) -> Row

    @Environment(\.dismiss) private var dismiss
    @State private var contentHeight: CGFloat = 0

    public init(
        title: String,
        items: [Item],
        selectedID: ID?,
        idFor: @escaping (Item) -> ID,
        onSelect: @escaping (Item) -> Void,
        @ViewBuilder row: @escaping (Item) -> Row
    ) {
        self.title = title
        self.items = items
        self.selectedID = selectedID
        self.idFor = idFor
        self.onSelect = onSelect
        self.row = row
    }

    private struct IdentifiedItem: Identifiable {
        let id: ID
        let value: Item
    }

    public var body: some View {
        let identified = items.map { IdentifiedItem(id: idFor($0), value: $0) }
        GlassSheetChrome(title: title, fittedContentHeight: contentHeight) {
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
                .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { height in
                    contentHeight = height
                }
            }
        }
    }
}
