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
/// ONE scrolling page: a filled group band (EXP-818) per topic, then a
/// WRAPPING ROW of equal-height tiles under it, each captioned with its label.
/// A tap opens the platform preview (Quick Look) over the same
/// download-to-temp path the comment strips use; a TALL picture (EXP-1128, a
/// full-page capture) opens `TallImageViewerSheet` instead, a width-fit
/// vertical scroll. This face owns NEITHER Stop /
/// Resume (the Run face's) NOR the merge bar (the Changes face's), and
/// (EXP-1150) no bottom bar at all — the faces are the screen's tab strip.
///
/// The pure rules — parse, group, tile size — are `ExpCore/SessionResults`,
/// mirrored by web `lib/session-results.ts`, desktop `session_results.rs` and
/// Android `domain/SessionResults.kt`.
struct SessionResultsFace: View {
    let groups: [SessionResultGroup]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var previewURL: URL?
    @State private var downloadingId: String?
    /// EXP-1128: the tall picture the scroll viewer shows, nil when closed.
    @State private var tallPreview: TallPreview?
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

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 20) {
                // By position: the workflow page concatenates several runs'
                // reports, so one topic can repeat.
                ForEach(Array(groups.enumerated()), id: \.offset) { _, group in
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(group.topic)
                        // EXP-933: the topic's report text sits ABOVE its
                        // tiles; a text-only topic is header + text.
                        if let text = group.text {
                            AgentMarkdownText(text: text, context: markdownContext)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.top, 8)
                                .padding(.bottom, group.entries.isEmpty ? 0 : 12)
                        }
                        if !group.entries.isEmpty {
                            FlowLayout(spacing: 12) {
                                ForEach(group.entries, id: \.attachmentId) { entry in
                                    tile(entry)
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
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
        .quickLookPreview($previewURL)
        .sheet(item: $tallPreview) { preview in
            TallImageViewerSheet(entry: preview.entry)
        }
        .accessibilityIdentifier("session-results")
    }

    private func tile(_ entry: SessionResultEntry) -> some View {
        let width = sessionResultTileWidth(entry, height: tileHeight)
        return VStack(alignment: .leading, spacing: 4) {
            Button {
                preview(entry)
            } label: {
                SessionResultTile(
                    entry: entry,
                    width: width,
                    height: tileHeight,
                    baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                    accountId: accountId,
                    httpClient: deps.httpClient,
                    isLoading: downloadingId == entry.attachmentId
                )
            }
            .buttonStyle(.plain)
            Text(entry.label)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(width: width, alignment: .leading)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(
            sessionResultIsTall(entry)
                ? "\(entry.topic), \(entry.label), tall"
                : "\(entry.topic), \(entry.label)"
        )
    }

    // MARK: - Quick Look

    /// Same contract as the comment strip's: the bytes land in a
    /// per-attachment temp folder and are reused on a second tap.
    private func preview(_ entry: SessionResultEntry) {
        if sessionResultIsTall(entry) {
            tallPreview = TallPreview(entry: entry)
            return
        }
        guard downloadingId == nil else { return }
        downloadingId = entry.attachmentId
        Task {
            let url = await download(entry)
            downloadingId = nil
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
}
