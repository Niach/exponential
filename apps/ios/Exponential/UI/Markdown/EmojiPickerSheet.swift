import ExpUI
import SwiftUI

/// EXP-551 — the emoji picker. Search field, skin-tone row, a "Recent" section
/// and then the nine dataset groups as grids. Picking inserts UNICODE (toned
/// when the preference is set), never `:shortcode:` text, and records the BASE
/// unicode in the shared recents list.
///
/// Chrome matches the other issue sheets (`GlassSheetChrome` +
/// `GlassSheetSearchField`) so it reads as part of the app rather than as a
/// system keyboard.
struct EmojiPickerSheet: View {
    let onPick: (String) -> Void

    @Environment(\.dismiss) private var dismiss

    @State private var query = ""
    @State private var index: EmojiCatalogIndex?
    @State private var tone: Int
    @State private var recents: [String]

    private let prefs: EmojiPreferences

    /// The tone swatches. A raised hand is the conventional sample and its five
    /// uniform variants are stable unicode, so they are spelled out rather than
    /// looked up in the dataset (which may not have decoded yet).
    private static let toneSwatches = ["\u{270B}", "\u{270B}\u{1F3FB}", "\u{270B}\u{1F3FC}",
                                       "\u{270B}\u{1F3FD}", "\u{270B}\u{1F3FE}", "\u{270B}\u{1F3FF}"]

    private let columns = [GridItem(.adaptive(minimum: 44), spacing: 4)]

    init(preferences: EmojiPreferences = EmojiPreferences(), onPick: @escaping (String) -> Void) {
        self.onPick = onPick
        self.prefs = preferences
        _tone = State(initialValue: preferences.skinTone)
        _recents = State(initialValue: preferences.recents)
    }

    var body: some View {
        GlassSheetChrome(title: "Emoji", detents: [.medium, .large]) {
            VStack(spacing: 0) {
                GlassSheetSearchField(placeholder: "Search emoji", text: $query)
                toneRow
                content
            }
        }
        .onAppear {
            // Usually already decoded by the editor's preload; this covers a
            // picker opened first (and is a no-op once loaded).
            index = EmojiCatalog.shared.loadNow()
        }
    }

    // MARK: - Skin tone

    private var toneRow: some View {
        HStack(spacing: 4) {
            ForEach(Self.toneSwatches.indices, id: \.self) { value in
                Button {
                    tone = value
                    prefs.skinTone = value
                } label: {
                    Text(Self.toneSwatches[value])
                        .font(.system(size: 20))
                        .frame(width: 34, height: 34)
                        .background(
                            Circle().fill(
                                value == tone ? Color.white.opacity(0.15) : Color.white.opacity(0.06)
                            )
                        )
                }
                .buttonStyle(.plain)
                .accessibilityLabel(value == 0 ? "No skin tone" : "Skin tone \(value)")
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
    }

    // MARK: - Content

    @ViewBuilder
    private var content: some View {
        if let index {
            if trimmedQuery.isEmpty {
                browseGrid(index)
            } else {
                searchGrid(index)
            }
        } else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private var trimmedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func searchGrid(_ index: EmojiCatalogIndex) -> some View {
        let results = index.search(trimmedQuery, limit: EmojiCatalog.pickerLimit)
        return ScrollView {
            if results.isEmpty {
                Text("No emoji found")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .frame(maxWidth: .infinity)
                    .padding(.top, 32)
            } else {
                LazyVGrid(columns: columns, spacing: 4) {
                    ForEach(results) { cell($0) }
                }
                .padding(.horizontal, 12)
                .padding(.bottom, 16)
            }
        }
    }

    private func browseGrid(_ index: EmojiCatalogIndex) -> some View {
        let recentRecords = recents.compactMap { index.find(unicode: $0) }
        return ScrollView {
            LazyVGrid(columns: columns, spacing: 4, pinnedViews: [.sectionHeaders]) {
                if !recentRecords.isEmpty {
                    Section {
                        ForEach(recentRecords) { cell($0) }
                    } header: {
                        sectionHeader("Recent")
                    }
                }
                ForEach(index.groups.indices, id: \.self) { group in
                    Section {
                        ForEach(index.emojis(inGroup: group)) { cell($0) }
                    } header: {
                        sectionHeader(index.groups[group])
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.bottom, 16)
        }
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.caption2.weight(.semibold))
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.vertical, 6)
            .background(.ultraThinMaterial)
    }

    private func cell(_ record: EmojiRecord) -> some View {
        Button {
            // Recents store the BASE unicode so the row re-tones when the
            // preference changes; the INSERT carries the tone.
            prefs.recordRecent(record.unicode)
            recents = prefs.recents
            onPick(record.applyingTone(tone))
            dismiss()
        } label: {
            Text(record.applyingTone(tone))
                .font(.system(size: 28))
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(record.label)
    }
}
