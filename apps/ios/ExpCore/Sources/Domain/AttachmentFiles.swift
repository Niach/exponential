import Foundation

/// EXP-297 — the client-side half of the attachment classification contract.
///
/// Attachments are no longer image-only: any content type can be uploaded (10 MB
/// for the inline-image types, 50 MB for everything else). A row is an INLINE
/// IMAGE iff its `content_type` is one of the five raster types the markdown
/// pipeline accepts — those live in the description/comment markdown as
/// `![](…)` and are rendered by the editor. EVERY other row (including other
/// `image/*` types such as tiff) belongs in the issue's Files section, which
/// renders straight from the synced `attachments` rows.
///
/// Mirrors `acceptedImageContentTypes` / `maxFileUploadBytes` in
/// apps/web/src/lib/storage/issue-attachments.ts, and its Android twin
/// `domain/AttachmentFiles.kt`.
public enum AttachmentFiles {
    /// The five content types the inline-image pipeline accepts. Exact mirror of
    /// the server's `acceptedImageContentTypes` — do not widen without changing
    /// the server first, or files would silently vanish from the Files section.
    public static let inlineImageContentTypes: Set<String> = [
        "image/png",
        "image/jpeg",
        "image/webp",
        "image/gif",
        "image/avif",
    ]

    /// 50 MB — the server's cap for non-image uploads (`maxFileUploadBytes`).
    /// Checked client-side so an oversize pick fails instantly instead of after
    /// a long upload.
    public static let maxFileUploadBytes = 50 * 1024 * 1024

    /// 10 MB — the server's cap for the inline-image types
    /// (`maxImageUploadBytes`). Checked client-side for the same reason as the
    /// file cap; every composer that queues images (steer, comments) shares it.
    public static let maxImageUploadBytes = 10 * 1024 * 1024

    /// EXP-824 — 2 MB, the server's cap for the optional `poster` part that
    /// rides a video upload (`maxPosterUploadBytes`). The poster generator
    /// re-encodes below it before the multipart body is built.
    public static let maxPosterUploadBytes = 2 * 1024 * 1024

    /// EXP-554 — how many attachments one comment may carry. Mirrors
    /// `MAX_COMMENT_ATTACHMENTS` in packages/db-schema/src/domain.ts, which the
    /// `comments.create`/`comments.update` inputs enforce server-side.
    public static let maxCommentAttachments = 10

    /// Fallback content type for a pick whose extension maps to no UTI.
    public static let fallbackContentType = "application/octet-stream"

    /// EXACT string match, mirroring the server, web, and desktop — a stored
    /// `image/PNG` or parameterized type is a Files row everywhere, and
    /// classifying it inline here would hide it on iOS only. Client-derived
    /// picker types must be canonicalized via `canonicalContentType` BEFORE
    /// upload, never at classification time.
    public static func isInlineImage(contentType: String) -> Bool {
        inlineImageContentTypes.contains(contentType)
    }

    /// EXP-824 — a `video/*` row is an INLINE VIDEO: embedded as the plain
    /// link `[clip.mp4](/api/attachments/{id})` and rendered as a player
    /// (poster + controls). Prefix match on the canonical essence, mirroring
    /// the server's `isVideoContentType` and its web/desktop/Android twins.
    public static func isInlineVideo(contentType: String) -> Bool {
        normalized(contentType).hasPrefix("video/")
    }

    /// `audio/*` rides the same link form and renders as an audio player row.
    public static func isInlineAudio(contentType: String) -> Bool {
        normalized(contentType).hasPrefix("audio/")
    }

    /// The third class beside inline image and file: rows that render as
    /// inline media. They leave the Files rail exactly like inline images do.
    public static func isInlineMedia(contentType: String) -> Bool {
        isInlineVideo(contentType: contentType) || isInlineAudio(contentType: contentType)
    }

    /// True for rows that belong in the Files rail — neither an inline image
    /// nor inline media (mirrors web `isFileAttachment`).
    public static func isFile(contentType: String) -> Bool {
        !isInlineImage(contentType: contentType) && !isInlineMedia(contentType: contentType)
    }

    // MARK: - EXP-1247: the row-aware rule (`as_file`)

    /// An `asFile` row (uploaded through a FILE button) is never inline, so it
    /// always lists under Files: `isInlineImage` / `isInlineMedia` are false
    /// for it and `isFile` = not inline OR `asFile` (issue-draft.json FILES,
    /// web `lib/attachment-files.ts`).
    public static func isInlineImage(contentType: String, asFile: Bool) -> Bool {
        !asFile && isInlineImage(contentType: contentType)
    }

    public static func isInlineMedia(contentType: String, asFile: Bool) -> Bool {
        !asFile && isInlineMedia(contentType: contentType)
    }

    public static func isFile(contentType: String, asFile: Bool) -> Bool {
        asFile || isFile(contentType: contentType)
    }

    public static func isFile(_ row: AttachmentEntity) -> Bool {
        isFile(contentType: row.contentType, asFile: row.asFile)
    }

    public static func isFile(_ row: DraftAttachmentDto) -> Bool {
        isFile(contentType: row.contentType, asFile: row.asFile)
    }

    // MARK: - Markdown preview (EXP-1003)

