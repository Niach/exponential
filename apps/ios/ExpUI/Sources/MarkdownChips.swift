import Foundation
import UIKit

extension NSAttributedString.Key {
    /// Marks the display-only chip-title attachment character; the value is the
    /// rendered title (String), which doubles as the idempotency key.
    public static let markdownIssueRefTitle = NSAttributedString.Key("exp.markdownIssueRefTitle")
}

/// The issue title drawn after a resolved `#IDENTIFIER`, as a single
/// `NSTextAttachment` character (U+FFFC).
///
/// Why an attachment and not injected text: `MarkdownConversion`'s serializer
/// already skips `.attachment` runs, so this contributes ZERO bytes to the
/// derived markdown with no serializer change — the round trip stays
/// byte-identical (web achieves the same with a CSS `::after`, which is also
/// not part of the document). And U+FFFC is indivisible, so the caret can never
/// sit inside the title, arrow keys and taps step over it, and a partial delete
/// is impossible. Injecting real characters would instead make the title's
/// exclusion from the markdown depend on an attribute surviving
/// `typingAttributes` inheritance — a data-loss failure mode.
public final class IssueRefTitleAttachment: NSTextAttachment {
    public let title: String

    private static let gap: CGFloat = 5
    private static let horizontalSlack: CGFloat = 6

    public init(title: String) {
        self.title = title
        super.init(data: nil, ofType: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    /// Equality by title so `NSAttributedString.isEqual` can tell a re-decorated
    /// document from a changed one — the decorator's `changed` flag depends on
    /// it (two freshly built attachments are otherwise never equal, and the
    /// editor would rewrite its storage on every keystroke forever).
    public override func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? IssueRefTitleAttachment else { return false }
        return other.title == title
    }

    public override var hash: Int { title.hashValue }

    private var titleFont: PlatformFont { MarkdownStyle.bodyFont }

    // Foreground title next to the muted token (Linear look, EXP-423) —
    // width measurement shares these attributes, so drawn and measured text
    // stay consistent.
    private var textAttributes: [NSAttributedString.Key: Any] {
        [.font: titleFont, .foregroundColor: MarkdownStyle.textColor]
    }

    public override func attachmentBounds(
        for textContainer: NSTextContainer?,
        proposedLineFragment lineFrag: CGRect,
        glyphPosition position: CGPoint,
        characterIndex charIndex: Int
    ) -> CGRect {
        let natural = (title as NSString).size(withAttributes: textAttributes).width
            + Self.gap + Self.horizontalSlack
        // Unlike web's `::after` (which can wrap), one attachment is a single
        // unbreakable box — so clamp it to the space actually left on the line
        // and let `image(forBounds:)` tail-truncate. Deliberate mobile
        // divergence, documented in EXP-322.
        let available = lineFrag.width - position.x
        // With no usable space left on this line the attachment wraps to its
        // own line, where the natural width can still overflow the container —
        // TextKit then lays the whole chip out full-width. Clamp to the
        // container's usable width so a too-long title truncates there too
        // (EXP-423).
        let containerWidth = (textContainer?.size.width ?? lineFrag.width)
            - 2 * (textContainer?.lineFragmentPadding ?? 0)
        let width = available > Self.gap
            ? min(natural, available)
            : min(natural, max(containerWidth, Self.gap))
        let font = titleFont
        return CGRect(x: 0, y: font.descender, width: ceil(width), height: ceil(font.lineHeight))
    }

    public override func image(
        forBounds imageBounds: CGRect,
        textContainer: NSTextContainer?,
        characterIndex charIndex: Int
    ) -> PlatformImage? {
        let key = CacheKey(title: title, width: imageBounds.width.rounded(), height: imageBounds.height.rounded())
        if let cached = Self.cache.object(forKey: key) { return cached }

        let attributes = textAttributes
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byTruncatingTail
        var drawAttributes = attributes
        drawAttributes[.paragraphStyle] = paragraph

        let renderer = UIGraphicsImageRenderer(size: imageBounds.size)
        let image = renderer.image { _ in
            // The box is descender-aligned, so the glyph origin inside it is
            // the box top; only the leading gap needs to be applied here.
            let textRect = CGRect(
                x: Self.gap,
                y: 0,
                width: max(0, imageBounds.width - Self.gap),
                height: imageBounds.height
            )
            (title as NSString).draw(in: textRect, withAttributes: drawAttributes)
        }
        Self.cache.setObject(image, forKey: key)
        return image
    }

    // The layout manager asks for this image on every draw pass, so memoize.
    private final class CacheKey: NSObject {
        let title: String
        let width: CGFloat
        let height: CGFloat
        init(title: String, width: CGFloat, height: CGFloat) {
            self.title = title
            self.width = width
            self.height = height
        }
        override func isEqual(_ object: Any?) -> Bool {
            guard let other = object as? CacheKey else { return false }
            return other.title == title && other.width == width && other.height == height
        }
        override var hash: Int {
            var hasher = Hasher()
            hasher.combine(title)
            hasher.combine(width)
            hasher.combine(height)
            return hasher.finalize()
        }
    }

    private nonisolated(unsafe) static let cache: NSCache<CacheKey, PlatformImage> = {
        let cache = NSCache<CacheKey, PlatformImage>()
        cache.countLimit = 128
        return cache
    }()
}

/// One idempotent decoration pass over an editable block: strip the previous
/// chips, re-detect resolved `#IDENTIFIER` / `@email` tokens, and re-insert the
/// display-only title attachments (EXP-322).
///
/// Callers run this after every text change, so it must be *idempotent* —
/// running it twice reports `changed == false` and produces the same string —
/// and it must never alter the characters the serializer sees.
public enum MarkdownChipDecorator {

