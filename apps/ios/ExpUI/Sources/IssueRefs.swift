import Foundation
import UIKit

extension NSAttributedString.Key {
    /// Render-only marker on a `#IDENTIFIER` token that resolved to a local
    /// issue; the value is the issue id (String). The markdown serializer
    /// ignores unknown attributes, so decorating never changes the saved text —
    /// the round trip stays byte-identical (pills are display-only).
    public static let markdownIssueRef = NSAttributedString.Key("exp.markdownIssueRef")

    /// `true` over every chip run — the `#IDENTIFIER` token, its display-only
    /// title attachment, and resolved `@email` mentions. `MarkdownLayoutManager`
    /// paints one rounded capsule per line fragment of a run, which is why the
    /// chip carries no `.backgroundColor` (that would double-paint a square
    /// box under the pill).
    public static let markdownChip = NSAttributedString.Key("exp.markdownChip")

    /// The `.foregroundColor` a run had before it was chipped, so un-chipping
    /// restores the blockquote / body color instead of guessing (EXP-322).
    public static let markdownChipBaseColor = NSAttributedString.Key("exp.markdownChipBaseColor")

    /// The `IssueRefStatusInfo` of a resolved `#IDENTIFIER` chip — the glyph
    /// `MarkdownLayoutManager` paints over the token's `#` cell (EXP-423).
    /// Render-only like every other chip attribute.
    public static let markdownIssueRefStatus = NSAttributedString.Key("exp.markdownIssueRefStatus")
}

/// The status a resolved `#IDENTIFIER` chip shows: a shared-registry glyph name
/// (EXP-273) plus the tint the platform's status colors resolved to (EXP-423,
/// Linear parity). A reference type because it rides an attributed-string
/// attribute.
public final class IssueRefStatusInfo: NSObject {
    public let iconName: String
    public let color: PlatformColor

    public init(iconName: String, color: PlatformColor) {
        self.iconName = iconName
        self.color = color
    }

    /// Equality over BOTH fields, for the same reason
    /// `IssueRefTitleAttachment.isEqual` exists: `MarkdownChipDecorator`'s
    /// `changed` flag IS an `NSAttributedString.isEqual`, and two freshly built
    /// infos are otherwise never equal — the editor would rewrite its storage on
    /// every keystroke forever. Comparing both fields (not just the name) is
    /// what still lets a recolored status repaint.
    public override func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? IssueRefStatusInfo else { return false }
        if other === self { return true }
        return other.iconName == iconName && other.color == color
    }

    public override var hash: Int {
        var hasher = Hasher()
        hasher.combine(iconName)
        hasher.combine(color)
        return hasher.finalize()
    }
}

/// The attributes every chip run carries (EXP-322). No `.backgroundColor`:
/// `MarkdownLayoutManager` paints a rounded capsule off `.markdownChip`
/// instead, which is what turns a flat highlight into a real pill.
public func expChipAttributes(baseColor: PlatformColor?) -> [NSAttributedString.Key: Any] {
    var attrs: [NSAttributedString.Key: Any] = [
        .markdownChip: true,
        .foregroundColor: MarkdownStyle.linkColor,
    ]
    if let baseColor, baseColor != MarkdownStyle.linkColor {
        attrs[.markdownChipBaseColor] = baseColor
    }
    return attrs
}

/// Inline issue references (`#MET-115`) — the same interchange form as the web
/// (`apps/web/src/lib/issue-refs.ts`): plain GFM text, typeable by hand, and
/// rendered as a tappable pill ONLY when the identifier resolves to an issue in
/// the local store. Unresolved tokens stay plain text.
public enum IssueRefs {
    /// Mirrors the web `ISSUE_REF_SOURCE`: `#` not glued to a word or another
    /// `#`, identifier = `{PREFIX}-{number}`, ending at a token boundary.
    public static let pattern = "(?<![\\w#])#([A-Za-z][A-Za-z0-9]*-\\d+)(?![\\w-])"

    /// EXP-760 — mirrors the web `ISSUE_REF_BARE_SOURCE`: the STEERING feeds
    /// also chip identifiers written WITHOUT the `#` (agents narrate
    /// `EXP-758`). Display-only and opt-in: the bare alternative needs an
    /// UPPERCASE prefix (`utf-8` / `x86-64` / `exp-758` never match) and must
    /// not be glued to a word, a `#` or a `-` (`foo-EXP-1` stays text, while
    /// `exp/EXP-758` chips). Group 1 is the `#` form, group 2 the bare one.
    /// Never used for stored text: extraction and auto-relations keep `#`.
    public static let barePattern =
        "(?:(?<![\\w#])#([A-Za-z][A-Za-z0-9]*-\\d+)|(?<![\\w#-])([A-Z][A-Z0-9]*-\\d+))(?![\\w-])"

    // NSRegularExpression is Sendable + documented thread-safe for matching.
    private static let regex = try! NSRegularExpression(pattern: pattern)
    private static let bareRegex = try! NSRegularExpression(pattern: barePattern)

    private static func matcher(bare: Bool) -> NSRegularExpression {
        bare ? bareRegex : regex
    }

