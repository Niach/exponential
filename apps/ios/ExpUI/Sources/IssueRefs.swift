import Foundation
import UIKit

extension NSAttributedString.Key {
    /// Render-only marker on a `#IDENTIFIER` token that resolved to a local
    /// issue; the value is the issue id (String). The markdown serializer
    /// ignores unknown attributes, so decorating never changes the saved text —
    /// the round trip stays byte-identical (pills are display-only).
    public static let markdownIssueRef = NSAttributedString.Key("exp.markdownIssueRef")
}

/// Inline issue references (`#MET-115`) — the same interchange form as the web
/// (`apps/web/src/lib/issue-refs.ts`): plain GFM text, typeable by hand, and
/// rendered as a tappable pill ONLY when the identifier resolves to an issue in
/// the local store. Unresolved tokens stay plain text.
public enum IssueRefs {
    /// Mirrors the web `ISSUE_REF_SOURCE`: `#` not glued to a word or another
    /// `#`, identifier = `{PREFIX}-{number}`, ending at a token boundary.
    public static let pattern = "(?<![\\w#])#([A-Za-z][A-Za-z0-9]*-\\d+)(?![\\w-])"

    // NSRegularExpression is Sendable + documented thread-safe for matching.
    private static let regex = try! NSRegularExpression(pattern: pattern)

    public struct Match: Sendable {
        /// Full token range (includes the leading `#`) in NSString UTF-16 units.
        public let range: NSRange
        /// Uppercase-normalized identifier, e.g. `MET-115`.
        public let identifier: String
        /// The raw matched text including the leading `#`, original case. Carried
        /// from the masked string (which the range was produced on) so callers
        /// never re-substring a different string with this range.
        public let token: String
    }

    /// All `#IDENTIFIER` tokens in `text`, skipping fenced code blocks and
    /// inline code spans (mirrors how the web only decorates non-code text).
    public static func matches(in text: String) -> [Match] {
        let ns = text as NSString
        guard ns.length > 0, ns.range(of: "#").location != NSNotFound else { return [] }
        let masked = maskCodeRegions(text)
        let maskedNS = masked as NSString
        // Fail-safe width guard: masking is UTF-16-width preserving, so the
        // masked string's ranges are meant to map 1:1 onto the original. If a
        // masking bug ever broke that invariant, applying a masked range to the
        // original text would raise an uncatchable NSRangeException. Rather than
        // risk it, decorate nothing — refs render as plain text, never a crash.
        guard maskedNS.length == ns.length else { return [] }
        return regex.matches(in: masked, range: NSRange(location: 0, length: maskedNS.length)).map {
            // Substring on the SAME NSString the range came from. Matched
            // characters are never masked (masked chars are spaces, which can't
            // be part of a #ID-n match), so the masked token equals the original
            // token, case intact.
            Match(
                range: $0.range,
                identifier: maskedNS.substring(with: $0.range(at: 1)).uppercased(),
                token: maskedNS.substring(with: $0.range)
            )
        }
    }

    /// Decorate resolved `#IDENTIFIER` tokens in an already-rendered attributed
    /// string (the block editor's text) with `.markdownIssueRef` + link styling.
    /// Code runs (inline + block) and existing links are skipped via their
    /// attributes — the rendered text no longer carries backticks. The character
    /// content is untouched, so serialization is unaffected.
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
            // Only decorate tokens that sit inside ONE attribute run — a token
            // spanning style boundaries would need per-fragment handling and
            // could disturb serialization.
            var effective = NSRange(location: 0, length: 0)
            let attrs = attributed.attributes(
                at: match.range.location, longestEffectiveRange: &effective, in: match.range)
            guard effective.location == match.range.location, effective.length == match.range.length else {
                continue
            }
            if attrs[.markdownInlineCode] != nil || attrs[.markdownCodeBlock] != nil || attrs[.link] != nil {
                continue
            }
            // Skip refs inside bold/italic/strikethrough spans: decorating
            // splits the attribute run, and the serializer wraps each fragment
            // separately (`**a**` + `**#X-1**` + `**b**`), which would break
            // the byte-identical round trip. Plain runs serialize as raw text,
            // so splitting them is loss-free.
            let font = attrs[.font] as? PlatformFont
            if expFontHasBold(font) || expFontHasItalic(font)
                || attrs[.markdownStrikethrough] as? Bool == true {
                continue
            }
            let identifier = ns.substring(with: match.range(at: 1)).uppercased()
            guard let issueId = resolver(identifier) else { continue }
            let target = mutable ?? NSMutableAttributedString(attributedString: attributed)
            target.addAttributes([
                .markdownIssueRef: issueId,
                .foregroundColor: MarkdownStyle.linkColor,
                .backgroundColor: MarkdownStyle.codeBackground,
            ], range: match.range)
            mutable = target
        }
        return mutable ?? attributed
    }

    /// Display-only markdown transform for read-mode renderers (e.g. the iOS
    /// comment `Markdown` view): wraps resolved tokens as
    /// `[#ID](<scheme>://<issueId>)` links, skipping code. NEVER persisted —
    /// edit paths always reseed from the raw stored markdown.
    public static func linkifyForDisplay(
        _ markdown: String,
        scheme: String = "exp-issue",
        resolver: (String) -> String?
    ) -> String {
        let found = matches(in: markdown)
        guard !found.isEmpty else { return markdown }
        var result = markdown
        // Single coordinate space: replace back-to-front so earlier ranges stay
        // valid, converting each match range against `result` with the guarded
        // `Range(_:in:)` (nil on a surrogate-splitting or out-of-bounds range).
        // The token text comes from the match itself — no NSString.substring,
        // whose out-of-bounds behavior is an uncatchable NSRangeException.
        for match in found.reversed() {
            guard let issueId = resolver(match.identifier) else { continue }
            guard let range = Range(match.range, in: result) else { continue }
            result.replaceSubrange(range, with: "[\(match.token)](\(scheme)://\(issueId))")
        }
        return result
    }

    /// Mask fenced code blocks and inline code spans with spaces so the regex
    /// can't match inside them. UTF-16-width preserving (each character is
    /// replaced by as many spaces as its UTF-16 length), so the returned
    /// string's NSRange indices map 1:1 onto the original text. Space is not a
    /// token character, so masking can't manufacture new matches either.
    private static func maskCodeRegions(_ text: String) -> String {
        var out = ""
        out.reserveCapacity(text.count)
        var inFence = false
        var first = true
        for lineSub in text.split(separator: "\n", omittingEmptySubsequences: false) {
            if !first { out.append("\n") }
            first = false
            let line = String(lineSub)
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                inFence.toggle()
                out.append(blank(line))
                continue
            }
            if inFence {
                out.append(blank(line))
                continue
            }
            // Inline code spans: mask characters between backtick delimiters.
            var inSpan = false
            for ch in line {
                if ch == "`" {
                    inSpan.toggle()
                    out.append(" ")
                } else if inSpan {
                    out.append(blank(String(ch)))
                } else {
                    out.append(ch)
                }
            }
        }
        return out
    }

    /// Spaces matching `s`'s UTF-16 width (keeps NSRange alignment).
    private static func blank(_ s: String) -> String {
        String(repeating: " ", count: s.utf16.count)
    }
}
