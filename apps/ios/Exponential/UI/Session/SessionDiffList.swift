import ExpCore
import ExpUI
import SwiftUI

/// EXP-895: the summary line every Changes surface prints — `Changes +82 −6 ·
/// 3 files`, the U+2212 minus the contract owns (`Diff.additionsLabel` /
/// `deletionsLabel` / `summaryLabel`), web's `ChangesTopBar` reading order.
struct DiffSummaryRow: View {
    let totals: Diff.Totals
    /// The word in front of the counts. The phone file sheet's own header
    /// already says what it is, so it drops it.
    var title: String?

    var body: some View {
        HStack(spacing: 8) {
            if let title {
                Text(title)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
            }
            DiffCountsLabel(additions: totals.additions, deletions: totals.deletions)
            Text(totals.files == 1 ? "1 file" : "\(totals.files) files")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            Diff.summaryLabel(
                files: totals.files, additions: totals.additions, deletions: totals.deletions
            )
        )
    }
}

/// EXP-895: the ONE diff view as a page — the summary over `DiffFileCard`s, the
/// publisher's cut note under them. `DiffFileList` is what the run's Changes
/// face (the live worktree diff) and the issue's PR files both draw; the file
/// column that sits beside it on a desktop is a bottom SHEET here
/// (`DiffFileListSheet`), so this only reacts to the path it picked.
struct DiffFileList<Header: View>: View {
    let files: [Diff.File]
    /// The publisher's OWN dropped-line count (`Diff.Parsed.truncatedLines`).
    var truncatedLines: Int?
    /// Every card starts closed rather than open. EXP-916: nothing asks for
    /// this any more — every Changes surface opens its cards, and only the
    /// SIZE rule (`DiffPresentation.diffOpensByDefault`) folds one away.
    var defaultCollapsed = false
    /// What an EMPTY file set says.
    var emptyLabel: String?
    /// The file the reader picked in the sheet: expand it and scroll to it. A
    /// CHANGE jumps; re-picking the file in view is a no-op.
    var focusPath: String?
    /// What the surface answers to in a UI test.
    var accessibilityId = "session-diff-list"
    /// Anything the surface puts above the cards (the PR/branch header).
    @ViewBuilder var header: () -> Header

    /// Sparse reader overrides on top of the default, keyed by path — a refresh
    /// replaces `files` without discarding the toggles.
    @State private var overrides: [String: Bool] = [:]

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    header()
                    if files.isEmpty, let emptyLabel {
                        Text(emptyLabel)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(.vertical, 12)
                    }
                    ForEach(files) { file in
                        DiffFileCard(
                            file: file,
                            expanded: isExpanded(file),
                            onToggle: { overrides[file.path] = !isExpanded(file) }
                        )
                        .id(file.path)
                    }
                    if let truncatedLines, truncatedLines > 0 {
                        Text(AgentFeed.diffTruncationNote(truncatedLines))
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(.horizontal, 4)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 4)
                .padding(.bottom, 24)
            }
            .stickyHeaderFade()
            .onChange(of: focusPath) { _, path in
                guard let path, !path.isEmpty else { return }
                overrides[path] = true
                withAnimation { proxy.scrollTo(path, anchor: .top) }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(accessibilityId)
    }

    /// EXP-916: a file opens by ITSELF unless it is huge — the ONE size rule,
    /// shared ×4. A reader's own tap still wins over it.
    private func isExpanded(_ file: Diff.File) -> Bool {
        overrides[file.path]
            ?? DiffPresentation.diffOpensByDefault(file, defaultCollapsed: defaultCollapsed)
    }
}

extension DiffFileList where Header == EmptyView {
    /// The headerless list — cards and nothing above them.
    init(
        files: [Diff.File],
        truncatedLines: Int? = nil,
        defaultCollapsed: Bool = false,
        emptyLabel: String? = nil,
        focusPath: String? = nil,
        accessibilityId: String = "session-diff-list"
    ) {
        self.init(
            files: files,
            truncatedLines: truncatedLines,
            defaultCollapsed: defaultCollapsed,
            emptyLabel: emptyLabel,
            focusPath: focusPath,
            accessibilityId: accessibilityId,
            header: { EmptyView() }
        )
    }
}

/// The run's latest worktree diff as a full page — the Work screen's Changes
/// face while the run has one. The raw `git diff` goes through the ONE parser
/// (`Diff.parse`) and comes out as the same cards every other Changes surface
/// draws.
struct SessionDiffList: View {
    let files: [Diff.File]
    var truncatedLines: Int?
    var focusPath: String?

    var body: some View {
        DiffFileList(
            files: files,
            truncatedLines: truncatedLines,
            emptyLabel: "No changed files.",
            focusPath: focusPath,
            header: {
                DiffSummaryRow(
                    totals: Diff.totals(files), title: DiffPresentation.changesTitle
                )
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .glassCard()
            }
        )
    }
}
