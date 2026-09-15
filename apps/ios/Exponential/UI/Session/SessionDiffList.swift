import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the diff summary line every changes surface prints — `3 files
/// +12 -4`, ASCII hyphen, two spaces between the count and the stats — the
/// Work screen's Changes face and the Reviews page alike (web / desktop /
/// Android parity).
enum DiffSummary {
    static func label(files: Int, additions: Int, deletions: Int) -> String {
        "\(files) \(files == 1 ? "file" : "files")  +\(additions) -\(deletions)"
    }
}

/// The summary row: the count in secondary, the stats in mono green / red.
struct DiffSummaryRow: View {
    let files: Int
    let additions: Int
    let deletions: Int

    var body: some View {
        HStack(spacing: 8) {
            Text("\(files) \(files == 1 ? "file" : "files")")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("+\(additions)")
                .font(.caption.monospaced())
                .foregroundStyle(.green)
            Text("-\(deletions)")
                .font(.caption.monospaced())
                .foregroundStyle(.red)
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(DiffSummary.label(files: files, additions: additions, deletions: deletions))
    }
}

/// The run's latest worktree diff as a full page (EXP-893; the pinned
/// "Latest changes" sheet until then): the raw `git diff` output split on
/// `diff --git` into per-file glass rows with the shared DiffRendering
/// coloring — horizontal panning stays inside each file's code block only.
/// The Work screen's Changes face while the run has a live diff.
struct SessionDiffList: View {
    let diff: String

    var body: some View {
        let stats = DiffRendering.stats(of: diff)
        let sections = DiffRendering.splitFiles(diff)
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 10) {
                DiffSummaryRow(
                    files: sections.count,
                    additions: stats.additions,
                    deletions: stats.deletions
                )
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .glassCard()
                ForEach(sections) { section in
                    VStack(alignment: .leading, spacing: 0) {
                        if let filename = section.filename {
                            Text(filename)
                                .font(.caption.monospaced())
                                .foregroundStyle(.white)
                                .lineLimit(1)
                                .truncationMode(.middle)
                                .padding(.horizontal, 12)
                                .padding(.vertical, 8)
                        }
                        DiffPatchBlock(patch: section.patch)
                            .padding(.horizontal, 8)
                            .padding(.top, section.filename == nil ? 8 : 0)
                            .padding(.bottom, 8)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    // One file among many in a gapped stack — a row, not a
                    // borderless group.
                    .glassRow()
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 4)
            .padding(.bottom, 24)
        }
        .stickyHeaderFade()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("session-diff-list")
    }
}
