import ExpCore
import ExpUI
import SwiftUI

/// EXP-1251: the GUIDE face — Changes + Results merged (web
/// `components/guide-face.tsx`, desktop `session_results.rs`, Android
/// `GuideFace.kt`); EXP-1295: a thread's Guide tab over its root's `guide`
/// (THE PR body). Top to bottom: the Summary lead, the numbered sections
/// (band, text, ONE `GuideChangesRow`, tiles, Earlier), the automatic `Other
/// changes` section, then `Show complete diff`. With no report, ONE
/// `Changes` section. Coverage is `guideCoverage` (ExpCore, fixture-locked ×4).
///
/// A Changes row opens that section's diff as a PAGE in place
/// (`GuideSectionDiffView`): a back row to the Guide, the cards filtered to
/// the section and the file sheet filtered the same way. The bar is the white
/// Merge capsule (`merge`).
struct GuideFace<Merge: View>: View {
    let groups: [SessionResultGroup]
    /// The diff the Guide counts (the run's live diff, else the PR / branch
    /// files); nil while it is not loaded or there is none.
    let files: [Diff.File]?
    var diffStatus: GuideDiffStatus = .ready
    /// The publisher's own dropped-line count, on a live run diff.
    var truncatedLines: Int?
    /// EXP-1188: the run's team, so the report chips issue identifiers like
    /// the steer feed does; nil = no chips.
    var teamId: String?
    /// The section page in view; nil = the Guide itself.
    @Binding var section: GuideSectionKey?
    /// The file the section page focuses (its sheet's pick).
    @Binding var focusPath: String?
    /// Whether `merge` draws anything (a generic view cannot say).
    var showsMerge = false
    var merge: () -> Merge

    init(
        groups: [SessionResultGroup],
        files: [Diff.File]?,
        diffStatus: GuideDiffStatus = .ready,
        truncatedLines: Int? = nil,
        teamId: String? = nil,
        section: Binding<GuideSectionKey?>,
        focusPath: Binding<String?>,
        showsMerge: Bool = false,
        @ViewBuilder merge: @escaping () -> Merge
    ) {
        self.groups = groups
        self.files = files
        self.diffStatus = diffStatus
        self.truncatedLines = truncatedLines
        self.teamId = teamId
        _section = section
        _focusPath = focusPath
        self.showsMerge = showsMerge
        self.merge = merge
    }

    var body: some View {
        if let section,
           let page = guideSectionPage(groups, files: files, section: section, numbered: true) {
            GuideSectionDiffView(
                page: page,
                truncatedLines: section == .all ? truncatedLines : nil,
                focusPath: $focusPath,
                onBack: { self.section = nil },
                showsMerge: showsMerge,
                merge: merge
            )
        } else {
            GuideBody(
                groups: groups,
                files: files,
                diffStatus: diffStatus,
                teamId: teamId,
                onOpen: { key in
                    focusPath = nil
                    section = key
                }
            )
            .safeAreaInset(edge: .bottom) {
                if showsMerge {
                    FloatingBarCluster {
                        EmptyView()
                    } center: {
                        merge()
                    } trailing: {
                        EmptyView()
                    }
                    .floatingBarEdge()
                }
            }
        }
    }
}

/// Where the diff the Guide counts stands.
enum GuideDiffStatus: Equatable {
    case loading
    case failed(String)
    case ready
}

