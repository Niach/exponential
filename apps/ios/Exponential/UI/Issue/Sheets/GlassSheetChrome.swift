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
    @ViewBuilder let content: () -> Content

    @Environment(\.dismiss) private var dismiss

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
                    Image(systemName: "xmark")
                        .font(.caption.weight(.semibold))
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
        .presentationDetents(detents)
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
                    Image(systemName: "checkmark")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Accent.indigo)
                }
            }
            .padding(.horizontal, 14)
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// Inline search field for the searchable sheets — deliberately NOT system
/// `.searchable` (iOS 26 renders that as a bottom-edge glass bar; see
/// DuplicatePickerSheet, whose styling this extracts).
struct GlassSheetSearchField: View {
    let placeholder: String
    @Binding var text: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField(placeholder, text: $text)
                .textFieldStyle(.plain)
                .foregroundStyle(.white)
                .submitLabel(.search)
            if !text.isEmpty {
                Button {
                    text = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
    }
}

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
                                    Image(systemName: "checkmark")
                                        .font(.subheadline.weight(.semibold))
                                        .foregroundStyle(Accent.indigo)
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
