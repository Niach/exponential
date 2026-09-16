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
/// - the open set. While the run's last call is the transcript's live row, ONLY
///   that row is open and its body is capped at the contract's inline height;
///   the moment the transcript moves on (`liveIndex` → nil) every row folds
///   again. A tap toggles a row IN PLACE — the card never navigates to the
///   Changes face, and there is no "Revert" and no "Show changes".
/// - the fold: `diffUi.cardPreviewFiles` rows, then `{n} more` / `Show less`.
///   A live row past the preview clamps the card open so the reader can see
///   what the agent is writing.
struct EditedFilesCard: View {
    /// The card's members, in publish order (the projection's row payload).
    let items: [AgentFeedItem]
    /// The transcript's live tool row (`AgentFeed.liveToolRowId`), or nil.
    var liveItemId: Int?

    /// Reader overrides on top of the live rule, keyed by path.
    @State private var openPaths: Set<String> = []
    /// The reader asked for every row.
    @State private var showAll = false
    /// The per-card parse cache — the patches are re-parsed only when the card
    /// actually changed, never once per frame.
    @State private var memo = EditCardMemo()

    private var card: EditCard.View { memo.card(items, liveItemId: liveItemId) }

    var body: some View {
        let view = card
        VStack(alignment: .leading, spacing: 0) {
            title(view)
            ForEach(Array(shown(view).enumerated()), id: \.element.path) { index, row in
                GlassDivider()
                fileRow(row, index: index, live: view.liveIndex)
            }
            if let fold = foldLabel(view) {
                GlassDivider()
                foldRow(fold)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
        .onChange(of: view.liveIndex) { _, now in
            // The run settled (or the transcript moved past it): the row that
            // opened itself folds away with it.
            if now == nil { openPaths = [] }
        }
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

    private func fileRow(_ row: EditCard.Row, index: Int, live: Int?) -> some View {
        let isLive = live == index
        return DiffFileCard(
            file: row.file ?? Diff.File(path: row.path),
            expanded: isLive || openPaths.contains(row.path),
            compact: true,
            flush: true,
            state: row.state,
            maxBodyHeight: isLive ? Self.inlineDiffMaxHeight : nil,
            onToggle: {
                if openPaths.contains(row.path) {
                    openPaths.remove(row.path)
                } else {
                    openPaths.insert(row.path)
                }
            }
        )
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
