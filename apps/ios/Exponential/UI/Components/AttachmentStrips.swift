import ExpUI
import ExpCore
import QuickLook
import SwiftUI
import UIKit
import UniformTypeIdentifiers

// EXP-554 — the shared attachment strips.
//
// Two shapes, one look. The PENDING strip is what a composer shows for things
// picked but not sent yet (the steer composer's original 64pt tile + black
// circle X, generalized to non-image files); the READ strip is what a posted
// comment shows below its body, driven straight off the synced `attachments`
// rows whose `comment_id` points at that comment.
//
// Comment attachments are NEVER inlined into the markdown body — that is the
// whole point of EXP-554. The bytes go up through the existing REST route
// (everything, images included, posts to /api/issues/{id}/files) and the ids
// are handed to `comments.create`/`comments.update`.

// MARK: - Pending model

/// One item queued in a comment composer. `uploadedId` is stamped the moment
/// its upload lands, so a mid-batch failure (or a rejected `comments.create`)
/// never re-uploads what already made it — the same retry contract
/// `PendingSteerImage` has for steering.
struct PendingCommentAttachment: Identifiable, Equatable, Sendable {
    let id: UUID
    let data: Data
    let filename: String
    let contentType: String
    var uploadedId: String?

    init(
        id: UUID = UUID(),
        data: Data,
        filename: String,
        contentType: String,
        uploadedId: String? = nil
    ) {
        self.id = id
        self.data = data
        self.filename = filename
        self.contentType = contentType
        self.uploadedId = uploadedId
    }
}

/// What the pending strip needs from a queued item. Derived, not stored: the
/// inline-image rule is a shared contract constant (`AttachmentFiles`), so a
/// tile and the upload route it takes can never disagree.
protocol PendingAttachmentItem: Identifiable where ID == UUID {
    var data: Data { get }
    var filename: String { get }
    var contentType: String { get }
}

extension PendingAttachmentItem {
    var isImage: Bool { AttachmentFiles.isInlineImage(contentType: contentType) }
}

extension PendingCommentAttachment: PendingAttachmentItem {}
extension PendingSteerImage: PendingAttachmentItem {}

// MARK: - Pending strip

/// The composer strip: 64pt tiles with a remove overlay. Images center-crop;
/// everything else gets a type glyph and a truncated name. Extracted verbatim
/// from `AgentSessionView.pendingImageStrip` (EXP-511) so steering and comments
/// queue attachments through one look.
struct PendingAttachmentStrip<Item: PendingAttachmentItem>: View {
    let items: [Item]
    let onRemove: (UUID) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(items) { item in
                    ZStack(alignment: .topTrailing) {
                        tile(item)
                        Button {
                            onRemove(item.id)
                        } label: {
                            AppIcon(AppIcons.uiClose, size: 10, weight: .semibold)
                                .foregroundStyle(.white)
                                .frame(width: 20, height: 20)
                                .background(Circle().fill(Color.black.opacity(0.6)))
                        }
                        .buttonStyle(.plain)
                        .padding(2)
                        .accessibilityLabel("Remove attachment")
                    }
                }
            }
            .padding(.vertical, 2)
        }
    }

    @ViewBuilder
    private func tile(_ item: Item) -> some View {
        if item.isImage, let uiImage = UIImage(data: item.data) {
            Image(uiImage: uiImage)
                .resizable()
                .scaledToFill()
                .frame(width: 64, height: 64)
                .clipShape(RoundedRectangle(cornerRadius: 8))
        } else {
            AttachmentFileTile(filename: item.filename, contentType: item.contentType)
        }
    }
}

/// A non-image tile: SF-symbol type glyph over a truncated filename, squared to
/// the same 64pt box the image thumbs use so a mixed strip stays on one rhythm.
/// The glyph is deliberately an SF Symbol, not a registry icon — the shared
/// Lucide registry carries no file-type glyphs (see `AttachmentFiles`).
struct AttachmentFileTile: View {
    let filename: String
    let contentType: String

    var body: some View {
        VStack(spacing: 4) {
            Image(systemName: AttachmentFiles.sfSymbolName(forContentType: contentType))
                .font(.system(size: 18))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text(filename)
                .font(.system(size: 9))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
                .truncationMode(.middle)
                .padding(.horizontal, 4)
        }
        .frame(width: 64, height: 64)
        .background(Color.white.opacity(0.06))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(
            RoundedRectangle(cornerRadius: 8).stroke(Color.white.opacity(0.1), lineWidth: 0.5)
        )
    }
}

// MARK: - Posted-comment strip

/// The read side: a comment's linked attachments. Images render as LARGE
/// inline tiles stacked vertically (EXP-723 activity redesign — a comment's
/// screenshot is usually its point, and a 64pt crop showed none of it);
/// everything else stays a chip. Both tap through to Quick Look over the same
/// download-to-temp path the Files section uses. Passing `onRemove` turns it
/// into the edit-mode strip (an X on every tile — removals become permanent
/// when the edit saves).
struct CommentAttachmentsStrip: View {
    let attachments: [AttachmentEntity]
    var onRemove: ((String) -> Void)?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var previewURL: URL?
    @State private var downloadingId: String?

