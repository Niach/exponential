import ExpCore
import Foundation

/// An upload queued behind a `draft://` placeholder in the editor: an inline
/// image, or (EXP-824) a normalised video/audio clip. Media entries carry the
/// probed length and a JPEG poster the block shows while the bytes go up and
/// the upload sends beside the file (`MediaUploadParts`).
public struct PendingImage: Sendable {
    public let data: Data
    public let filename: String
    public let contentType: String
    /// Intrinsic pixel size of the image, derived locally at insert time so the
    /// editor can reserve correct aspect-ratio space before/while uploading.
    /// For a video: the rotation-aware frame size.
    public var width: Int?
    public var height: Int?
    /// EXP-824: media length in milliseconds (nil for images).
    public var durationMs: Int?
    /// EXP-824: JPEG poster frame for a video (nil for images and audio).
    public var poster: Data?

    public init(
        data: Data,
        filename: String,
        contentType: String,
        width: Int? = nil,
        height: Int? = nil,
        durationMs: Int? = nil,
        poster: Data? = nil
    ) {
        self.data = data
        self.filename = filename
        self.contentType = contentType
        self.width = width
        self.height = height
        self.durationMs = durationMs
        self.poster = poster
    }

    /// Whether this entry is inline MEDIA (video/audio) rather than an image —
    /// decides the block form (`[label](url)` vs `![alt](url)`) and the
    /// multipart parts the upload carries.
    public var isMedia: Bool { AttachmentFiles.isInlineMedia(contentType: contentType) }

    /// The media metadata parts for the upload; nil for images.
    public var mediaUploadParts: MediaUploadParts? {
        guard isMedia else { return nil }
        return MediaUploadParts(poster: poster, width: width, height: height, durationMs: durationMs)
    }
}

/// A plain `[label](url "title")` occurrence — never `![…]` — parsed out of a
/// markdown string (EXP-824). Mirrors web `MarkdownLinkOccurrence`
/// (`extractMarkdownLinkOccurrences`), the media-link twin of the image
/// occurrence below.
public struct MarkdownLinkOccurrence {
    public let label: String
    public let url: String
    public let occurrenceIndex: Int
    /// Range of the full `[label](url)` token within the source string.
    public let range: NSRange
    /// Range of just the URL within the source string.
    public let urlRange: NSRange
}

/// A single `![alt](url "title")` occurrence parsed out of a markdown string.
/// Mirrors `MarkdownImageOccurrence` in the web backend
/// (`apps/web/src/lib/storage/issue-attachments.ts`) so all clients agree on
/// image rewrite/removal semantics byte-for-byte.
public struct MarkdownImageOccurrence {
    public let alt: String
    public let url: String
    public let occurrenceIndex: Int
    /// Range of the full `![alt](url)` token within the source string.
    public let range: NSRange
    /// Range of just the URL within the source string.
    public let urlRange: NSRange
}

public enum MarkdownImageUtils {
    // Matches the web pattern exactly: alt = group 1, url = group 2, optional
    // quoted title is consumed but not captured. Stops the URL at the first
    // whitespace so `![a](u "t")` parses cleanly. The alt group consumes
    // backslash-escape pairs — serializers escape markdown punctuation in alt
    // (web's TipTap turns a `shot [1].png` filename into `\[1\]`), and a plain
    // `[^\]]*` would drop the whole occurrence (REV-6). `alt` stays the raw
    // (still-escaped) source text; only ranges/urls are consumed here.
    private static let imagePattern = #"!\[((?:\\.|[^\\\]])*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)"#

    private static let regex: NSRegularExpression? = {
        try? NSRegularExpression(pattern: imagePattern)
    }()

    // EXP-824: the plain-link twin, byte-identical to the web
    // `markdownLinkPattern`. The negative lookbehind keeps image tokens
    // (whose `[` follows a `!`) out of the scan; the label consumes escape
    // pairs like the image alt does.
    private static let linkPattern = #"(?<!!)\[((?:\\.|[^\\\]])*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)"#

    private static let linkRegex: NSRegularExpression? = {
        try? NSRegularExpression(pattern: linkPattern)
    }()

    // MARK: - Parsing

    public static func occurrences(in markdown: String) -> [MarkdownImageOccurrence] {
        guard let regex else { return [] }
        let ns = markdown as NSString
        let full = NSRange(location: 0, length: ns.length)
        return regex.matches(in: markdown, range: full).enumerated().compactMap { index, match in
            let altRange = match.range(at: 1)
            let urlRange = match.range(at: 2)
            guard urlRange.location != NSNotFound else { return nil }
            let alt = altRange.location != NSNotFound ? ns.substring(with: altRange) : ""
            return MarkdownImageOccurrence(
                alt: alt,
                url: ns.substring(with: urlRange),
                occurrenceIndex: index,
                range: match.range,
                urlRange: urlRange
            )
        }
    }

    public static func extractImageUrls(from markdown: String) -> [String] {
        occurrences(in: markdown).map(\.url)
    }

    public static func hasDraftImages(_ markdown: String) -> Bool {
        occurrences(in: markdown).contains { $0.url.hasPrefix("draft://") }
    }

    // MARK: - Plain links (EXP-824, the inline media form)

    public static func linkOccurrences(in markdown: String) -> [MarkdownLinkOccurrence] {
        guard let linkRegex else { return [] }
        let ns = markdown as NSString
        let full = NSRange(location: 0, length: ns.length)
        return linkRegex.matches(in: markdown, range: full).enumerated().compactMap { index, match in
            let labelRange = match.range(at: 1)
            let urlRange = match.range(at: 2)
            guard urlRange.location != NSNotFound else { return nil }
            let label = labelRange.location != NSNotFound ? ns.substring(with: labelRange) : ""
            return MarkdownLinkOccurrence(
                label: label,
                url: ns.substring(with: urlRange),
                occurrenceIndex: index,
                range: match.range,
                urlRange: urlRange
            )
        }
    }