    /// The identifier group that actually fired: `#`-form first, bare second.
    /// The non-bare regex has a single group, hence the arity guard.
    private static func identifierRange(_ match: NSTextCheckingResult) -> NSRange {
        let hashed = match.range(at: 1)
        if hashed.location != NSNotFound { return hashed }
        guard match.numberOfRanges > 2 else { return hashed }
        return match.range(at: 2)
    }

    public struct Match: Sendable {
        /// Full token range (includes the leading `#`) in NSString UTF-16 units.
        public let range: NSRange
        /// Uppercase-normalized identifier, e.g. `MET-115`.
        public let identifier: String
        /// The raw matched text including the leading `#`, original case. Carried
        /// from the masked string (which the range was produced on) so callers
        /// never re-substring a different string with this range.
        public let token: String
        /// True when the token was written WITHOUT a `#` (bare mode only).
        /// Callers that paint over the `#` cell must skip these — there is no
        /// `#` to hide, only the prefix's first letter.
        public let isBare: Bool
    }

    /// All `#IDENTIFIER` tokens in `text`, skipping fenced code blocks and
    /// inline code spans (mirrors how the web only decorates non-code text).
    /// `bare: true` additionally matches bare `EXP-758` tokens (EXP-760,
    /// steering feeds only — see `barePattern`).
    public static func matches(in text: String, bare: Bool = false) -> [Match] {
        let ns = text as NSString
        guard ns.length > 0 else { return [] }
        // Cheap bail-out for the `#`-only contract; bare tokens carry no
        // sentinel character, so the scan always runs in bare mode.
        if !bare, ns.range(of: "#").location == NSNotFound { return [] }
        let masked = expMaskCodeRegions(text)
        let maskedNS = masked as NSString
        // Fail-safe width guard: masking is UTF-16-width preserving, so the
        // masked string's ranges are meant to map 1:1 onto the original. If a
        // masking bug ever broke that invariant, applying a masked range to the
        // original text would raise an uncatchable NSRangeException. Rather than
        // risk it, decorate nothing — refs render as plain text, never a crash.
        guard maskedNS.length == ns.length else { return [] }
        return matcher(bare: bare)
            .matches(in: masked, range: NSRange(location: 0, length: maskedNS.length))
            .compactMap {
                let idRange = identifierRange($0)
                guard idRange.location != NSNotFound else { return nil }
                // Substring on the SAME NSString the range came from. Matched
                // characters are never masked (masked chars are spaces, which
                // can't be part of a #ID-n match), so the masked token equals
                // the original token, case intact.
                let token = maskedNS.substring(with: $0.range)
                return Match(
                    range: $0.range,
                    identifier: maskedNS.substring(with: idRange).uppercased(),
                    token: token,
                    isBare: !token.hasPrefix("#")
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
        resolver: (String) -> String?,
        statusResolver: ((String) -> IssueRefStatusInfo?)? = nil
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
            let status = statusResolver?(identifier)
            var chipAttrs = expChipAttributes(baseColor: attrs[.foregroundColor] as? PlatformColor)
            chipAttrs[.markdownIssueRef] = issueId
            if let status { chipAttrs[.markdownIssueRefStatus] = status }
            // Issue chips override the shared chip foreground to the muted
            // token color (Linear look, EXP-423 cross-client parity) — the
            // base color for `unchip` was captured above, BEFORE the override,
            // so idempotence is unaffected. Mentions keep `linkColor`.
            chipAttrs[.foregroundColor] = MarkdownStyle.chipTokenColor
            let target = mutable ?? NSMutableAttributedString(attributedString: attributed)
            target.addAttributes(chipAttrs, range: match.range)
            if status != nil {
                // Hide the `#` UNDER the status glyph the layout manager paints
                // in its cell: a color change, so zero characters move and the
                // serializer, caret math and `chipAtomRange` are untouched by
                // construction (EXP-423). Deliberate divergence — web and the
                // desktop editor keep their `#` visible next to the icon, for
                // edit affordance and offset-map safety.
                let hashRange = NSRange(location: match.range.location, length: 1)
                target.addAttribute(.foregroundColor, value: PlatformColor.clear, range: hashRange)
                // Widen that one cell so the 13pt glyph painted in it no longer
                // crowds the identifier (EXP-655). Kerning is advance-only, so
                // still no character moves.
                target.addAttribute(
                    .kern, value: MarkdownStyle.chipStatusIconGap, range: hashRange)
            }
            mutable = target
        }
        return mutable ?? attributed
    }

    /// EXP-307: like `decorate`, but for READ-ONLY display models — replaces a
    /// resolved token's text with `#ID <title>` so the chip shows the whole
    /// issue title next to the short code (web/Android read-view parity). This
    /// CHANGES the character content, so it must never run on an editable
    /// model whose markdown gets serialized — edit paths reseed from the raw
    /// stored markdown and use `decorate` instead.
    ///
    /// `bare: true` (EXP-760) also chips bare `EXP-758` tokens — the steering
    /// feed's display models only, never an editable one.
    public static func decorateForDisplay(
        _ attributed: NSAttributedString,
        resolver: (String) -> String?,
        titleResolver: (String) -> String?,
        statusResolver: ((String) -> IssueRefStatusInfo?)? = nil,
        bare: Bool = false
    ) -> NSAttributedString {
        guard attributed.length > 0 else { return attributed }
        let ns = attributed.string as NSString
        let found = matcher(bare: bare)
            .matches(in: attributed.string, range: NSRange(location: 0, length: ns.length))
        guard !found.isEmpty else { return attributed }

        var mutable: NSMutableAttributedString?
        // Replacements grow the string, so walk back-to-front to keep every
        // earlier match range valid.
        for match in found.reversed() {
            // Same guards as `decorate`: one attribute run, never code/links,
            // and skip styled spans (consistent pill rules across both paths).
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
            // Already decorated (this model was re-decorated when the member
            // list synced): the token's title text is ALREADY spliced in, and
            // splicing again would duplicate it — `decorateForDisplay` replaces
            // characters, so unlike `decorate` it is only idempotent with this
            // guard (EXP-322).
            if attrs[.markdownIssueRef] != nil { continue }
            let idRange = identifierRange(match)
            guard idRange.location != NSNotFound else { continue }
            let identifier = ns.substring(with: idRange).uppercased()
            guard let issueId = resolver(identifier) else { continue }
            let token = ns.substring(with: match.range)
            // A bare token (EXP-760) has no `#` cell to paint a glyph over —
            // hiding its first character would eat the prefix's first letter.
            let hasHash = token.hasPrefix("#")
            let title = titleResolver(identifier).map(chipTitle) ?? ""
            let display = title.isEmpty ? token : "\(token) \(title)"
            var chipAttrs = attrs
            for (key, value) in expChipAttributes(baseColor: attrs[.foregroundColor] as? PlatformColor) {
                chipAttrs[key] = value
            }
            chipAttrs[.markdownIssueRef] = issueId
            let status = hasHash ? statusResolver?(identifier) : nil
            if let status { chipAttrs[.markdownIssueRefStatus] = status }
            // Linear look (EXP-423): muted token, foreground title — the same
            // split web/Android/desktop ship. Mentions keep `linkColor`.
            chipAttrs[.foregroundColor] = MarkdownStyle.chipTokenColor
            let piece = NSMutableAttributedString(string: display, attributes: chipAttrs)
            if !title.isEmpty {
                let tokenLength = (token as NSString).length
                piece.addAttribute(
                    .foregroundColor,
                    value: MarkdownStyle.textColor,
                    range: NSRange(location: tokenLength, length: piece.length - tokenLength),
                )
            }
            if status != nil {
                // Same hidden `#` (EXP-423) and the same kerned gap under the
                // glyph (EXP-655) as the editable path.
                let hashRange = NSRange(location: 0, length: 1)
                piece.addAttribute(.foregroundColor, value: PlatformColor.clear, range: hashRange)
                piece.addAttribute(
                    .kern, value: MarkdownStyle.chipStatusIconGap, range: hashRange)
            }
            let target = mutable ?? NSMutableAttributedString(attributedString: attributed)
            target.replaceCharacters(in: match.range, with: piece)
            mutable = target
        }
        return mutable ?? attributed
    }

    /// Keep chips readable (web parity: `MAX_CHIP_TITLE_LENGTH` = 60 chars,
    /// ellipsis beyond). The full title is one tap away — the chip opens the
    /// issue.
    public static func chipTitle(_ title: String) -> String {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count > 60 else { return trimmed }
        let cut = String(trimmed.prefix(59)).trimmingCharacters(in: .whitespaces)
        return "\(cut)…"
    }

    /// Display-only markdown transform for read-mode renderers (e.g. the iOS
    /// comment `Markdown` view): wraps resolved tokens as
    /// `[#ID](<scheme>://<issueId>)` links, skipping code. NEVER persisted —
    /// edit paths always reseed from the raw stored markdown.
    /// `bare: true` (EXP-760) also links bare `EXP-758` tokens.
    public static func linkifyForDisplay(
        _ markdown: String,
        scheme: String = "exp-issue",
        resolver: (String) -> String?,
        bare: Bool = false
    ) -> String {
        let found = matches(in: markdown, bare: bare)
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

    /// Spaces matching `s`'s UTF-16 width (keeps NSRange alignment).
    private static func blank(_ s: String) -> String {
        String(repeating: " ", count: s.utf16.count)
    }
}

/// Mask fenced code blocks and inline code spans with spaces so a token regex
/// can't match inside them. UTF-16-width preserving (each character is replaced
/// by as many spaces as its UTF-16 length), so the returned string's NSRange
/// indices map 1:1 onto the original text. Space is not a token character, so
/// masking can't manufacture new matches either. Shared by `IssueRefs` and
/// `MentionRefs` so the two token kinds agree on what "inside code" means.
func expMaskCodeRegions(_ text: String) -> String {
    func blank(_ s: String) -> String { String(repeating: " ", count: s.utf16.count) }
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