    private var images: [AttachmentEntity] {
        attachments.filter { AttachmentFiles.isInlineImage(contentType: $0.contentType) }
    }

    private var files: [AttachmentEntity] {
        attachments.filter { !AttachmentFiles.isInlineImage(contentType: $0.contentType) }
    }

    var body: some View {
        if !attachments.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                if !images.isEmpty {
                    VStack(spacing: 8) {
                        ForEach(images) { attachment in
                            imageTile(attachment)
                        }
                    }
                }
                ForEach(files) { attachment in
                    fileChip(attachment)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .quickLookPreview($previewURL)
        }
    }

    // MARK: - Rows

    private func imageTile(_ attachment: AttachmentEntity) -> some View {
        ZStack(alignment: .topTrailing) {
            Button {
                preview(attachment)
            } label: {
                LargeAttachmentImage(
                    attachment: attachment,
                    baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                    accountId: accountId,
                    httpClient: deps.httpClient,
                    isLoading: downloadingId == attachment.id
                )
            }
            .buttonStyle(.plain)
            .accessibilityLabel(attachment.filename)
            removeButton(attachment)
        }
    }

    private func fileChip(_ attachment: AttachmentEntity) -> some View {
        HStack(spacing: 8) {
            Button {
                preview(attachment)
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: AttachmentFiles.sfSymbolName(forContentType: attachment.contentType))
                        .font(.system(size: 13))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Text(attachment.filename)
                        .font(.caption)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Text(Int64(attachment.sizeBytes).formatted(.byteCount(style: .file)))
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    if downloadingId == attachment.id {
                        ProgressView()
                            .controlSize(.small)
                            .tint(.white)
                    }
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 7)
                .glassRow()
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if onRemove != nil {
                Button {
                    onRemove?(attachment.id)
                } label: {
                    AppIcon(AppIcons.uiClose, size: 11, weight: .semibold)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Remove attachment")
            }
        }
    }

    @ViewBuilder
    private func removeButton(_ attachment: AttachmentEntity) -> some View {
        if onRemove != nil {
            Button {
                onRemove?(attachment.id)
            } label: {
                AppIcon(AppIcons.uiClose, size: 10, weight: .semibold)
                    .foregroundStyle(.white)
                    .frame(width: 20, height: 20)
                    .background(Circle().fill(Color.black.opacity(0.6)))
            }
            .buttonStyle(.plain)
            .padding(2)
            .accessibilityLabel("Remove attachment")
        }
    }

    // MARK: - Quick Look

    /// Same contract as `IssueDetailViewModel.downloadForPreview`: the bytes land
    /// in a per-attachment temp folder and are reused on a second tap.
    private func preview(_ attachment: AttachmentEntity) {
        guard downloadingId == nil else { return }
        downloadingId = attachment.id
        Task {
            let url = await download(attachment)
            downloadingId = nil
            if let url { previewURL = url }
        }
    }

    private func download(_ attachment: AttachmentEntity) async -> URL? {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("attachments", isDirectory: true)
            .appendingPathComponent(attachment.id, isDirectory: true)
        let destination = directory
            .appendingPathComponent(AttachmentFiles.sanitizedFilename(attachment.filename))
        if FileManager.default.fileExists(atPath: destination.path) { return destination }
        do {
            let data = try await deps.attachmentsApi.download(
                accountId: accountId,
                relativeUrl: attachment.url
            )
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try data.write(to: destination, options: .atomic)
            return destination
        } catch {
            return nil
        }
    }
}

/// A posted comment's image, full column width (EXP-723): the placeholder
/// holds the attachment's PROBED aspect ratio so the row never jumps when the
/// bytes land, and the tile is bounded at 480pt tall like every other client.
/// Same loader (and process cache) as the thumb below.
private struct LargeAttachmentImage: View {
    let attachment: AttachmentEntity
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    let isLoading: Bool

    @State private var image: UIImage?

    /// Probed dimensions when the server has them; a neutral 4:3 otherwise.
    private var aspectRatio: CGFloat {
        guard let width = attachment.width, let height = attachment.height,
              width > 0, height > 0 else { return 4.0 / 3.0 }
        return CGFloat(width) / CGFloat(height)
    }

    var body: some View {
        ZStack {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
            } else {
                Color.white.opacity(0.06)
                    .aspectRatio(aspectRatio, contentMode: .fit)
            }
            if isLoading {
                ProgressView()
                    .controlSize(.small)
                    .tint(.white)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: 480)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.fieldRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.fieldRadius)
                .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .task(id: attachment.id) {
            let loader = AttachmentImageLoader(
                baseURL: baseURL,
                accountId: accountId,
                httpClient: httpClient,
                pendingImages: [:]
            )
            image = try? await loader.load(attachment.url)
        }
    }
}

