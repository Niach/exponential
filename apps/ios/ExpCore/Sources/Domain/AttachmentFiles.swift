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
