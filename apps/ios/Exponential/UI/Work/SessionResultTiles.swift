import ExpCore
import ExpUI
import QuickLook
import SwiftUI
import UIKit

// EXP-879/1251: the pieces a run's published pictures render with — the Guide
// face's tiles (`GuideFace`), the transcript's inline `sessions_show` tile,
// and the environment the Run face hands both. The tile rules (size, tall)
// are `ExpCore/SessionResults`, mirrored ×4.

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

/// EXP-933: switches the enclosing Work screen to its Guide face (EXP-1251) —
/// what the inline `sessions_guide` card's `Open Guide` button calls. nil
/// outside a Work screen (the card then draws no button).
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
