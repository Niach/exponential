import ExpCore
import ExpUI
import SwiftUI

/// EXP-581: the ONE `@mention` / `#issue` / `:emoji` candidate menu of the
/// markdown editor — a vertical list, the same row layout as the web
/// `autocomplete-rows.tsx` and Android's `AutocompleteMenu`:
///
/// - issue rows: status glyph · mono identifier · title
/// - member rows: name · email
/// - emoji rows: glyph · `:shortcode:` · label
///
/// It replaced the horizontal capsule strips the editor used to overlay on
/// its top edge, which covered the line being typed in short editors (the
/// comment composer) and were unusable for more than a couple of candidates.
///
/// Rows are plain `Button`s: tapping routes through the model's `apply*`,
/// which keeps the text view first responder, so the keyboard never drops.
/// The editor anchors this under the focused block (`MarkdownEditor
/// .autocompletePlacement == .belowFocusedBlock`); a host whose editor is
/// clipped mounts it itself via `.external`.
struct EditorAutocompleteMenu: View {
    let model: IssueEditorModel

    /// Rows visible before the list scrolls.
    private static let visibleRows: CGFloat = 5
    private static let rowHeight: CGFloat = 40

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 0) {
                if !model.mentionCandidates.isEmpty {
                    ForEach(model.mentionCandidates) { member in
                        row { model.applyMention(member) } label: {
                            Text(member.name)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            Spacer(minLength: 8)
                            if member.email != member.name {
                                Text(member.email)
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                    .lineLimit(1)
                            }
                        }
                    }
                } else if !model.issueRefCandidates.isEmpty {
                    ForEach(model.issueRefCandidates) { candidate in
                        row { model.applyIssueRef(candidate) } label: {
                            if let status = candidate.status {
                                AppIcon(status.iconName, size: 16)
                                    .foregroundStyle(status.color)
                                    .frame(width: 16, height: 16)
                            } else {
                                Color.clear.frame(width: 16, height: 16)
                            }
                            Text(candidate.identifier)
                                .font(.caption.monospaced())
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .lineLimit(1)
                                .layoutPriority(1)
                            Text(candidate.title)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                                .truncationMode(.tail)
                            Spacer(minLength: 0)
                        }
                    }
                } else {
                    ForEach(model.emojiCandidates) { record in
                        row { model.applyEmoji(record) } label: {
                            Text(record.applyingTone(model.emojiSkinTone))
                                .font(.system(size: 18))
                                .frame(width: 24)
                            Text(":\(record.shortcodes.first ?? record.label):")
                                .font(.caption.monospaced())
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .lineLimit(1)
                                .layoutPriority(1)
                            Text(record.label)
                                .font(.subheadline)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .lineLimit(1)
                            Spacer(minLength: 0)
                        }
                    }
                }
            }
            .padding(.vertical, 4)
        }
        .frame(maxHeight: Self.rowHeight * Self.visibleRows + 8)
        .fixedSize(horizontal: false, vertical: true)
        .background(.ultraThinMaterial)
        .background(Color.black.opacity(0.35))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(
            RoundedRectangle(cornerRadius: 12).stroke(Color.white.opacity(0.12), lineWidth: 0.5)
        )
        .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
    }

    private func row<Label: View>(
        action: @escaping () -> Void,
        @ViewBuilder label: () -> Label
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                label()
            }
            .padding(.horizontal, 12)
            .frame(minHeight: Self.rowHeight)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
