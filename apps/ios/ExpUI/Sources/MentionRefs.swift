import Foundation
import UIKit

extension NSAttributedString.Key {
    /// Render-only marker on an `@email` token that resolved to a team member;
    /// the value is the member's display name (String). Like
    /// `.markdownIssueRef` the serializer ignores it, so the stored markdown
    /// keeps the plain `@email` interchange form.
    public static let markdownMention = NSAttributedString.Key("exp.markdownMention")
}

/// Inline `@email` mentions — the Android/web counterpart of `IssueRefs`
/// (`apps/web/src/lib/mention-pill-extension.ts`). The token stays the literal
/// `@email` in both the document and the editor (web: "hiding characters under
/// an active caret makes editing hazardous"); only the styling says it is a
/// person. Unknown addresses stay plain text.
public enum MentionRefs {

    /// Mirrors the web `MENTION_SOURCE` and the Android `Mentions.REGEX`.
    public static let pattern = "@([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,})"

    // NSRegularExpression is Sendable + documented thread-safe for matching.
    private static let regex = try! NSRegularExpression(pattern: pattern)

    public struct Match: Sendable {
        /// Full token range (includes the leading `@`) in NSString UTF-16 units.
        public let range: NSRange
        /// The address as written.
        public let email: String
    }

    /// All `@email` tokens in `text`, skipping fenced code blocks and inline
    /// code spans (same masking as `IssueRefs.matches`).
    public static func matches(in text: String) -> [Match] {
        let ns = text as NSString
        guard ns.length > 0, ns.range(of: "@").location != NSNotFound else { return [] }
        let masked = expMaskCodeRegions(text)
        let maskedNS = masked as NSString
        // Same fail-safe as IssueRefs: if masking ever stopped being
        // UTF-16-width preserving, decorate nothing rather than risk an
        // uncatchable NSRangeException.
        guard maskedNS.length == ns.length else { return [] }
        return regex.matches(in: masked, range: NSRange(location: 0, length: maskedNS.length)).map {
            Match(range: $0.range, email: maskedNS.substring(with: $0.range(at: 1)))
        }
    }

    /// Decorate resolved `@email` tokens in an already-rendered attributed
    /// string. Attributes only — the character content is untouched, so
    /// serialization is unaffected. The guards mirror `IssueRefs.decorate`:
    /// one attribute run, never inside code / links / emphasis.
    public static func decorate(
        _ attributed: NSAttributedString,
        resolver: (String) -> String?
    ) -> NSAttributedString {
        guard attributed.length > 0 else { return attributed }
        let ns = attributed.string as NSString
        let found = regex.matches(in: attributed.string, range: NSRange(location: 0, length: ns.length))
        guard !found.isEmpty else { return attributed }

        var mutable: NSMutableAttributedString?
        for match in found {
            var effective = NSRange(location: 0, length: 0)
            let attrs = attributed.attributes(
                at: match.range.location, longestEffectiveRange: &effective, in: match.range)
            guard effective.location == match.range.location, effective.length == match.range.length else {
                continue
            }
            if attrs[.markdownInlineCode] != nil || attrs[.markdownCodeBlock] != nil || attrs[.link] != nil {
                continue
            }
            // Verbatim pipe-table runs are re-emitted from their source string,
            // so nothing inside them may be decorated (EXP-322).
            if attrs[.markdownTableBlock] != nil { continue }
            let font = attrs[.font] as? PlatformFont
            if expFontHasBold(font) || expFontHasItalic(font)
                || attrs[.markdownStrikethrough] as? Bool == true {
                continue
            }
            let email = ns.substring(with: match.range(at: 1))
            guard let name = resolver(email) else { continue }
            let target = mutable ?? NSMutableAttributedString(attributedString: attributed)
            target.addAttributes(
                expChipAttributes(baseColor: attrs[.foregroundColor] as? PlatformColor)
                    .merging([.markdownMention: name]) { _, new in new },
                range: match.range,
            )
            mutable = target
        }
        return mutable ?? attributed
    }
}
