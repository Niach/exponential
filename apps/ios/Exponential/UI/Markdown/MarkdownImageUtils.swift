import Foundation

struct PendingImage: Sendable {
    let data: Data
    let filename: String
    let contentType: String
    /// Intrinsic pixel size of the image, derived locally at insert time so the
    /// editor can reserve correct aspect-ratio space before/while uploading.
    var width: Int?
    var height: Int?
}

/// A single `![alt](url "title")` occurrence parsed out of a markdown string.
/// Mirrors `MarkdownImageOccurrence` in the web backend
/// (`apps/web/src/lib/storage/issue-attachments.ts`) so all clients agree on
/// image rewrite/removal semantics byte-for-byte.
struct MarkdownImageOccurrence {
    let alt: String
    let url: String
    let occurrenceIndex: Int
    /// Range of the full `![alt](url)` token within the source string.
    let range: NSRange
    /// Range of just the URL within the source string.
    let urlRange: NSRange
}

enum MarkdownImageUtils {
    // Matches the web pattern exactly: alt = group 1, url = group 2, optional
    // quoted title is consumed but not captured. Stops the URL at the first
    // whitespace so `![a](u "t")` parses cleanly.
    private static let imagePattern = #"!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)"#

    private static let regex: NSRegularExpression? = {
        try? NSRegularExpression(pattern: imagePattern)
    }()

    // MARK: - Parsing

    static func occurrences(in markdown: String) -> [MarkdownImageOccurrence] {
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

    static func extractImageUrls(from markdown: String) -> [String] {
        occurrences(in: markdown).map(\.url)
    }

    static func hasDraftImages(_ markdown: String) -> Bool {
        occurrences(in: markdown).contains { $0.url.hasPrefix("draft://") }
    }

    static func draftUrl() -> String {
        "draft://\(UUID().uuidString)"
    }

    static func isDraft(_ url: String) -> Bool {
        url.hasPrefix("draft://")
    }

    // MARK: - Rewriting

    /// Rebuilds `markdown`, letting `transform` return a replacement for each
    /// image occurrence (or `nil` to keep it verbatim). Mirrors the web
    /// `updateMarkdownImages` so rewrite results match the server exactly.
    static func updateImages(
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
    static func replaceImageUrl(in markdown: String, from oldUrl: String, to newUrl: String) -> String {
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
    static func stripUnknownDraftImages(_ markdown: String, keep: Set<String>) -> String {
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
