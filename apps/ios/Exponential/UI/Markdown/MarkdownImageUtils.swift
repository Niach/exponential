import Foundation

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
}
