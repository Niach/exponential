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
///
/// The HOST mounts it, never `MarkdownEditor` — in a bottom `safeAreaInset`
/// so it rides above the keyboard, gated on `model.showsAutocompleteMenu`.
/// It is up to 208pt tall and the editors it serves live inside scrollers
/// that clip, so anywhere inside the editor's own layout is off-screen for a
/// caret anywhere but the top of a short document (EXP-592).
///
/// EXP-802: the rows are handed in, not read off a model, so a host that
/// drives its editor through a wrapper (the steer composer) can route a pick
/// through its own handler. `init(model:)` is the shorthand every issue editor
/// uses — candidates straight off the model, picks straight back into it.
struct EditorAutocompleteMenu: View {
    let mentions: [MentionMember]
    let issueRefs: [IssueRefCandidate]
    let emoji: [EmojiRecord]
    let onPickMention: (MentionMember) -> Void
    let onPickIssueRef: (IssueRefCandidate) -> Void
    let onPickEmoji: (EmojiRecord) -> Void

    init(
        mentions: [MentionMember],
        issueRefs: [IssueRefCandidate],
        emoji: [EmojiRecord],
        onPickMention: @escaping (MentionMember) -> Void,
        onPickIssueRef: @escaping (IssueRefCandidate) -> Void,
        onPickEmoji: @escaping (EmojiRecord) -> Void
    ) {
        self.mentions = mentions
        self.issueRefs = issueRefs
        self.emoji = emoji
        self.onPickMention = onPickMention
        self.onPickIssueRef = onPickIssueRef
        self.onPickEmoji = onPickEmoji
    }

    /// The candidates a model is offering, applied back to that same model.
    init(model: IssueEditorModel) {
        self.init(
            mentions: model.mentionCandidates,
            issueRefs: model.issueRefCandidates,
            emoji: model.emojiCandidates,
            onPickMention: { model.applyMention($0) },
            onPickIssueRef: { model.applyIssueRef($0) },
            onPickEmoji: { model.applyEmoji($0) }
        )
    }

    /// Rows visible before the list scrolls.
    private static let visibleRows: CGFloat = 5
    private static let rowHeight: CGFloat = 40

    // EXP-603: the chrome is `GlassMenuSurface`, the one menu container —
    // this used to hand-roll a brighter, blurred, shadowed variant of it.
    var body: some View {
        GlassMenuSurface {
            list
        }
    }

    private var list: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 0) {
                if !mentions.isEmpty {
                    ForEach(mentions) { member in
                        row { onPickMention(member) } label: {
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
                } else if !issueRefs.isEmpty {
                    ForEach(issueRefs) { candidate in
                        row { onPickIssueRef(candidate) } label: {
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
                    ForEach(emoji) { record in
                        row { onPickEmoji(record) } label: {
                            Text(record.unicode)
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
