import Foundation

struct PendingImage: Sendable {
    let data: Data
    let filename: String
    let contentType: String
}

enum MarkdownImageUtils {
    private static let imagePattern = #"!\[([^\]]*)\]\(([^)]+)\)"#

    static func extractImageUrls(from markdown: String) -> [String] {
        guard let regex = try? NSRegularExpression(pattern: imagePattern) else { return [] }
        let range = NSRange(markdown.startIndex..., in: markdown)
        return regex.matches(in: markdown, range: range).compactMap { match in
            guard let urlRange = Range(match.range(at: 2), in: markdown) else { return nil }
            return String(markdown[urlRange])
        }
    }

    static func replaceImageUrl(in markdown: String, from oldUrl: String, to newUrl: String) -> String {
        markdown.replacingOccurrences(of: oldUrl, with: newUrl)
    }

    static func hasDraftImages(_ markdown: String) -> Bool {
        extractImageUrls(from: markdown).contains { $0.hasPrefix("draft://") }
    }

    static func draftUrl() -> String {
        "draft://\(UUID().uuidString)"
    }

    /// Strips `![alt](draft://...)` markdown image references for placeholders
    /// that no longer have a pending upload (e.g. user undid the insertion).
    /// Returns the cleaned markdown.
    static func stripUnknownDraftImages(
        _ markdown: String,
        keep: Set<String>
    ) -> String {
        guard let regex = try? NSRegularExpression(pattern: imagePattern) else { return markdown }
        var result = markdown
        let range = NSRange(result.startIndex..., in: result)
        let matches = regex.matches(in: result, range: range).reversed()
        for match in matches {
            guard let urlRange = Range(match.range(at: 2), in: result),
                  let fullRange = Range(match.range, in: result) else { continue }
            let url = String(result[urlRange])
            if url.hasPrefix("draft://") && !keep.contains(url) {
                result.removeSubrange(fullRange)
            }
        }
        return result
    }
}
