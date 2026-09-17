import ExpCore
import ExpUI
import SwiftUI

/// EXP-895/916 — ONE file's diff, ×4 (web `FileDiffCard`): a glassy
/// `letter · dimmed dir/basename · +a −b · chevron` header over the collapsible
/// unified body. It takes a `Diff.File` off the shared parser and nothing
/// else — no `PrFile`, no patch string — so the review page, the run's Changes
/// face, an issue's Changes face and the transcript's edited-files card all
/// draw the SAME card.
///
/// EXP-916 adds the two things the transcript needed to stop having its own
/// diff view: `flush` (no outer border or radius — the card stacks inside a
/// parent card, hairline-separated) and `state`, the edit run's row states
/// where there is no patch (`pending` = the header with an inert chevron,
/// `done` = the plain settled header, `failed` = the header in the danger tint;
/// none of the three opens). The header wears the glass SECTION fill, the same
/// band a section header does — never an opaque background.
struct DiffFileCard: View {
    let file: Diff.File
    let expanded: Bool
    /// A transcript's tool card: tighter type, no old-side gutter.
    var compact = false
    /// EXP-916: borderless and square — one row of an edited-files card.
    var flush = false
    /// EXP-916: what this row IS. A `pending`/`done`/`failed` row has no patch,
    /// so it has no counts and nothing to disclose.
    var state: EditCard.RowState = .ready
    /// EXP-916: cap the open body and scroll it inside that height — the
    /// contract's `diffUi.inlineDiffMaxHeight`, used by the ONE row an
    /// edited-files card opens while its call runs.
    var maxBodyHeight: CGFloat?
    let onToggle: () -> Void

    @Environment(\.motion) private var motion

    /// Only a row with a patch discloses anything.
    private var openable: Bool { state == .ready }

    var body: some View {
        if flush {
            card
        } else {
            // A gapped list item (one file among many), so it wears the row
            // hairline the borderless group no longer draws.
            card.glassRow()
        }
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: 0) {
            if openable {
                Button(action: onToggle) {
                    header
                }
                .buttonStyle(.plain)
                .accessibilityLabel(expanded ? "Collapse \(file.path)" : "Expand \(file.path)")
                .accessibilityIdentifier("changes-file-row")
            } else {
                header.accessibilityIdentifier("changes-file-row")
            }

            if expanded, openable {
                patchBlock
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The body, capped and scrolled when the host asked for a ceiling.
    @ViewBuilder
    private var patchBlock: some View {
        if let maxBodyHeight {
            ScrollView(.vertical) {
                DiffPatchBlock(file: file, compact: compact)
                    .padding(.bottom, 6)
            }
            .frame(maxHeight: maxBodyHeight)
            // With a short patch there is nothing to scroll here, so the drag
            // belongs to the transcript.
            .scrollBounceBehavior(.basedOnSize)
        } else {
            DiffPatchBlock(file: file, compact: compact)
                .padding(.bottom, 6)
        }
    }

    private var header: some View {
        HStack(spacing: 8) {
            DiffStatusLetter(status: file.status, compact: compact)
                .opacity(state == .failed ? 0 : 1)
            DiffPathLabel(path: file.path, compact: compact, failed: state == .failed)
            // A rename names where it came from, exactly as the web card does.
            if let previous = file.previousPath, !previous.isEmpty, state == .ready {
                Text("\u{2190} \(previous)")
                    .font((compact ? Font.caption2 : Font.caption).monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 8)
            switch state {
            case .ready:
                DiffCountsLabel(
                    additions: file.additions, deletions: file.deletions, compact: compact
                )
                // EXP-706: ONE chevron that turns over, not two glyphs
                // swapping — the rotation reads as the section opening.
                chevron(opacity: TextOpacity.tertiary)
            case .pending:
                // No counts yet: the call is still writing the file.
                chevron(opacity: TextOpacity.quaternary)
            case .done:
                // EXP-916: the call settled and carried no patch (a delete, a
                // move, an edit that changed nothing) — a plain settled row:
                // no counts, and no chevron, because nothing will ever open.
                EmptyView()
            case .failed:
                // EXP-916: the call settled without a patch — the contract's
                // own word for that row, in the danger tint.
                Text("failed")
                    .font((compact ? Font.caption2 : Font.caption))
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, compact ? 7 : 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        // The header band: the SECTION fill every glass band wears, so the
        // header reads as chrome over the patch rather than as another row.
        .background(GlassTokens.fillSection)
        .contentShape(Rectangle())
    }

    private func chevron(opacity: Double) -> some View {
        AppIcon(AppIcons.uiChevronDown, size: 11)
            .foregroundStyle(.white.opacity(opacity))
            .rotationEffect(.degrees(expanded ? 180 : 0))
            .animation(motion.standard, value: expanded)
    }
}

// MARK: - The three atoms (web `diff-counts.tsx`)

/// The status letter a file row leads with. Only the two states that ARE a
/// colour in the diff body carry one: `A` the addition green, `D` the deletion
/// red. `M`/`R`/`C` stay muted — an amber "modified" and a sky "renamed" (the
/// pre-EXP-895 palette) invented two accent hues the token set does not have.
struct DiffStatusLetter: View {
    let status: Diff.Status
    var compact = false

    var body: some View {
        Text(DiffPresentation.statusLetter(status))
            .font((compact ? Font.caption2 : Font.caption).monospaced().weight(.semibold))
            .foregroundStyle(color)
    }

    private var color: Color {
        switch status {
        case .added: DesignTokens.Diff.addFg
        case .removed: DesignTokens.Diff.delFg
        case .modified, .renamed, .copied: .white.opacity(TextOpacity.secondary)
        }
    }
}

/// `+a −b` — the addition green and the deletion red, the U+2212 minus the
/// contract owns (`Diff.additionsLabel` / `deletionsLabel`).
struct DiffCountsLabel: View {
    let additions: Int
    let deletions: Int
    var compact = false

    var body: some View {
        HStack(spacing: 6) {
            Text(Diff.additionsLabel(additions))
                .foregroundStyle(DesignTokens.Diff.addFg)
            Text(Diff.deletionsLabel(deletions))
                .foregroundStyle(DesignTokens.Diff.delFg)
        }
        .font((compact ? Font.caption2 : Font.caption).monospaced())
        .accessibilityElement(children: .combine)
    }
}

/// `apps/web/src/` dimmed, `file.tsx` at full weight — one mono run. EXP-698:
/// on a phone a trailing ellipsis eats the only part of a path that identifies
/// the file, so the DIRECTORY gives way (middle-truncated) and the filename is
/// never cut.
struct DiffPathLabel: View {
    let path: String
    var compact = false
    /// EXP-916: the edit that never landed — the whole path tints danger.
    var failed = false

    var body: some View {
        HStack(spacing: 0) {
            let dir = DiffPresentation.pathDirPrefix(path)
            if !dir.isEmpty {
                Text(dir)
                    .foregroundStyle(
                        failed
                            ? DesignTokens.Semantic.red.opacity(TextOpacity.secondary)
                            : Color.white.opacity(TextOpacity.tertiary)
                    )
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Text(DiffPresentation.pathBase(path))
                .foregroundStyle(failed ? DesignTokens.Semantic.red : Color.white)
                .lineLimit(1)
                .layoutPriority(1)
        }
        .font((compact ? Font.caption2 : Font.caption).monospaced())
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(path)
    }
}