    /// EXP-1003 — a markdown attachment opens in the in-app preview instead of
    /// Quick Look. A `text/markdown`/`text/x-markdown` essence always counts; a
    /// generic type (`""`, `text/plain`, octet-stream) falls back to the
    /// `.md`/`.markdown` extension; any other specific type never does.
    /// Mirrored ×4 from web `isMarkdownAttachment`
    /// (apps/web/src/lib/attachment-files.ts).
    public static func isMarkdown(contentType: String, filename: String) -> Bool {
        let essence = normalized(contentType)
        if markdownContentTypes.contains(essence) { return true }
        guard essence.isEmpty || essence == "text/plain" || essence == fallbackContentType else {
            return false
        }
        let name = filename.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return name.hasSuffix(".md") || name.hasSuffix(".markdown")
    }

    /// 1 MiB — above it the preview shows a download hint instead of rendering
    /// (web `MARKDOWN_PREVIEW_MAX_BYTES`).
    public static let markdownPreviewMaxBytes = 1024 * 1024

    /// What a fetched markdown file renders as.
    public enum MarkdownPreviewOutcome: Equatable, Sendable {
        case ready(String)
        case tooLarge
    }

    /// The first half of the double size check: a synced `size_bytes` over the
    /// ceiling goes straight to the too-large hint without a request.
    public static func markdownPreviewSkipsFetch(sizeBytes: Int) -> Bool {
        sizeBytes > markdownPreviewMaxBytes
    }

    /// The second half: legacy rows carry `size_bytes = 0`, so the decoded
    /// text is measured too. Counted in UTF-16 units for parity with JS
    /// `text.length`; invalid UTF-8 decodes lossily (U+FFFD), never fails.
    public static func markdownPreviewOutcome(data: Data) -> MarkdownPreviewOutcome {
        let text = String(decoding: data, as: UTF8.self)
        return text.utf16.count > markdownPreviewMaxBytes ? .tooLarge : .ready(text)
    }

    /// The preview's error copy, byte-identical to the web dialog's.
    public static func markdownPreviewErrorMessage(_ error: Error) -> String {
        if let attachmentsError = error as? AttachmentsError,
           case let .httpError(code, _) = attachmentsError
        {
            return code == 404
                ? "This file is no longer available."
                : "Couldn't load this file (HTTP \(code))."
        }
        let message = error.userFacingMessage
        return message.isEmpty ? "Couldn't load this file." : message
    }

    /// The preview's subtitle: `Markdown · <size>` or plain `Markdown` for a
    /// row without a known size. Same formatter as the Files row.
    public static func markdownPreviewSubtitle(sizeBytes: Int) -> String {
        sizeBytes > 0
            ? "Markdown · \(Int64(sizeBytes).formatted(.byteCount(style: .file)))"
            : "Markdown"
    }

    /// Canonical upload form of a picker-derived content type: lowercased media
    /// essence with any `;`-parameter suffix stripped, falling back to
    /// `application/octet-stream`. Mirrors the server's
    /// `canonicalizeContentType` so stored rows classify identically on every
    /// client.
    public static func canonicalContentType(_ raw: String?) -> String {
        let essence = normalized(raw ?? "")
        return essence.isEmpty ? fallbackContentType : essence
    }

    /// SF Symbol for a file row's leading type glyph. Deliberately NOT a Lucide
    /// registry icon: the shared registry carries no file-type glyphs, and these
    /// are decorative per-row hints rather than a cross-client unified surface.
    public static func sfSymbolName(forContentType contentType: String) -> String {
        let type = normalized(contentType)
        if type == "application/pdf" { return "doc.richtext" }
        if isArchive(type) { return "doc.zipper" }
        if type.hasPrefix("video/") { return "film" }
        if type.hasPrefix("audio/") { return "waveform" }
        if type.hasPrefix("text/") { return "doc.text" }
        return "doc"
    }

    /// A filename safe to write into a temp directory: path separators and
    /// traversal segments can't escape the per-attachment folder, and an empty
    /// result falls back to a generic name.
    public static func sanitizedFilename(_ filename: String) -> String {
        let cleaned = String(filename.map { character in
            let scalar = character.unicodeScalars.first
            let isControl = scalar.map { CharacterSet.controlCharacters.contains($0) } ?? false
            return (character == "/" || character == ":" || character == "\\" || isControl)
                ? "_"
                : character
        })
        let trimmed = cleaned.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty || trimmed == "." || trimmed == ".." { return "file" }
        // Keep the name comfortably inside every filesystem's per-component cap.
        return String(trimmed.prefix(120))
    }

    // MARK: - Internals

    /// Lowercased, parameter-free (`text/plain; charset=utf-8` → `text/plain`).
    private static func normalized(_ contentType: String) -> String {
        let base = contentType.split(separator: ";", maxSplits: 1).first.map(String.init) ?? contentType
        return base.trimmingCharacters(in: .whitespaces).lowercased()
    }

    private static let markdownContentTypes: Set<String> = ["text/markdown", "text/x-markdown"]

    private static let archiveContentTypes: Set<String> = [
        "application/zip",
        "application/x-zip-compressed",
        "application/gzip",
        "application/x-gzip",
        "application/x-tar",
        "application/x-bzip2",
        "application/x-7z-compressed",
        "application/vnd.rar",
        "application/x-rar-compressed",
    ]

    private static func isArchive(_ type: String) -> Bool {
        archiveContentTypes.contains(type)
    }
}
