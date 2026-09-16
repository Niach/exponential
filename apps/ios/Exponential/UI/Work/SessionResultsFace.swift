import ExpCore
import ExpUI
import QuickLook
import SwiftUI
import UIKit

/// EXP-879: the Work screen's RESULTS face — the screenshots the shown run
/// published with `exponential_sessions_results`, read off its synced
/// `coding_sessions.results` blob.
///
/// ONE scrolling page: a filled group band (EXP-818) per topic, then a
/// WRAPPING ROW of equal-height tiles under it, each captioned with its label.
/// A tap opens the platform preview (Quick Look) over the same
/// download-to-temp path the comment strips use. This face owns NEITHER Stop /
/// Resume (the Run face's) NOR the merge bar (the Changes face's): its bottom
/// bar carries the face switcher and nothing else.
///
/// The pure rules — parse, group, tile size — are `ExpCore/SessionResults`,
/// mirrored by web `lib/session-results.ts`, desktop `session_results.rs` and
/// Android `domain/SessionResults.kt`.
struct SessionResultsFace<Trailing: View>: View {
    let groups: [SessionResultGroup]
    @ViewBuilder let trailing: () -> Trailing

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var previewURL: URL?
    @State private var downloadingId: String?
    /// The page's content width, measured once — a phone is narrower than the
    /// pinned 320pt tile is wide for anything landscape, so the whole page
    /// scales DOWN by one factor. Every tile keeps its probed aspect and every
    /// tile stays the same height, which is the point of the strip.
    @State private var contentWidth: CGFloat = 0

    private var horizontalPadding: CGFloat { 16 }

    /// The widest tile at the pinned height decides the page's scale.
    private var tileHeight: CGFloat {
        sessionResultTileHeightFitting(
            groups.flatMap(\.entries),
            availableWidth: contentWidth - horizontalPadding * 2
        )
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 20) {
                ForEach(groups) { group in
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(group.topic)
                        FlowLayout(spacing: 12) {
                            ForEach(group.entries, id: \.attachmentId) { entry in
                                tile(entry)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
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
        .accessibilityIdentifier("session-results")
        // The face switcher, alone — Results has no verb of its own.
        .safeAreaInset(edge: .bottom) {
            FloatingBottomBar {
                EmptyView()
            } center: {
                EmptyView()
            } trailing: {
                trailing()
            }
        }
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
        .accessibilityLabel("\(entry.topic), \(entry.label)")
    }

    // MARK: - Quick Look

    /// Same contract as the comment strip's: the bytes land in a
    /// per-attachment temp folder and are reused on a second tap.
    private func preview(_ entry: SessionResultEntry) {
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

    @State private var image: UIImage?

    var body: some View {
        ZStack {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
            } else {
                Color.white.opacity(0.06)
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
            image = try? await loader.load(entry.url)
        }
    }
}