/// The Guide's scrolling body (the page `GuideFace` shows with no section
/// open).
private struct GuideBody: View {
    let groups: [SessionResultGroup]
    let files: [Diff.File]?
    let diffStatus: GuideDiffStatus
    let teamId: String?
    let onOpen: (GuideSectionKey) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.motion) private var motion

    /// EXP-1172: the topics whose `Earlier` band is open.
    @State private var expandedEarlier: Set<String> = []
    /// The page's content width: the whole page's tiles scale DOWN by one
    /// factor to fit it (`sessionResultTileHeightFitting`).
    @State private var contentWidth: CGFloat = 0

    private let horizontalPadding: CGFloat = 16

    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            // EXP-1188: the report chips issue identifiers (`#EXP-1` and the
            // bare `EXP-1` agents write) exactly like the steer feed.
            issueRefs: teamId.map { teamId in
                AgentIssueRefContext(
                    teamId: teamId,
                    db: deps.db,
                    onOpen: { [deps, accountId] issueId in
                        deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                    }
                )
            }
        )
    }

    private var tileHeight: CGFloat {
        sessionResultTileHeightFitting(
            sessionResultPictures(groups),
            availableWidth: contentWidth - horizontalPadding * 2
        )
    }

    var body: some View {
        let shown = groups
        let coverage = guideCoverage(shown, files)
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 24) {
                if let lead = coverage.lead {
                    VStack(alignment: .leading, spacing: 0) {
                        groupBody(lead.group, changes: lead.changes, key: .lead)
                    }
                    // `.contain`: a bare identifier on a stack propagates to
                    // every element inside and REPLACES theirs (the Changes
                    // row lost `guide-changes-row`); a container keeps them.
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("guide-lead")
                }
                // By position: one topic can repeat.
                ForEach(Array(coverage.sections.enumerated()), id: \.offset) { _, section in
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(section.group.topic) {
                            Text(guideSectionCaption(section.index, section.total))
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .padding(.trailing, 2)
                                .accessibilityIdentifier("guide-section-caption")
                        } trailing: {
                            EmptyView()
                        }
                        groupBody(section.group, changes: section.changes, key: .section(section.index))
                            .padding(.top, 4)
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("guide-section-\(section.index)")
                }
                if let other = coverage.other {
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(other.topic)
                            .opacity(shown.isEmpty ? 1 : 0.7)
                        GuideChangesRow(changes: other.changes) { onOpen(.other) }
                            .padding(.top, 4)
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("guide-other-changes")
                }
                // No report: the one Changes section already is the whole diff.
                if !shown.isEmpty, let complete = coverage.complete, complete.fileCount > 0 {
                    GuideShowCompleteDiffRow(changes: complete) { onOpen(.all) }
                }
                diffStatusLine
            }
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, 12)
        }
        .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width in
            contentWidth = width
        }
        .accessibilityIdentifier("session-results")
    }

    @ViewBuilder
    private var diffStatusLine: some View {
        switch diffStatus {
        case .loading:
            // EXP-1275 (D8): the file rows' placeholder, not "Loading…".
            SkeletonRows(rows: 3, rowHeight: 36)
                .accessibilityIdentifier("guide-diff-loading")
        case .failed, .ready:
            // FEED-79: the load runs on open, so a failure is quiet: the
            // face keeps what it has, never a toast.
            EmptyView()
        }
    }

    /// One topic under its band (or as the lead): the text, its ONE Changes
    /// row, the tiles, the `Earlier` fold.
    @ViewBuilder
    private func groupBody(
        _ group: SessionResultGroup, changes: GuideChangeSet?, key: GuideSectionKey
    ) -> some View {
        if let text = group.text {
            AgentMarkdownText(
                text: text, context: markdownContext, options: [.autolinkBareURLs],
                overrides: AgentMarkdownText.guideReport
            )
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, 4)
        }
        // A section that matched no file draws no row (desktop parity).
        if let changes, changes.fileCount > 0 {
            GuideChangesRow(changes: changes) { onOpen(key) }
                .padding(.top, group.text == nil ? 4 : 8)
        }
        if !group.entries.isEmpty {
            tiles(group.entries)
                .padding(.top, 12)
        }
        if !group.earlier.isEmpty {
            earlierBand(group)
        }
    }

    private func tiles(_ entries: [SessionResultEntry]) -> some View {
        FlowLayout(spacing: 12) {
            ForEach(entries, id: \.attachmentId) { entry in
                SessionResultPictureTile(entry: entry, height: tileHeight, caption: entry.label)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// EXP-1172: the muted `Earlier · N` disclosure under a group's tiles.
    @ViewBuilder
    private func earlierBand(_ group: SessionResultGroup) -> some View {
        let expanded = expandedEarlier.contains(group.topic)
        Button {
            withAnimation(motion.standard) {
                if expanded {
                    expandedEarlier.remove(group.topic)
                } else {
                    expandedEarlier.insert(group.topic)
                }
            }
        } label: {
            HStack(spacing: 6) {
                AppIcon(expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 11)
                Text("\(sessionResultsEarlierLabel) · \(group.earlier.count)")
                    .font(.caption)
                Spacer(minLength: 0)
            }
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.top, group.entries.isEmpty ? 4 : 8)
        .accessibilityLabel("\(sessionResultsEarlierLabel), \(group.earlier.count)")
        .accessibilityValue(expanded ? "Expanded" : "Collapsed")
        .accessibilityIdentifier("session-results-earlier")
        if expanded {
            tiles(group.earlier)
        }
    }
}

/// EXP-1251: a section's ONE changes row — `<> Changes · N files · +A −D ›`
/// (web `GuideChangesRow`): the `guide-changes` concept, the contract label,
/// the muted file count, the counts, a chevron.
struct GuideChangesRow: View {
    let changes: GuideChangeSet
    let onOpen: () -> Void

    var body: some View {
        Button(action: onOpen) {
            HStack(spacing: 10) {
                AppIcon(AppIcons.guideChanges, size: 15, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(DomainContract.diffUiGuideChangesRow)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                Spacer(minLength: 8)
                Text(guideFileCountLabel(changes.fileCount))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                DiffCountsLabel(additions: changes.additions, deletions: changes.deletions)
                AppIcon(AppIcons.uiChevronRight, size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 4)
            .frame(minHeight: 40)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(
            "\(DomainContract.diffUiGuideChangesRow), \(guideFileCountLabel(changes.fileCount))"
        )
        .accessibilityIdentifier("guide-changes-row")
    }
}

/// EXP-1251: the final hairline row that opens the complete diff.
struct GuideShowCompleteDiffRow: View {
    let changes: GuideChangeSet
    let onOpen: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            GlassDivider()
            Button(action: onOpen) {
                HStack(spacing: 10) {
                    Text(DomainContract.diffUiGuideShowCompleteDiff)
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Spacer(minLength: 8)
                    Text(guideFileCountLabel(changes.fileCount))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    DiffCountsLabel(additions: changes.additions, deletions: changes.deletions)
                    AppIcon(AppIcons.uiChevronRight, size: 12)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .padding(.horizontal, 4)
                .frame(minHeight: 44)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("guide-show-complete-diff")
        }
    }
}

/// EXP-1251: a Guide section's changes as a PAGE (web
/// `guide-section-diff.tsx` on phones): the back row `‹ Guide · 02 / 06 ·
/// title · +A −D`, the diff cards filtered to the section, and the bar's
/// file sheet filtered the same way beside the Merge capsule.
struct GuideSectionDiffView<Merge: View>: View {
    let page: GuideSectionPage
    var truncatedLines: Int?
    @Binding var focusPath: String?
    let onBack: () -> Void
    var showsMerge = false
    @ViewBuilder var merge: () -> Merge

    @State private var fileSheet = false

    var body: some View {
        DiffFileList(
            files: page.files,
            truncatedLines: truncatedLines,
            emptyLabel: .changedFiles,
            focusPath: focusPath,
            accessibilityId: "guide-section-cards",
            headerFade: false,
            header: { EmptyView() }
        )
        .safeAreaInset(edge: .top, spacing: 0) { backRow }
        .safeAreaInset(edge: .bottom) { bar }
        .sheet(isPresented: $fileSheet) {
            DiffFileListSheet(files: page.files, selected: focusPath, onSelect: { focusPath = $0 })
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("guide-section-page")
    }

    private var backRow: some View {
        HStack(spacing: 8) {
            Button(action: onBack) {
                HStack(spacing: 4) {
                    AppIcon(AppIcons.uiChevronLeft, size: 14, weight: .medium)
                    Text(SessionTree.ThreadCopy.guide)
                        .font(.subheadline.weight(.medium))
                }
                .foregroundStyle(.white)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("guide-section-back")
            Rectangle()
                .fill(GlassTokens.strokeCard)
                .frame(width: GlassTokens.hairline, height: 14)
            if let caption = page.caption {
                Text(caption)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Text(page.title)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 4)
            DiffCountsLabel(additions: page.additions, deletions: page.deletions)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(guideSectionSummary(page))
    }

    @ViewBuilder
    private var bar: some View {
        if !page.files.isEmpty || showsMerge {
            FloatingBarCluster {
                if !page.files.isEmpty {
                    DiffFilesBarCircle(count: page.files.count) { fileSheet = true }
                }
            } center: {
                merge()
            } trailing: {
                EmptyView()
            }
            .floatingBarEdge()
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("changes-files-bar")
        }
    }
}
