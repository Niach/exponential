import ExpCore
import ExpUI
import QuickLook
import SwiftUI
import UIKit

/// EXP-879: the Work screen's RESULTS face — the screenshots the shown run
/// published with `exponential_sessions_results`, read off its synced
/// `coding_sessions.results` blob. EXP-933: the run's REPORT — each topic's
/// GFM text renders above its tiles.
///
/// EXP-1154: the report reads as the GUIDE (`sessionResultsGuide`, web
/// `SessionResultsView`): the `Summary` topic is the lead paragraph with no
/// band and no number; every other topic wears the shared `GlassSectionBand`
/// with the muted `01 / 03` caption in its leading slot, then its text, the
/// FILES it touched (flat hairline rows, the diff path + `+N −M` once the
/// path matches a loaded diff file; a tap opens the Changes face on it), then
/// a WRAPPING ROW of equal-height tiles, each captioned with its label. With
/// no report yet, an open PR's GitHub body stands in (`prFallback`) as ONE
/// band labelled with the PR title.
///
/// A tile tap opens the platform preview (Quick Look) over the same
/// download-to-temp path the comment strips use; a TALL picture (EXP-1128, a
/// full-page capture) opens `TallImageViewerSheet` instead, a width-fit
/// vertical scroll.
///
/// EXP-1172: a group's folded `earlier` pictures (the `sessions_show` shots
/// filed while the run worked) sit under its tiles behind a collapsed
/// `Earlier · N` row that expands in place to the same tiles.
///
/// The pure rules — parse, group, guide, tile size — are
/// `ExpCore/SessionResults`, mirrored by web `session-results.ts`, desktop
/// `session_results.rs` and Android `domain/SessionResults.kt`.
struct SessionResultsFace: View {
    let groups: [SessionResultGroup]
    /// EXP-1154: the Changes face's loaded files — what a file row's counts
    /// resolve against. nil = no diff loaded (rows draw the path alone).
    var files: [Diff.File]? = nil
    /// EXP-1154: a file row's tap — the Changes face with that file
    /// selected. nil = the rows are plain (no Changes face to open).
    var onOpenFile: ((String) -> Void)? = nil
    /// EXP-1154: the open PR's GitHub title + body, drawn when `groups` is
    /// empty.
    var prFallback: PrDescription? = nil

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.motion) private var motion

    /// EXP-1172: the topics whose `Earlier` band is open — collapsed by
    /// default.
    @State private var expandedEarlier: Set<String> = []
    /// The page's content width, measured once — a phone is narrower than the
    /// pinned 320pt tile is wide for anything landscape, so the whole page
    /// scales DOWN by one factor. Every tile keeps its probed aspect and every
    /// tile stays the same height, which is the point of the strip.
    @State private var contentWidth: CGFloat = 0

    private var horizontalPadding: CGFloat { 16 }

    /// Report text resolves its inline `/api/attachments/{id}` images through
    /// the same member-gated loader the tiles use.
    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient
        )
    }

    /// The widest tile at the pinned height decides the page's scale.
    private var tileHeight: CGFloat {
        sessionResultTileHeightFitting(
            sessionResultPictures(groups),
            availableWidth: contentWidth - horizontalPadding * 2
        )
    }

    private var guide: SessionResultsGuide { sessionResultsGuide(groups) }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 24) {
                if groups.isEmpty, let prFallback {
                    fallback(prFallback)
                } else {
                    let guide = guide
                    if let lead = guide.lead {
                        VStack(alignment: .leading, spacing: 0) {
                            groupBody(lead)
                        }
                        .accessibilityIdentifier("guide-lead")
                    }
                    // By position: one topic can repeat.
                    ForEach(Array(guide.sections.enumerated()), id: \.offset) { _, section in
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
                            groupBody(section.group)
                                .padding(.top, 4)
                        }
                    }
                }
            }
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, 12)
        }
        .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width in
            contentWidth = width
        }
        .accessibilityIdentifier("session-results")
    }

    /// One topic under its band (or as the lead): the text, the files it
    /// touched, the tiles, the `Earlier` fold.
    @ViewBuilder
    private func groupBody(_ group: SessionResultGroup) -> some View {
        // EXP-933: the topic's report text sits ABOVE its tiles; a
        // text-only topic is header + text.
        if let text = group.text {
            AgentMarkdownText(text: text, context: markdownContext)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, 4)
        }
        if !group.files.isEmpty {
            fileList(group.files)
                .padding(.top, group.text == nil ? 4 : 12)
        }
        if !group.entries.isEmpty {
            tiles(group.entries)
                .padding(.top, group.text == nil && group.files.isEmpty ? 4 : 12)
        }
        if !group.earlier.isEmpty {
            earlierBand(group)
        }
    }

    /// EXP-1154: the files a topic touched — flat rows between hairlines,
    /// the path and (when the loaded diff knows it) its `+N −M`.
    private func fileList(_ paths: [String]) -> some View {
        VStack(spacing: 0) {
            GlassDivider()
            ForEach(guideFileRows(paths, files: files)) { row in
                fileRow(row)
                GlassDivider()
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("session-results-files")
    }

    @ViewBuilder
    private func fileRow(_ row: GuideFileRow) -> some View {
        let label = HStack(spacing: 12) {
            DiffPathLabel(path: row.path)
            Spacer(minLength: 0)
            if let additions = row.additions, let deletions = row.deletions {
                DiffCountsLabel(additions: additions, deletions: deletions)
            }
        }
        .padding(.horizontal, 4)
        .frame(minHeight: 36)
        .contentShape(Rectangle())
        if let onOpenFile {
            Button { onOpenFile(row.path) } label: { label }
                .buttonStyle(.plain)
                .flatRow()
                .accessibilityLabel(row.path)
                .accessibilityIdentifier("session-results-file-row")
        } else {
            label
                .flatRow()
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("session-results-file-row")
        }
    }

    /// EXP-1154: an open PR with no run report — its GitHub body as one band
    /// labelled with the PR title.
    private func fallback(_ description: PrDescription) -> some View {
        let title = description.title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let body = description.body?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(title.isEmpty ? "Pull request" : title)
            if body.isEmpty {
                Text("No description.")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.top, 4)
            } else {
                AgentMarkdownText(text: body, context: markdownContext)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.top, 4)
            }
        }
        .accessibilityIdentifier("session-results-pr-body")
    }

    private func tiles(_ entries: [SessionResultEntry]) -> some View {
        FlowLayout(spacing: 12) {
            ForEach(entries, id: \.attachmentId) { entry in
                SessionResultPictureTile(
                    entry: entry,
                    height: tileHeight,
                    caption: entry.label
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// EXP-1172: the muted `Earlier · N` disclosure under a group's tiles —
    /// the transcript's chevron idiom — opening in place to the same tiles.
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

/// EXP-1172: the `sessions_show` picture an Exponential tool row renders
/// under itself — ONE Results tile at `sessionInlineTileHeight`, scaled down
/// to the transcript column by the Results face's own fitting rule, captioned
/// with the call's caption (else the label).
struct SessionInlinePicture: View {
    let entry: SessionResultEntry

    @State private var columnWidth: CGFloat = 0

    private var height: CGFloat {
        sessionResultTileHeightFitting(
            [entry], availableWidth: columnWidth, base: sessionInlineTileHeight
        )
    }

    var body: some View {
        SessionResultPictureTile(entry: entry, height: height, caption: entry.tileCaption)
            .frame(maxWidth: .infinity, alignment: .leading)
            .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width in
                columnWidth = width
            }
            .accessibilityIdentifier("exp-tool-inline-picture")
    }
}

/// One published picture as a tappable tile with its caption line under it —
/// the Results face's tile (EXP-879) and, at the inline height, the
/// transcript's `sessions_show` tile (EXP-1172). A tap opens Quick Look over
/// the download-to-temp path the comment strips use; a TALL picture
/// (EXP-1128) opens `TallImageViewerSheet` instead.
struct SessionResultPictureTile: View {
    let entry: SessionResultEntry
    let height: CGFloat
    let caption: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var previewURL: URL?
    @State private var downloading = false
    /// EXP-1128: the tall picture the scroll viewer shows, nil when closed.
    @State private var tallPreview: TallPreview?

    var body: some View {
        let width = sessionResultTileWidth(entry, height: height)
        VStack(alignment: .leading, spacing: 4) {
            Button {
                preview(entry)
            } label: {
                SessionResultTile(
                    entry: entry,
                    width: width,
                    height: height,
                    baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                    accountId: accountId,
                    httpClient: deps.httpClient,
                    isLoading: downloading
                )
            }
            .buttonStyle(.plain)
            Text(caption)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(width: width, alignment: .leading)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(
            sessionResultIsTall(entry)
                ? "\(entry.topic), \(caption), tall"
                : "\(entry.topic), \(caption)"
        )
        .quickLookPreview($previewURL)
        .sheet(item: $tallPreview) { preview in
            TallImageViewerSheet(entry: preview.entry)
        }
    }

    // MARK: - Quick Look

    /// Same contract as the comment strip's: the bytes land in a
    /// per-attachment temp folder and are reused on a second tap.
    private func preview(_ entry: SessionResultEntry) {
        if sessionResultIsTall(entry) {
            tallPreview = TallPreview(entry: entry)
            return
        }
        guard !downloading else { return }
        downloading = true
        Task {
            let url = await download(entry)
            downloading = false
            if let url { previewURL = url }
        }
    }

    private func download(_ entry: SessionResultEntry) async -> URL? {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("attachments", isDirectory: true)
            .appendingPathComponent(entry.attachmentId, isDirectory: true)
        // The blob carries no filename — an id-named `.png` is enough for
        // Quick Look to pick its image preview.
        let destination = directory.appendingPathComponent("\(entry.attachmentId).png")
        if FileManager.default.fileExists(atPath: destination.path) { return destination }
        do {
            let data = try await deps.attachmentsApi.download(
                accountId: accountId,
                relativeUrl: entry.url
            )
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try data.write(to: destination, options: .atomic)
            return destination
        } catch {
            return nil
        }
    }
}

/// EXP-1128: the tall viewer's sheet item, keyed by the attachment.
private struct TallPreview: Identifiable {
    let entry: SessionResultEntry
    var id: String { entry.attachmentId }
}

/// One published screenshot at the page's tile size, skinned like a posted
/// comment's image (`LargeAttachmentImage`): rounded, hairline-bordered,
/// aspect-FILLED into its box, and fetched through the shared attachment
/// loader so a tile and the same image inline in a description share one
/// download and one decoded copy (`MarkdownImageCache`).
private struct SessionResultTile: View {
    let entry: SessionResultEntry
    let width: CGFloat
    let height: CGFloat
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    let isLoading: Bool

    @Environment(\.displayScale) private var displayScale
    @State private var image: UIImage?

    private var isTall: Bool { sessionResultIsTall(entry) }

    var body: some View {
        ZStack {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
            } else {
                Color.white.opacity(0.06)
            }
            // EXP-1128: a tall picture shows its top under a fade and a
            // `Tall` pill (same copy ×4); the tap opens the scroll viewer.
            if isTall {
                VStack(spacing: 0) {
                    Spacer(minLength: 0)
                    LinearGradient(
                        colors: [.clear, .black.opacity(0.5)],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                    .frame(height: 48)
                }
                .allowsHitTesting(false)
                Text("Tall")
                    .font(.caption2)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Capsule().fill(.black.opacity(0.6)))
                    .overlay(
                        Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                    )
                    .padding(6)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                    .allowsHitTesting(false)
            }
            if isLoading {
                ProgressView()
                    .controlSize(.small)
                    .tint(.white)
            }
        }
        .frame(width: width, height: height)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.fieldRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.fieldRadius)
                .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .task(id: entry.attachmentId) {
            let loader = AttachmentImageLoader(
                baseURL: baseURL,
                accountId: accountId,
                httpClient: httpClient,
                pendingImages: [:]
            )
            // A tall capture keeps only its 4:3 top: the whole page would
            // draw blank past the texture cap, and the tile shows the top.
            // It decodes at the tile's width, off main (`DownsampledImage`),
            // and caches just the crop under its own width-keyed entry.
            if isTall {
                image = await loadTallTop(loader)
            } else {
                image = try? await loader.load(entry.url)
            }
        }
    }

    private func loadTallTop(_ loader: AttachmentImageLoader) async -> UIImage? {
        let maxPixelWidth = Int((width * displayScale).rounded(.up))
        let cacheKey = AttachmentURL.resolve(entry.url, baseURL: baseURL)
            .map { "\($0.absoluteString)#tile\(maxPixelWidth)" }
        if let cacheKey, let cached = MarkdownImageCache.shared.image(for: cacheKey) {
            return cached
        }
        guard let data = try? await loader.data(entry.url) else { return nil }
        let tile = await Task.detached(priority: .userInitiated) {
            DownsampledImage.decode(data, maxPixelWidth: maxPixelWidth)?.tallTopTile()
        }.value
        if let tile, let cacheKey { MarkdownImageCache.shared.store(tile, for: cacheKey) }
        return tile
    }
}

/// EXP-1172: the shown run's raw synced `coding_sessions.results` — what a
/// settled `sessions_show` row looks its picture up in (`preview.id`). Set by
/// `AgentSessionView` off its LIVE row, so a picture that syncs after the call
/// settled appears under it; nil elsewhere (the row then draws no tile).
private struct SessionResultsRawKey: EnvironmentKey {
    static let defaultValue: String? = nil
}

/// EXP-933: switches the enclosing Work screen to its Results face — what the
/// inline `sessions_results` card's `Open Results` button calls. nil outside a
/// Work screen (the card then draws no button).
private struct OpenResultsFaceKey: EnvironmentKey {
    nonisolated(unsafe) static let defaultValue: (() -> Void)? = nil
}

extension EnvironmentValues {
    var openResultsFace: (() -> Void)? {
        get { self[OpenResultsFaceKey.self] }
        set { self[OpenResultsFaceKey.self] = newValue }
    }

    var sessionResultsRaw: String? {
        get { self[SessionResultsRawKey.self] }
        set { self[SessionResultsRawKey.self] = newValue }
    }
}
