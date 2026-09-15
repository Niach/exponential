import ExpCore
import ExpUI
import SwiftUI

/// EXP-895 — the phone's file list (web `ChangesFileSheet` + `FileDiffNav`). A
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

    private var summary: String {
        let sum = Diff.totals(files)
        return Diff.summaryLabel(
            files: sum.files, additions: sum.additions, deletions: sum.deletions
        )
    }

    var body: some View {
        GlassSheetChrome(
            title: DiffPresentation.changedFilesTitle,
            height: .full,
            pinnedHeader: {
                VStack(spacing: 8) {
                    // The summary counts the WHOLE diff, never the filtered
                    // slice: it is the review's summary line, not a search
                    // result count. ONE contract string (web `FileDiffNav`
                    // heads its list with exactly this).
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
                    VStack(alignment: .leading, spacing: 0) {
                        let shown = DiffPresentation.filter(files, query: filter)
                        if shown.isEmpty {
                            Text(files.isEmpty ? "No changed files." : "No matching files.")
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .padding(.horizontal, GlassSheetTokens.headerHPadding)
                                .padding(.vertical, 12)
                        }
                        ForEach(Array(shown.enumerated()), id: \.element.path) { index, file in
                            if index > 0 { GlassDivider() }
                            row(file)
                        }
                    }
                    .padding(.horizontal, 8)
                    .padding(.bottom, 16)
                }
            }
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("changes-file-list")
    }

    /// `letter · name · dimmed dir · counts` — the basename leads, the
    /// directory trails it dimmed (web `FileDiffNav`).
    private func row(_ file: Diff.File) -> some View {
        Button {
            dismiss()
            onSelect(file.path)
        } label: {
            HStack(spacing: 8) {
                DiffStatusLetter(status: file.status)
                Text(DiffPresentation.pathBase(file.path))
                    .font(.caption.monospaced())
                    .foregroundStyle(.white)
                    .lineLimit(1)
                let dir = DiffPresentation.pathDir(file.path)
                if !dir.isEmpty {
                    Text(dir)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: 8)
                DiffCountsLabel(additions: file.additions, deletions: file.deletions)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .background(file.path == selected ? GlassTokens.fillActive : .clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("changes-file-list-row")
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
            VStack(spacing: 2) {
                AppIcon(AppIcons.navFiles, size: AppIcon.Size.medium, weight: .medium)
                Text("\(count)")
                    .font(.caption2.weight(.medium))
                    .monospacedDigit()
            }
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
        .accessibilityIdentifier("changes-file-list-button")
    }
}
