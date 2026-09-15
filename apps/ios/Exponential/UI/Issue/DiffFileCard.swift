import ExpCore
import ExpUI
import SwiftUI

/// EXP-895 — ONE file's diff, ×4 (web `FileDiffCard`): a tappable
/// `letter · dimmed dir/basename · +a −b · chevron` header over the collapsible
/// unified body. It takes a `Diff.File` off the shared parser and nothing
/// else — no `PrFile`, no patch string — so the review page, the run's Changes
/// face, an issue's Changes face and one tool call's patch all draw the SAME
/// card, differing only in `compact` and whether it starts open.
struct DiffFileCard: View {
    let file: Diff.File
    let expanded: Bool
    /// A transcript's tool card: tighter type, no old-side gutter.
    var compact = false
    let onToggle: () -> Void

    @Environment(\.motion) private var motion

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button(action: onToggle) {
                header
            }
            .buttonStyle(.plain)
            .accessibilityLabel(expanded ? "Collapse \(file.path)" : "Expand \(file.path)")
            .accessibilityIdentifier("changes-file-row")

            if expanded {
                DiffPatchBlock(file: file, compact: compact)
                    .padding(.bottom, 6)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        // A gapped list item (one file among many), so it wears the row
        // hairline the borderless group no longer draws.
        .glassRow()
    }

    private var header: some View {
        HStack(spacing: 8) {
            DiffStatusLetter(status: file.status, compact: compact)
            DiffPathLabel(path: file.path, compact: compact)
            // A rename names where it came from, exactly as the web card does.
            if let previous = file.previousPath, !previous.isEmpty {
                Text("\u{2190} \(previous)")
                    .font((compact ? Font.caption2 : Font.caption).monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 8)
            DiffCountsLabel(
                additions: file.additions, deletions: file.deletions, compact: compact
            )
            // EXP-706: ONE chevron that turns over, not two glyphs swapping —
            // the rotation reads as the section opening.
            AppIcon(AppIcons.uiChevronDown, size: 11)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .rotationEffect(.degrees(expanded ? 180 : 0))
                .animation(motion.standard, value: expanded)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, compact ? 7 : 10)
        .contentShape(Rectangle())
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

    var body: some View {
        HStack(spacing: 0) {
            let dir = DiffPresentation.pathDirPrefix(path)
            if !dir.isEmpty {
                Text(dir)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Text(DiffPresentation.pathBase(path))
                .foregroundStyle(.white)
                .lineLimit(1)
                .layoutPriority(1)
        }
        .font((compact ? Font.caption2 : Font.caption).monospaced())
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(path)
    }
}