    public struct Result {
        public let attributed: NSAttributedString
        /// `selection` mapped through the attachment insertions/removals.
        public let selection: NSRange
        /// False ⇒ the caller must not touch its text storage.
        public let changed: Bool
    }

    public static func decorate(
        _ input: NSAttributedString,
        selection: NSRange = NSRange(location: 0, length: 0),
        issueRefResolver: ((String) -> String?)?,
        issueRefTitleResolver: ((String) -> String?)? = nil,
        issueRefStatusResolver: ((String) -> IssueRefStatusInfo?)? = nil,
        mentionResolver: ((String) -> String?)? = nil
    ) -> Result {
        guard input.length > 0 else {
            return Result(attributed: input, selection: selection, changed: false)
        }
        var caret = selection

        // 1. Remove the previous title attachments, so detection runs against
        //    the real document text and the pass is idempotent.
        let stripped = stripTitles(input, caret: &caret)
        // 2. Drop the previous chip attributes (restoring the base color), so a
        //    token that stopped resolving goes back to plain text.
        var next: NSAttributedString = unchip(stripped)
        // 3. Re-chip.
        if let issueRefResolver {
            next = IssueRefs.decorate(
                next, resolver: issueRefResolver, statusResolver: issueRefStatusResolver)
        }
        if let mentionResolver {
            next = MentionRefs.decorate(next, resolver: mentionResolver)
        }
        // 4. Re-insert the title attachments.
        if issueRefResolver != nil, let issueRefTitleResolver {
            next = insertTitles(next, titleResolver: issueRefTitleResolver, caret: &caret)
        }

        return Result(attributed: next, selection: caret, changed: !next.isEqual(to: input))
    }

    /// Typing attributes with every chip key removed and the foreground
    /// restored, so the NEXT character the user types is never chip-styled.
    /// UITextView recomputes typing attributes on selection changes too, which
    /// is why callers must apply this from both the decoration pass and
    /// `textViewDidChangeSelection`.
    public static func sanitizedTypingAttributes(
        _ attrs: [NSAttributedString.Key: Any]
    ) -> [NSAttributedString.Key: Any] {
        guard attrs[.markdownChip] != nil || attrs[.attachment] != nil else { return attrs }
        var out = attrs
        let base = out[.markdownChipBaseColor] as? PlatformColor
        out[.markdownChip] = nil
        out[.markdownChipBaseColor] = nil
        out[.markdownIssueRef] = nil
        out[.markdownIssueRefTitle] = nil
        out[.markdownIssueRefStatus] = nil
        out[.markdownMention] = nil
        out[.attachment] = nil
        out[.backgroundColor] = nil
        // The status glyph's cell gap must not ride along into typed text
        // (EXP-655).
        out[.kern] = nil
        out[.foregroundColor] = base ?? MarkdownStyle.textColor
        return out
    }

