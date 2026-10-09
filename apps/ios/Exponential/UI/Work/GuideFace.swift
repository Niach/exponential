import ExpCore
import ExpUI
import SwiftUI

/// EXP-1251: the Work screen's GUIDE face — Changes + Results merged (web
/// `components/guide-face.tsx`, desktop `session_results.rs`, Android
/// `GuideFace.kt`). Top to bottom: the Stack card (the shared `StackRail`
/// when the PR sits in a linear open stack of 2+), the Summary lead, the
/// numbered sections (band, text, ONE `GuideChangesRow`, tiles, Earlier), the
/// automatic `Other changes` section, then `Show complete diff`. With no
/// report an open PR's GitHub body is ONE unnumbered section claiming the
/// whole diff (web M8, `prDescriptionGroup`); with neither, ONE `Changes`
/// section. Coverage is `guideCoverage` (ExpCore, fixture-locked ×4).
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
    /// With no report: the open PR's GitHub body (or its load state).
    var prFallback: GuidePrFallback?
    /// The rail of the PR's open stack (`PrStack.stackView`), nil = none.
    var stack: PrStack.StackView?
    /// The board's own default branch: the Stack card's base row when the
    /// bottom member's base is not synced (Reviews' fallback).
    var defaultBranch: String?
    /// A stack member tap: the Work screen swaps to that issue in place.
    var onOpenStackMember: ((String) -> Void)?
    /// A stack member's long-press "Merge through here".
    var onMergeThrough: ((String) -> Void)?
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
        prFallback: GuidePrFallback? = nil,
        stack: PrStack.StackView? = nil,
        defaultBranch: String? = nil,
        onOpenStackMember: ((String) -> Void)? = nil,
        onMergeThrough: ((String) -> Void)? = nil,
        section: Binding<GuideSectionKey?>,
        focusPath: Binding<String?>,
        showsMerge: Bool = false,
        @ViewBuilder merge: @escaping () -> Merge
    ) {
        self.groups = groups
        self.files = files
        self.diffStatus = diffStatus
        self.truncatedLines = truncatedLines
        self.prFallback = prFallback
        self.stack = stack
        self.defaultBranch = defaultBranch
        self.onOpenStackMember = onOpenStackMember
        self.onMergeThrough = onMergeThrough
        _section = section
        _focusPath = focusPath
        self.showsMerge = showsMerge
        self.merge = merge
    }

    /// M8 ×4: with no report, the loaded PR body is ONE unnumbered group
    /// that claims every diff path (`prDescriptionGroup`), so its band holds
    /// the one Changes row and `Other changes` never shows.
    private var prGroup: SessionResultGroup? {
        guard groups.isEmpty, case let .loaded(description) = prFallback else { return nil }
        return prDescriptionGroup(title: description.title, body: description.body, files: files)
    }

    var body: some View {
        let prGroup = prGroup
        let shown = prGroup.map { [$0] } ?? groups
        if let section, let page = WorkFaces.guideSectionPage(shown, files: files, section: section) {
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
                groups: shown,
                files: files,
                diffStatus: diffStatus,
                prFallback: prFallback,
                prBody: prGroup != nil,
                stack: stack,
                defaultBranch: defaultBranch,
                onOpenStackMember: onOpenStackMember,
                onMergeThrough: onMergeThrough,
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

/// The open PR's GitHub body standing in for a report.
enum GuidePrFallback: Equatable {
    case loading
    case failed(String)
    case loaded(PrDescription)
}

/// The Guide's scrolling body (the page `GuideFace` shows with no section
/// open).
private struct GuideBody: View {
    let groups: [SessionResultGroup]
    let files: [Diff.File]?
    let diffStatus: GuideDiffStatus
    let prFallback: GuidePrFallback?
    /// `groups` is the loaded PR body standing in for a missing report.
    let prBody: Bool
    let stack: PrStack.StackView?
    let defaultBranch: String?
    let onOpenStackMember: ((String) -> Void)?
    let onMergeThrough: ((String) -> Void)?
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
            httpClient: deps.httpClient
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
                if let stack {
                    GuideStackCard(
                        stack: stack,
                        defaultBranch: defaultBranch,
                        onOpen: onOpenStackMember,
                        onMergeThrough: onMergeThrough
                    )
                }
                if groups.isEmpty, let prFallback {
                    fallback(prFallback)
                }
                if let lead = coverage.lead {
                    VStack(alignment: .leading, spacing: 0) {
                        groupBody(lead.group, changes: lead.changes, key: .lead)
                    }
                    .accessibilityIdentifier("guide-lead")
                }
                // By position: one topic can repeat.
                ForEach(Array(coverage.sections.enumerated()), id: \.offset) { _, section in
                    VStack(alignment: .leading, spacing: 0) {
                        if prBody {
                            // The PR body: one UNNUMBERED section.
                            GlassSectionBand(section.group.topic)
                        } else {
                            GlassSectionBand(section.group.topic) {
                                Text(guideSectionCaption(section.index, section.total))
                                    .font(.caption.monospacedDigit())
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .padding(.trailing, 2)
                                    .accessibilityIdentifier("guide-section-caption")
                            } trailing: {
                                EmptyView()
                            }
                        }
                        groupBody(section.group, changes: section.changes, key: .section(section.index))
                            .padding(.top, 4)
                    }
                    .accessibilityIdentifier(
                        prBody ? "session-results-pr-body" : "guide-section-\(section.index)"
                    )
                }
                if let other = coverage.other {
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(other.topic)
                            .opacity(shown.isEmpty ? 1 : 0.7)
                        GuideChangesRow(changes: other.changes) { onOpen(.other) }
                            .padding(.top, 4)
                    }
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
            HStack(spacing: 8) {
                ProgressView().controlSize(.small).tint(.white)
                Text("Loading changes…")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .accessibilityIdentifier("guide-diff-loading")
        case let .failed(message):
            Text("Couldn't load changes: \(message)")
                .font(.caption)
                .foregroundStyle(DesignTokens.Semantic.red)
                .accessibilityIdentifier("guide-diff-error")
        case .ready:
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
            AgentMarkdownText(text: text, context: markdownContext)
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

    @ViewBuilder
    private func fallback(_ state: GuidePrFallback) -> some View {
        switch state {
        case .loaded:
            // Rendered as the PR-body group (`prDescriptionGroup`).
            EmptyView()
        case let .failed(message):
            Text(message)
                .font(.subheadline)
                .foregroundStyle(DesignTokens.Palette.destructive)
                .accessibilityIdentifier("results-pr-fallback-error")
        case .loading:
            HStack(spacing: 8) {
                ProgressView().tint(.white).controlSize(.small)
                Text("Loading the pull request…")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .accessibilityIdentifier("results-pr-fallback-loading")
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

/// EXP-1248: the Guide's Stack card — a `Stack` band (the `pr-stack` concept,
/// NO count) over the shared rail, top-first down to the base branch. A
/// member tap swaps the screen to that issue in place; a long-press offers
/// "Merge through here".
struct GuideStackCard: View {
    static let title = "Stack"

    let stack: PrStack.StackView
    /// The board's default branch, for a bottom member with no synced base.
    var defaultBranch: String?
    var onOpen: ((String) -> Void)?
    var onMergeThrough: ((String) -> Void)?

    /// The base row: the bottom member's synced base, else the board's
    /// default branch, else the literal `default branch` (Reviews ×4).
    static func baseRowTitle(_ baseBranch: String?, defaultBranch: String?) -> String {
        for candidate in [baseBranch, defaultBranch] {
            if let value = candidate?.trimmingCharacters(in: .whitespacesAndNewlines), !value.isEmpty {
                return value
            }
        }
        return "default branch"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            GlassSectionBand(Self.title) {
                AppIcon(AppIcons.prStack, size: 14, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            } trailing: {
                EmptyView()
            }
            StackRail(
                members: stack.rows.map {
                    StackRailMember(
                        id: $0.issueId, identifier: $0.identifier, title: $0.title, current: $0.isCurrent
                    )
                },
                baseBranch: Self.baseRowTitle(stack.baseBranch, defaultBranch: defaultBranch),
                onOpen: onOpen.map { open in { member in open(member.id) } },
                onMergeThrough: onMergeThrough.map { merge in { member in merge(member.id) } }
            )
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("guide-stack-card")
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
            emptyLabel: "No changed files.",
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
                    Text(WorkFaces.guideFaceLabel)
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
        .accessibilityLabel(WorkFaces.guideSectionSummary(page))
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

/// EXP-893/1251: a shown run's retained steer model, claimed while this view
/// is up — the Guide reads the run's LIVE worktree diff off it. Its own
/// ref-counted claim (`attachSteerModel`), so a finished run's model survives
/// whichever page the pager unmounts first.
struct RunModelClaim<Content: View>: View {
    let accountId: String
    let session: CodingSessionEntity
    @ViewBuilder let content: (AgentSessionModel?) -> Content

    @Environment(AppDependencies.self) private var deps
    @State private var model: AgentSessionModel?
    /// ONE claim: the pager can fire `onAppear` / `onDisappear` unpaired.
    @State private var claimed = false

    var body: some View {
        content(model)
            .onAppear {
                guard !claimed else { return }
                claimed = true
                model = deps.attachSteerModel(accountId: accountId, session: session)
            }
            .onDisappear {
                guard claimed else { return }
                claimed = false
                deps.steerSessions.detach(accountId: accountId, sessionId: session.id)
            }
    }
}
