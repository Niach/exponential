import UIKit

// Bounds-checked NSRange / paragraph helpers shared by the editor and toolbar.
// Centralizes the clamping logic that used to be duplicated (and occasionally
// wrong) in MarkdownEditor.shouldChangeTextIn and MarkdownToolbar.paragraphRange.

extension NSString {
    /// The paragraph range for the caret at `location`, safe for empty strings
    /// and end-of-string carets (where `paragraphRange(for:)` can misbehave).
    /// Returns an empty range at `length` when the caret sits past a trailing
    /// newline so callers can treat it as "fresh paragraph, no attributes".
    func safeParagraphRange(at location: Int) -> NSRange {
        guard length > 0 else { return NSRange(location: 0, length: 0) }
        let clamped = max(0, min(location, length))
        if clamped >= length {
            let lastChar = character(at: length - 1)
            if lastChar == 0x0A || lastChar == 0x0D {
                return NSRange(location: length, length: 0)
            }
            return paragraphRange(for: NSRange(location: length - 1, length: 0))
        }
        return paragraphRange(for: NSRange(location: clamped, length: 0))
    }
}

extension NSAttributedString {
    /// Attributes at `location`, or `nil` if the location is out of bounds.
    func attributesIfInBounds(at location: Int) -> [NSAttributedString.Key: Any]? {
        guard location >= 0, location < length else { return nil }
        return attributes(at: location, effectiveRange: nil)
    }

    /// Substring for `range`, or `nil` if the range is out of bounds.
    func attributedSubstringIfInBounds(from range: NSRange) -> NSAttributedString? {
        guard range.location >= 0, NSMaxRange(range) <= length else { return nil }
        return attributedSubstring(from: range)
    }
}