/// A 64pt center-cropped thumbnail for a synced image attachment, fetched (and
/// process-cached) through the editor's loader so a thumb and the same image
/// inline in a description share one download. The PENDING/compose strips keep
/// it; posted comments render `LargeAttachmentImage` instead.
private struct AttachmentThumb: View {
    let attachment: AttachmentEntity
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
        .frame(width: 64, height: 64)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(
            RoundedRectangle(cornerRadius: 8).stroke(Color.white.opacity(0.1), lineWidth: 0.5)
        )
        .task(id: attachment.id) {
            let loader = AttachmentImageLoader(
                baseURL: baseURL,
                accountId: accountId,
                httpClient: httpClient,
                pendingImages: [:]
            )
            image = try? await loader.load(attachment.url)
        }
    }
}

// MARK: - Picking

/// The outcome of turning one pick into a pending attachment: exactly one of the
/// two is set.
struct AttachmentPickOutcome: Sendable {
    var attachment: PendingCommentAttachment?
    var failure: String?
}

/// Shared pick normalization (EXP-554). Both comment composers and the steer
/// composer run picks through here so the HEIC→JPEG transcode, the canonical
/// content type, and the size caps have exactly one implementation.
enum AttachmentPicks {
    /// Normalize a photo pick: anything the server's inline-image pipeline
    /// doesn't accept (notably HEIC) is transcoded to JPEG, exactly as the share
    /// extension does.
    static func normalizedPhoto(
        data: Data,
        contentTypeHint: String?,
        filenameExtensionHint: String?
    ) -> AttachmentPickOutcome {
        let contentType = AttachmentFiles.canonicalContentType(contentTypeHint ?? "image/jpeg")
        let stamp = Int(Date().timeIntervalSince1970)
        let normalized: (data: Data, filename: String, contentType: String)
        if AttachmentFiles.isInlineImage(contentType: contentType) {
            let ext = filenameExtensionHint ?? "jpg"
            normalized = (data, "image-\(stamp).\(ext)", contentType)
        } else if let image = UIImage(data: data),
                  let jpeg = image.jpegData(compressionQuality: 0.9) {
            normalized = (jpeg, "image-\(stamp).jpg", "image/jpeg")
        } else {
            return AttachmentPickOutcome(failure: "That image type isn't supported.")
        }
        guard normalized.data.count <= AttachmentFiles.maxImageUploadBytes else {
            return AttachmentPickOutcome(failure: "That image is too large.")
        }
        return AttachmentPickOutcome(
            attachment: PendingCommentAttachment(
                data: normalized.data,
                filename: normalized.filename,
                contentType: normalized.contentType
            )
        )
    }

    /// Read a `.fileImporter` pick. The security-scoped access is started and
    /// stopped off-main around the read (mirroring
    /// `IssueDetailViewModel.uploadFile`) so a 50 MB, possibly cloud-backed file
    /// can never freeze the main thread.
    static func readPickedFile(at url: URL) async -> AttachmentPickOutcome {
        let filename = AttachmentFiles.sanitizedFilename(url.lastPathComponent)
        // Canonical (lowercased, parameter-free) so the stored row classifies
        // identically on every client.
        let contentType = AttachmentFiles.canonicalContentType(
            UTType(filenameExtension: url.pathExtension)?.preferredMIMEType
        )
        let isImage = AttachmentFiles.isInlineImage(contentType: contentType)
        let limit = isImage
            ? AttachmentFiles.maxImageUploadBytes
            : AttachmentFiles.maxFileUploadBytes
        let tooLarge = isImage
            ? "Images must be 10 MB or smaller."
            : "Files must be 50 MB or smaller."

        return await Task.detached { () -> AttachmentPickOutcome in
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }

            // Size check before buffering — never read bytes the cap rejects.
            let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize
            if let size, size > limit {
                return AttachmentPickOutcome(failure: tooLarge)
            }
            guard let data = try? Data(contentsOf: url) else {
                return AttachmentPickOutcome(failure: "Couldn't read this file.")
            }
            guard data.count <= limit else {
                return AttachmentPickOutcome(failure: tooLarge)
            }
            return AttachmentPickOutcome(
                attachment: PendingCommentAttachment(
                    data: data,
                    filename: filename,
                    contentType: contentType
                )
            )
        }.value
    }
}

// MARK: - Upload on send

/// Upload-on-send for comment attachments (EXP-554). Sequential and
/// retry-safe: every item that already carries an `uploadedId` is skipped, and a
/// failure returns the partially-stamped list so the composer can keep its strip
/// and retry only what is left.
enum CommentAttachmentUploads {
    struct Outcome: Sendable {
        var items: [PendingCommentAttachment]
        var failure: String?
    }

    static func uploadAll(
        _ items: [PendingCommentAttachment],
        accountId: String,
        issueId: String,
        attachmentsApi: AttachmentsApi
    ) async -> Outcome {
        var result = items
        for index in result.indices {
            let item = result[index]
            if item.uploadedId != nil { continue }
            do {
                // EXP-613: inline images and files share the one /files route.
                result[index].uploadedId = try await attachmentsApi.upload(
                    accountId: accountId,
                    issueId: issueId,
                    data: item.data,
                    filename: item.filename,
                    contentType: item.contentType
                ).id
            } catch {
                return Outcome(items: result, failure: error.userFacingMessage)
            }
        }
        return Outcome(items: result, failure: nil)
    }
}
