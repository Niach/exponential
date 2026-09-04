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
/// person. Read-only renderers show the member's NAME instead
/// (`decorateForDisplay`, EXP-713) — the same split web and Android ship.
/// Unknown addresses stay plain text.
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

    /// Display-only counterpart of `decorate` for read-only renderers (the
    /// comment cards): a resolved `@email` token is REPLACED by `@<name>`, the
    /// way web's read-only widget and Android's `MentionDisplay` render it
    /// (EXP-713 — the raw address was iOS-only). Never run this on a model
    /// whose markdown gets saved: the characters change, so the derived
    /// markdown would carry the name instead of the interchange `@email`.
    /// Editable models keep `decorate` (attributes only).
    ///
    /// The spaces inside the name are non-breaking so the pill stays one unit
    /// (web `.mention-pill { white-space: nowrap }`). Same guards as
    /// `decorate`; already-decorated runs are skipped so a re-run (the member
    /// list syncing in later) can never substitute twice.
    public static func decorateForDisplay(
        _ attributed: NSAttributedString,
        resolver: (String) -> String?
    ) -> NSAttributedString {
        guard attributed.length > 0 else { return attributed }
        let ns = attributed.string as NSString
        let found = regex.matches(in: attributed.string, range: NSRange(location: 0, length: ns.length))
        guard !found.isEmpty else { return attributed }

        var mutable: NSMutableAttributedString?
        // Replacements change the length, so walk back-to-front to keep every
        // earlier match range valid (the `IssueRefs.decorateForDisplay` pattern).
        for match in found.reversed() {
            var effective = NSRange(location: 0, length: 0)
            let attrs = attributed.attributes(
                at: match.range.location, longestEffectiveRange: &effective, in: match.range)
            guard effective.location == match.range.location, effective.length == match.range.length else {
                continue
            }
            if attrs[.markdownInlineCode] != nil || attrs[.markdownCodeBlock] != nil || attrs[.link] != nil {
                continue
            }
            let font = attrs[.font] as? PlatformFont
            if expFontHasBold(font) || expFontHasItalic(font)
                || attrs[.markdownStrikethrough] as? Bool == true {
                continue
            }
            if attrs[.markdownMention] != nil { continue }
            let email = ns.substring(with: match.range(at: 1))
            guard let name = resolver(email) else { continue }
            var chipAttrs = attrs
            for (key, value) in expChipAttributes(baseColor: attrs[.foregroundColor] as? PlatformColor) {
                chipAttrs[key] = value
            }
            chipAttrs[.markdownMention] = name
            let piece = NSAttributedString(string: displayText(name: name, email: email), attributes: chipAttrs)
            let target = mutable ?? NSMutableAttributedString(attributedString: attributed)
            target.replaceCharacters(in: match.range, with: piece)
            mutable = target
        }
        return mutable ?? attributed
    }

    /// `@<name>` with the name's internal whitespace made non-breaking; a
    /// blank name falls back to the stored token.
    public static func displayText(name: String, email: String) -> String {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "@\(email)" }
        let joined = trimmed
            .split(whereSeparator: { $0.isWhitespace })
            .joined(separator: "\u{00A0}")
        return "@\(joined)"
    }
}