    /// A `[label](draft://…)` media placeholder is still in the body.
    public static func hasDraftLinks(_ markdown: String) -> Bool {
        linkOccurrences(in: markdown).contains { isDraft($0.url) }
    }

    /// Any upload placeholder at all — image OR media link. What every save
    /// gate checks, so a failed video upload can never leak a `draft://` link
    /// into a persisted body.
    public static func hasDraftReferences(_ markdown: String) -> Bool {
        hasDraftImages(markdown) || hasDraftLinks(markdown)
    }

    /// Rebuilds `markdown`, letting `transform` return a replacement for each
    /// plain-link occurrence (or `nil` to keep it verbatim). Mirrors web
    /// `updateMarkdownLinks`.
    public static func updateLinks(
        in markdown: String,
        transform: (MarkdownLinkOccurrence) -> String?
    ) -> String {
        let ns = markdown as NSString
        var result = ""
        var lastIndex = 0
        for occ in linkOccurrences(in: markdown) {
            result += ns.substring(with: NSRange(location: lastIndex, length: occ.range.location - lastIndex))
            if let replacement = transform(occ) {
                result += replacement
            } else {
                result += ns.substring(with: occ.range)
            }
            lastIndex = occ.range.location + occ.range.length
        }
        result += ns.substring(with: NSRange(location: lastIndex, length: ns.length - lastIndex))
        return result
    }

    /// Replaces the URL of every plain-link occurrence whose URL exactly equals
    /// `oldUrl`, preserving the label (and any title) verbatim.
    public static func replaceLinkUrl(in markdown: String, from oldUrl: String, to newUrl: String) -> String {
        let oldNS = oldUrl as NSString
        return updateLinks(in: markdown) { occ -> String? in
            guard occ.url == oldUrl else { return nil }
            let token = (markdown as NSString).substring(with: occ.range) as NSString
            let relativeURLRange = NSRange(
                location: occ.urlRange.location - occ.range.location,
                length: oldNS.length
            )
            return token.replacingCharacters(in: relativeURLRange, with: newUrl)
        }
    }

    /// Removes `[label](draft://…)` references whose placeholder is no longer
    /// in `keep`. Only draft URLs are ever removed — a real link is untouched.
    public static func stripUnknownDraftLinks(_ markdown: String, keep: Set<String>) -> String {
        updateLinks(in: markdown) { occ in
            (isDraft(occ.url) && !keep.contains(occ.url)) ? "" : nil
        }
    }

    /// Both placeholder forms at once — what a create-before-upload path
    /// strips out of the body it sends first.
    public static func stripUnknownDrafts(_ markdown: String, keep: Set<String>) -> String {
        stripUnknownDraftLinks(stripUnknownDraftImages(markdown, keep: keep), keep: keep)
    }

    public static func draftUrl() -> String {
        "draft://\(UUID().uuidString)"
    }

    public static func isDraft(_ url: String) -> Bool {
        url.hasPrefix("draft://")
    }

    // MARK: - Rewriting

    /// Rebuilds `markdown`, letting `transform` return a replacement for each
    /// image occurrence (or `nil` to keep it verbatim). Mirrors the web
    /// `updateMarkdownImages` so rewrite results match the server exactly.
    public static func updateImages(
        in markdown: String,
        transform: (MarkdownImageOccurrence) -> String?
    ) -> String {
        let ns = markdown as NSString
        var result = ""
        var lastIndex = 0
        for occ in occurrences(in: markdown) {
            result += ns.substring(with: NSRange(location: lastIndex, length: occ.range.location - lastIndex))
            if let replacement = transform(occ) {
                result += replacement
            } else {
                result += ns.substring(with: occ.range)
            }
            lastIndex = occ.range.location + occ.range.length
        }
        result += ns.substring(with: NSRange(location: lastIndex, length: ns.length - lastIndex))
        return result
    }

    /// Replaces the URL of every image occurrence whose URL exactly equals
    /// `oldUrl`, preserving alt text and any title verbatim. Targeted by URL
    /// (not a blind substring replace) so alt text containing the URL string
    /// can never be corrupted.
    public static func replaceImageUrl(in markdown: String, from oldUrl: String, to newUrl: String) -> String {
        let oldNS = oldUrl as NSString
        let escapedNew = newUrl
        return updateImages(in: markdown) { occ -> String? in
            guard occ.url == oldUrl else { return nil }
            // Rebuild only the url portion of the matched token, keeping alt + title.
            let token = (markdown as NSString).substring(with: occ.range) as NSString
            let relativeURLRange = NSRange(
                location: occ.urlRange.location - occ.range.location,
                length: oldNS.length
            )
            return token.replacingCharacters(in: relativeURLRange, with: escapedNew)
        }
    }

    /// Removes `![alt](draft://…)` references whose placeholder is no longer in
    /// `keep` (e.g. the user undid the insertion, or the upload failed and we
    /// are dropping a dangling draft). Only draft URLs are ever removed.
    public static func stripUnknownDraftImages(_ markdown: String, keep: Set<String>) -> String {
        updateImages(in: markdown) { occ in
            (isDraft(occ.url) && !keep.contains(occ.url)) ? "" : nil
        }
    }
}

private extension NSString {
    func replacingCharacters(in range: NSRange, with replacement: String) -> String {
        let mutable = NSMutableString(string: self)
        mutable.replaceCharacters(in: range, with: replacement)
        return mutable as String
    }
}
