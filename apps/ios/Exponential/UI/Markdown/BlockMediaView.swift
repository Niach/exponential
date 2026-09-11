import ExpCore
import ExpUI
import SwiftUI
import UIKit

/// EXP-824 — the `ContentBlock.attachmentLink` renderer. What it paints
/// follows the referenced row, resolved through the model's
/// `attachmentResolver`:
///
/// - a `draft://` placeholder with pending bytes → the pending video tile
///   (poster + play glyph + duration) or audio row, with the same
///   uploading / failed-retry overlays an image block gets;
/// - a synced `video/*` row → `InlineVideoPlayerView`;
/// - a synced `audio/*` row → `InlineAudioPlayerView`;
/// - anything else (other types, a row not synced yet, a dangling draft) →
///   the plain link, which is what the markdown says.
///
/// The X + tap-below strip mirror `BlockImageView` so a media block edits
/// exactly like an image.
struct BlockMediaView: View {
    let model: IssueEditorModel
    let blockId: UUID
    let url: String
    let label: String
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    let pendingImages: [String: PendingImage]
    var isReadOnly = false
    var maxHeight: CGFloat?
    var onDelete: () -> Void
    var onTapBelow: () -> Void
    var onRetry: () -> Void

    @Environment(\.openURL) private var openURL

    private var uploadState: ImageUploadState { model.uploadState(for: blockId) }

    var body: some View {
        // Reading the revision subscribes this view to `attachmentsDidChange`,
        // so a row syncing in after the block rendered upgrades the link.
        let _ = model.attachmentInfoRevision
        VStack(spacing: 0) {
            ZStack(alignment: .topTrailing) {
                content
                    .frame(maxWidth: .infinity, alignment: .leading)
                if !isReadOnly {
                    Button(action: onDelete) {
                        AppIcon(AppIcons.uiClear, size: 22)
                            .foregroundStyle(.white.opacity(0.9))
                            .padding(2)
                            .background(Circle().fill(.black.opacity(0.5)))
                    }
                    .padding(8)
                    .accessibilityLabel("Remove")
                }
            }
            .padding(.vertical, 4)

            if !isReadOnly {
                Color.clear
                    .frame(height: 20)
                    .contentShape(Rectangle())
                    .onTapGesture { onTapBelow() }
            }
        }
    }

    @ViewBuilder
    private var content: some View {
        if MarkdownImageUtils.isDraft(url) {
            if let pending = pendingImages[url] {
                pendingTile(pending)
            } else {
                plainLink
            }
        } else if let attachmentId = AttachmentLinks.attachmentId(fromUrl: url, baseURL: baseURL),
                  let info = model.attachmentResolver?(attachmentId) {
            if info.isVideo {
                InlineVideoPlayerView(
                    attachmentId: attachmentId,
                    url: url,
                    info: info,
                    baseURL: baseURL,
                    accountId: accountId,
                    httpClient: httpClient,
                    maxHeight: maxHeight
                )
            } else if info.isAudio {
                InlineAudioPlayerView(
                    url: url, label: label, info: info, baseURL: baseURL, accountId: accountId
                )
            } else {
                plainLink
            }
        } else {
            plainLink
        }
    }

    /// The upload-in-flight rendering: the locally generated poster (or a
    /// neutral box) at the probed aspect ratio, dimmed play glyph, duration,
    /// and the uploading / failed capsule an image block shows.
    @ViewBuilder
    private func pendingTile(_ pending: PendingImage) -> some View {
        let isAudio = AttachmentFiles.isInlineAudio(contentType: pending.contentType)
        ZStack(alignment: .bottomLeading) {
            if isAudio {
                pendingAudioRow(pending)
            } else {
                VideoPosterTile(
                    poster: pending.poster.flatMap(UIImage.init(data:)),
                    aspectRatio: pendingAspectRatio(pending),
                    durationMs: pending.durationMs,
                    dimmed: true
                )
                .frame(maxHeight: maxHeight ?? .infinity, alignment: .leading)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }
            if uploadState == .uploading {
                uploadingOverlay
            } else if case .failed(let reason) = uploadState {
                uploadFailedOverlay(reason)
            }
        }
    }

    private func pendingAspectRatio(_ pending: PendingImage) -> CGFloat {
        guard let w = pending.width, let h = pending.height, w > 0, h > 0 else { return 16.0 / 9.0 }
        return CGFloat(w) / CGFloat(h)
    }

    private func pendingAudioRow(_ pending: PendingImage) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "waveform")
                .font(.system(size: 13))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text(pending.filename)
                .font(.caption)
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.middle)
            if let durationMs = pending.durationMs {
                Text(MediaDuration.format(ms: durationMs))
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    /// The markdown's own meaning: a tappable link labelled with the file
    /// name. Opens through the system so a same-origin attachment URL lands
    /// in the browser with the user's web session, like any other link.
    private var plainLink: some View {
        Button {
            if let resolved = AttachmentURL.resolve(url, baseURL: baseURL) { openURL(resolved) }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "link")
                    .font(.system(size: 12))
                Text(label.isEmpty ? url : label)
                    .font(.body)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .foregroundStyle(Color(uiColor: MarkdownStyle.linkColor))
        }
        .buttonStyle(.plain)
        .padding(.vertical, 4)
    }

    private var uploadingOverlay: some View {
        HStack(spacing: 6) {
            ProgressView().tint(.white).controlSize(.small)
            Text("Uploading…").font(.caption).foregroundStyle(.white)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.black.opacity(0.45), in: Capsule())
        .padding(8)
    }

    private func uploadFailedOverlay(_ reason: ImageUploadFailureReason) -> some View {
        Button(action: onRetry) {
            HStack(spacing: 6) {
                AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                Text(reason == .storageFull
                    ? "Team storage is full. Tap to retry"
                    : "Upload failed. Tap to retry")
                    .font(.caption)
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(.black.opacity(0.45), in: Capsule())
            .padding(8)
        }
        .buttonStyle(.plain)
    }
}