    /// The `#IDENTIFIER` token range whose chip covers `location`, together
    /// with its title attachment — the unit the editor deletes as one atom.
    /// Backspacing at a chip's right edge would otherwise delete only the
    /// attachment, which the next pass immediately re-inserts.
    public static func chipAtomRange(in text: NSAttributedString, endingAt location: Int) -> NSRange? {
        guard location > 0, location <= text.length else { return nil }
        let attachmentIndex = location - 1
        guard text.attribute(.markdownIssueRefTitle, at: attachmentIndex, effectiveRange: nil) != nil else {
            return nil
        }
        var tokenRange = NSRange(location: 0, length: 0)
        // longestEffectiveRange, not effectiveRange: the hidden `#` of a chip
        // with a status glyph carries a different `.foregroundColor` than the
        // rest of its token (EXP-423), and an implementation-defined effective
        // range could stop at that boundary — the atom must always start at the
        // `#`, or a backspace would leave one behind.
        guard attachmentIndex > 0,
              text.attribute(
                  .markdownIssueRef,
                  at: attachmentIndex - 1,
                  longestEffectiveRange: &tokenRange,
                  in: NSRange(location: 0, length: text.length)
              ) != nil
        else {
            return NSRange(location: attachmentIndex, length: 1)
        }
        return NSRange(location: tokenRange.location, length: location - tokenRange.location)
    }

    /// The caret position `proposed` collapses to, so a chip behaves like one
    /// atom to the caret as well as to backspace (EXP-655). A collapsed caret
    /// may rest before the `#`, at the seam between identifier and title only
    /// when the decorator put it there, or after the whole atom — never
    /// strictly inside `#IDENTIFIER`.
    ///
    /// [previous] is the last collapsed caret: an adjacent one gives the
    /// direction of travel, anything else is a tap and falls out the nearer
    /// side. [allowSeam] is set for programmatic moves (the post-decoration
    /// caret sits at the seam on purpose, so typing keeps extending the token).
    public static func snappedCaret(
        in text: NSAttributedString,
        proposed: Int,
        previous: Int?,
        allowSeam: Bool
    ) -> Int {
        guard proposed > 0, proposed < text.length else { return proposed }
        var run = NSRange(location: 0, length: 0)
        guard text.attribute(
            .markdownIssueRef,
            at: proposed,
            longestEffectiveRange: &run,
            in: NSRange(location: 0, length: text.length)
        ) != nil else { return proposed }
        // A caret AT the run's start is outside the pill (the char before it is
        // not part of the token); only a caret with token text on both sides is
        // inside.
        guard run.location < proposed else { return proposed }
        let atomEnd = NSMaxRange(run)
        // The display-only title attachment is not part of the identifier, so
        // the seam in front of it is a legal programmatic resting place.
        var tokenEnd = atomEnd
        text.enumerateAttribute(.markdownIssueRefTitle, in: run, options: []) { value, range, stop in
            if value != nil {
                tokenEnd = range.location
                stop.pointee = true
            }
        }
        let inside = proposed < tokenEnd || (proposed == tokenEnd && !allowSeam)
        guard inside else { return proposed }
        // Only a one-character step carries a direction (arrow keys, the
        // system's caret nudges): a tap can land anywhere and must fall out
        // the nearer side, whatever the caret did before.
        if let previous, abs(proposed - previous) == 1 {
            return previous < proposed ? atomEnd : run.location
        }
        return proposed - run.location <= tokenEnd - proposed ? run.location : atomEnd
    }

    // MARK: - Passes

    private static func stripTitles(
        _ attributed: NSAttributedString,
        caret: inout NSRange
    ) -> NSAttributedString {
        var ranges: [NSRange] = []
        attributed.enumerateAttribute(
            .markdownIssueRefTitle,
            in: NSRange(location: 0, length: attributed.length),
            options: []
        ) { value, range, _ in
            if value != nil { ranges.append(range) }
        }
        guard !ranges.isEmpty else { return attributed }
        let mutable = NSMutableAttributedString(attributedString: attributed)
        for range in ranges.reversed() {
            mutable.replaceCharacters(in: range, with: "")
            caret = shift(caret, at: range.location, by: -range.length)
        }
        return mutable
    }

