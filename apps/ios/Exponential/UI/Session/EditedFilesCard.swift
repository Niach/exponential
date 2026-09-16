import ExpCore
import ExpUI
import SwiftUI

/// EXP-916 — the ONE "edited files" card, ×4 (web `@exp/ui` `EditedFilesCard`,
/// desktop `steer::feed`, Android `EditedFilesCard`).
///
/// A run of consecutive edit calls is ONE card in the transcript
/// (`AgentFeedRow.edits`), not a tool row per call and not a second diff view:
/// its title is what the run DID (`3 files edited`) and each row is a real
/// `DiffFileCard` — the same per-file unit the Changes face draws — stacked
/// FLUSH inside the card and separated by hairlines.
///
/// What the card owns:
/// - the open set: ONE optional set of paths. `nil` = follow the live row —
///   while the run's last call is the transcript's live row THAT row is open by
///   itself, and it folds again the moment the transcript moves on
///   (`liveIndex` → nil). A tap replaces the default with the READER's own set,
///   seeded from what is effectively open, so the live row can be collapsed
///   like any other — and that set survives the settle, because their opens are
///   theirs. Every open body is capped at the contract's inline height. A tap
///   toggles a row IN PLACE — the card never navigates to the Changes face, and
///   there is no "Revert" and no "Show changes".
/// - the publisher's cut: `truncatedLines` under the rows, the same note a file
///   list carries, so a card never silently shows a short diff (EXP-786).
/// - the fold: `diffUi.cardPreviewFiles` rows, then `{n} more` / `Show less`.
///   A live row past the preview clamps the card open so the reader can see
///   what the agent is writing.
struct EditedFilesCard: View {
    /// The card's members, in publish order (the projection's row payload).
    let items: [AgentFeedItem]
    /// The transcript's live tool row (`AgentFeed.liveToolRowId`), or nil.
    var liveItemId: Int?

    /// The reader's OWN open set, or nil while the card still follows the live
    /// row. A tap seeds it from the effective set and owns it from then on.
    @State private var readerOpen: Set<String>?
    /// The reader asked for every row.
    @State private var showAll = false
    /// The per-card parse cache — the patches are re-parsed only when the card
    /// actually changed, never once per frame.
    @State private var memo = EditCardMemo()

    private var card: EditCard.View { memo.card(items, liveItemId: liveItemId) }

    var body: some View {
        let view = card
        let open = openPaths(view)
        VStack(alignment: .leading, spacing: 0) {
            title(view)
            ForEach(shown(view)) { row in
                GlassDivider()
                fileRow(row, open: open.contains(row.path))
            }
            if view.truncatedLines > 0 {
                // EXP-786: what the PUBLISHER cut off the members' patches —
                // the same note a file list carries.
                GlassDivider()
                truncationNote(view.truncatedLines)
            }
            if let fold = foldLabel(view) {
                GlassDivider()
                foldRow(fold)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("edited-files-card")
    }

    // MARK: - Pieces

    private func title(_ view: EditCard.View) -> some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.codingDiff, size: 11)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text(view.title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    private func fileRow(_ row: EditCard.Row, open: Bool) -> some View {
        DiffFileCard(
            file: row.file ?? Diff.File(path: row.path),
            expanded: open,
            compact: true,
            flush: true,
            state: row.state,
            // EVERY open patch is capped, not just the live one: a long
            // file a reader taps open pushes the conversation off screen
            // exactly as the growing live one would.
            maxBodyHeight: Self.inlineDiffMaxHeight,
            onToggle: { toggle(row.path) }
        )
    }

    /// The publisher's own dropped-line count, ×4 (`AgentFeed`'s note).
    private func truncationNote(_ lines: Int) -> some View {
        Text(AgentFeed.diffTruncationNote(lines))
            .font(.caption2)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityIdentifier("edited-files-truncation-note")
    }

    private func foldRow(_ label: String) -> some View {
        Button {
            showAll.toggle()
        } label: {
            HStack(spacing: 6) {
                Text(label)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("edited-files-card-fold")
    }

    // MARK: - The open set

    /// What is effectively open: the reader's set once they have touched the
    /// card, else the live row alone (and nothing at all once it is gone).
    private func openPaths(_ view: EditCard.View) -> Set<String> {
        if let readerOpen { return readerOpen }
        guard let live = view.liveIndex, view.rows.indices.contains(live)
        else { return [] }
        return [view.rows[live].path]
    }

    /// A tap toggles the path in a COPY of the EFFECTIVE set — so the live row
    /// can be collapsed like any other, and the reader's opens outlive it.
    private func toggle(_ path: String) {
        var next = openPaths(card)
        if next.contains(path) {
            next.remove(path)
        } else {
            next.insert(path)
        }
        readerOpen = next
    }

    // MARK: - Rules

    /// Web's `max-h-72` as the contract states it — the ceiling on the ONE
    /// row a live card opens by itself.
    private static let inlineDiffMaxHeight =
        CGFloat(DomainContract.diffUiInlineDiffMaxHeight)

    /// A live row past the preview clamps the card open: the reader must see
    /// the file the agent is writing right now.
    private func clamped(_ view: EditCard.View) -> Bool {
        guard let live = view.liveIndex else { return false }
        return live >= EditCard.preview
    }

    private func shown(_ view: EditCard.View) -> [EditCard.Row] {
        guard !showAll, !clamped(view), view.rows.count > EditCard.preview
        else { return view.rows }
        return Array(view.rows.prefix(EditCard.preview))
    }

    /// `{n} more` / `Show less`, or nil when there is nothing folded away (and
    /// while a live row clamps the card open — that fold is not the reader's).
    private func foldLabel(_ view: EditCard.View) -> String? {
        guard !clamped(view) else { return nil }
        if showAll { return DomainContract.diffUiShowLess }
        return EditCard.moreLabel(view.rows.count)
    }
}

/// EXP-916 — the per-card parse cache.
///
/// `EditCard.card` parses every member's patch, and a SwiftUI body runs on any
/// state change in the transcript (every streamed token of a live run). The
/// card is identified by its first and last member plus the total patch length:
/// a growing run changes its last id, a `tool_update` that lands a patch
/// changes the length, and nothing else can change what the card renders.
///
/// Deliberately NOT `@Observable`: the cache is written DURING a body pass, and
/// an observed write there would invalidate the view that just read it.
@MainActor
final class EditCardMemo {
    private var key: String?
    private var cached: EditCard.View?

    func card(_ items: [AgentFeedItem], liveItemId: Int?) -> EditCard.View {
        var length = 0
        for item in items {
            if case let .tool(_, _, _, _, _, _, settled, failed, diff, _, _) = item {
                length += diff?.utf8.count ?? 0
                if settled { length += 1 }
                if failed { length += 2 }
            }
        }
        let key = [
            String(items.first?.id ?? -1),
            String(items.last?.id ?? -1),
            String(items.count),
            String(length),
            String(liveItemId ?? -1),
        ].joined(separator: "-")
        if key == self.key, let cached { return cached }
        let made = EditCard.card(items, liveItemId: liveItemId)
        self.key = key
        cached = made
        return made
    }
}
