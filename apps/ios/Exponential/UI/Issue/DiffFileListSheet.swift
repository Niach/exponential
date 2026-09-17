import ExpCore
import ExpUI
import SwiftUI

/// EXP-895 — the phone's file list (web `ChangesFileSheet` + `@exp/ui`
/// `file-diff-tree.tsx`). A
/// column beside the cards leaves neither readable on a phone, so the list
/// lives in a bottom SHEET off the work bar's LEADING slot — the slot GitHub
/// used to hold on the Changes face (GitHub moved to the header's action slot).
///
/// A pick closes the sheet and reports the path; the card list expands that
/// file and scrolls to it.
struct DiffFileListSheet: View {
    let files: [Diff.File]
    var selected: String?
    let onSelect: (String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var filter = ""
    /// EXP-916: the totals are summed once per FILE SET, not once per body —
    /// every keystroke in the filter re-evaluates this view.
    @State private var summaryMemo = DiffSummaryMemo()

    private var summary: String { summaryMemo.summary(files) }

    var body: some View {
        GlassSheetChrome(
            title: DiffPresentation.changedFilesTitle,
            height: .full,
            pinnedHeader: {
                VStack(spacing: 8) {
                    // The summary counts the WHOLE diff, never the filtered
                    // slice: it is the review's summary line, not a search
                    // result count. ONE contract string (web
                    // `file-diff-tree.tsx` heads its list with exactly this).
                    Text(summary)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .glassRow()
                    GlassSheetSearchField(
                        placeholder: DiffPresentation.filterPlaceholder,
                        text: $filter,
                        accessibilityIdentifier: "changes-file-filter"
                    )
                }
                .padding(.horizontal, GlassSheetTokens.headerHPadding)
                .padding(.bottom, 8)
            },
            content: {
                ScrollView {
                    // EXP-916: the TREE, not a flat list — the same nesting
                    // the sidebar draws on web and the desktop.
                    DiffFileTree(
                        files: files,
                        query: filter,
                        selected: selected,
                        onSelect: { path in
                            dismiss()
                            onSelect(path)
                        }
                    )
                    .padding(.horizontal, 8)
                    .padding(.bottom, 16)
                }
            }
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("changes-file-list")
    }
}

/// EXP-916 — the summary cache (the same idea as `DiffTreeMemo`): `Diff.totals`
/// walks every file, and the sheet's body runs on every keystroke in its
/// filter. Keyed on the files' identity, so only a refreshed diff re-sums.
///
/// Deliberately NOT `@Observable`: the cache is written DURING a body pass.
@MainActor
final class DiffSummaryMemo {
    private var key: Int?
    private var cached = ""

    func summary(_ files: [Diff.File]) -> String {
        let key = diffFilesKey(files)
        if key == self.key { return cached }
        let sum = Diff.totals(files)
        let made = Diff.summaryLabel(
            files: sum.files, additions: sum.additions, deletions: sum.deletions
        )
        self.key = key
        cached = made
        return made
    }
}

/// The bar circle the sheet opens from: the files concept glyph over the file
/// COUNT, in the shared 52pt chrome (web's `changes-file-sheet-button`).
struct DiffFilesBarCircle: View {
    let count: Int
    let action: () -> Void

    var body: some View {
        FloatingBarCircle(
            accessibilityLabel: DiffPresentation.changedFilesTitle,
            action: action
        ) {
            // EXP-916: Android's `FileListCircle` — an 18pt white glyph over
            // the count in the secondary emphasis.
            VStack(spacing: 2) {
                AppIcon(AppIcons.navFiles, size: 18, weight: .medium)
                    .foregroundStyle(.white)
                Text("\(count)")
                    .font(.caption2.weight(.medium))
                    .monospacedDigit()
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
        .accessibilityIdentifier("changes-file-list-button")
    }
}