    private static func unchip(_ attributed: NSAttributedString) -> NSAttributedString {
        var ranges: [(NSRange, PlatformColor?)] = []
        attributed.enumerateAttribute(
            .markdownChip,
            in: NSRange(location: 0, length: attributed.length),
            options: []
        ) { value, range, _ in
            guard value != nil else { return }
            let base = attributed.attribute(.markdownChipBaseColor, at: range.location, effectiveRange: nil)
            ranges.append((range, base as? PlatformColor))
        }
        guard !ranges.isEmpty else { return attributed }
        let mutable = NSMutableAttributedString(attributedString: attributed)
        for (range, base) in ranges {
            mutable.removeAttribute(.markdownChip, range: range)
            mutable.removeAttribute(.markdownChipBaseColor, range: range)
            mutable.removeAttribute(.markdownIssueRef, range: range)
            // The status glyph goes with the chip — and so does the CLEAR
            // foreground it hid the `#` behind, which the base-color restore
            // below undoes over the whole range (EXP-423).
            mutable.removeAttribute(.markdownIssueRefStatus, range: range)
            // And the `#` cell's kerned gap (EXP-655). REQUIRED for
            // idempotence: a leftover `.kern` splits the token into two
            // attribute runs, `IssueRefs.decorate`'s one-run guard then refuses
            // to re-chip, and the pass would report `changed` on every
            // keystroke.
            mutable.removeAttribute(.kern, range: range)
            mutable.removeAttribute(.markdownMention, range: range)
            mutable.removeAttribute(.backgroundColor, range: range)
            mutable.addAttribute(.foregroundColor, value: base ?? MarkdownStyle.textColor, range: range)
        }
        return mutable
    }

    private static func insertTitles(
        _ attributed: NSAttributedString,
        titleResolver: (String) -> String?,
        caret: inout NSRange
    ) -> NSAttributedString {
        let ns = attributed.string as NSString
        let full = NSRange(location: 0, length: attributed.length)
        var insertions: [(location: Int, attrs: [NSAttributedString.Key: Any], title: String)] = []
        // Each enumerated range is expanded to the attribute's longest effective
        // range and the ones it swallows are skipped: a chip with a status glyph
        // hides its `#` behind a different `.foregroundColor` (EXP-423), so the
        // token must never be read as starting after it.
        var handledEnd = 0
        attributed.enumerateAttribute(.markdownIssueRef, in: full, options: []) { value, enumerated, _ in
            guard value != nil, enumerated.location >= handledEnd else { return }
            var range = NSRange(location: 0, length: 0)
            guard attributed.attribute(
                .markdownIssueRef,
                at: enumerated.location,
                longestEffectiveRange: &range,
                in: full
            ) != nil else { return }
            handledEnd = NSMaxRange(range)
            guard range.length > 1 else { return }
            // Inherit from the token's LAST character: the first is the `#`,
            // whose foreground is CLEAR when a status glyph is painted over it
            // (EXP-423) — the title must not inherit that.
            var attrs = attributed.attributes(at: NSMaxRange(range) - 1, effectiveRange: nil)
            // Belt and braces with `IssueRefs.decorate`'s own guard: a verbatim
            // pipe-table run is re-emitted from its SOURCE STRING, so an
            // attachment character inserted into it reaches the saved markdown
            // (EXP-322). Never insert a title inside one.
            guard attrs[.markdownTableBlock] == nil else { return }
            let identifier = ns.substring(with: NSRange(location: range.location + 1, length: range.length - 1))
                .uppercased()
            guard let raw = titleResolver(identifier) else { return }
            let title = IssueRefs.chipTitle(raw)
            guard !title.isEmpty else { return }
            attrs[.markdownIssueRefTitle] = title
            attrs[.attachment] = IssueRefTitleAttachment(title: title)
            insertions.append((NSMaxRange(range), attrs, title))
        }
        guard !insertions.isEmpty else { return attributed }
        let mutable = NSMutableAttributedString(attributedString: attributed)
        for insertion in insertions.reversed() {
            let piece = NSAttributedString(string: "\u{FFFC}", attributes: insertion.attrs)
            mutable.insert(piece, at: insertion.location)
            caret = shift(caret, at: insertion.location, by: piece.length)
        }
        return mutable
    }

    /// A caret moves for an edit at `location` only when it sits strictly after
    /// it. So a caret AT a token's end stays before the title just inserted
    /// there and typing keeps extending the token — web-identical.
    private static func shift(_ range: NSRange, at location: Int, by delta: Int) -> NSRange {
        var out = range
        if out.location > location {
            out.location = max(location, out.location + delta)
        } else if NSMaxRange(out) > location {
            out.length = max(0, out.length + delta)
        }
        return out
    }
}

extension NSAttributedString {
    /// `isEqual` typed for readability at the one call site that needs it.
    fileprivate func isEqual(to other: NSAttributedString) -> Bool { isEqual(other) }
}
