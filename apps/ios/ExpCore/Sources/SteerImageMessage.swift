import Foundation

/// EXP-511: steer messages carry attached images as markdown embeds. The host
/// device localizes each embed to a file path before the agent sees it, and the
/// activity echo restores the embed, so this exact shape is load-bearing across
/// web (`lib/steer-image-message.ts`), iOS and Android (`SteerImageMessage.kt`)
/// — keep the three builders byte-identical.
public enum SteerImageMessage {
    /// How many images one steer message may carry (MAX_STEER_IMAGES).
    public static let maxImages = 4

    /// EXP-698: a POSITIONAL reference to one of the message's images. The
    /// composer drops `[Image #k]` at the caret when the k-th image is
    /// attached, so the agent reads "crop [Image #2]" instead of guessing
    /// which embed a sentence means. The marker is plain text on the wire —
    /// the embeds below the text stay the only image payload — and the viewer
    /// renders it as a chip.
    /// `[0-9]` and not `\d`: ICU's `\d` also matches non-ASCII digits (Arabic-
    /// Indic, Devanagari, …), and JS `\d` does not — a marker written with
    /// those would chip here and stay prose on web.
    public static let imageMarkerPattern = "\\[Image #([0-9]+)\\]"

    public static func imageMarker(_ index: Int) -> String {
        "[Image #\(index)]"
    }

    /// One embed line, exactly as `build` writes it.
    private static let embedLinePattern = "^!\\[image\\]\\(/api/attachments/([^)\\s]+)\\)$"

    /// JS `String.prototype.trim()` — every Unicode whitespace character AND
    /// line terminators. `.whitespaces` alone leaves a `\r` behind, which is
    /// exactly the byte a CRLF-normalized transcript hands back.
    private static func jsTrim(_ text: String) -> String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static let markerRegex = try! NSRegularExpression(pattern: imageMarkerPattern)
    private static let embedLineRegex = try! NSRegularExpression(pattern: embedLinePattern)

    public static func build(text: String, attachmentIds: [String]) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if attachmentIds.isEmpty { return trimmed }
        let embeds = attachmentIds
            .map { "![image](/api/attachments/\($0))" }
            .joined(separator: "\n")
        if trimmed.isEmpty { return embeds }
        return "\(trimmed)\n\n\(embeds)"
    }

    /// The inverse of `build`: the prose without its trailing embed block, the
    /// attachment ids in embed order (image #1 is `attachmentIds[0]`), and the
    /// `[Image #N]` numbers the prose carries — 1-based, in text order,
    /// deduped. A number with no matching embed is still reported; the viewer
    /// decides what to do with a dangling reference.
    public struct Parsed: Equatable, Sendable {
        public let text: String
        public let attachmentIds: [String]
        public let markers: [Int]

        public init(text: String, attachmentIds: [String], markers: [Int]) {
            self.text = text
            self.attachmentIds = attachmentIds
            self.markers = markers
        }
    }

    public static func parse(_ message: String) -> Parsed {
        var lines = message.components(separatedBy: "\n")
        var end = lines.count
        while end > 0, jsTrim(lines[end - 1]).isEmpty { end -= 1 }
        var attachmentIds: [String] = []
        while end > 0 {
            let line = jsTrim(lines[end - 1])
            guard let id = embedAttachmentId(line) else { break }
            attachmentIds.insert(id, at: 0)
            end -= 1
        }
        lines = Array(lines.prefix(end))
        let text = trimmingTrailingWhitespace(lines.joined(separator: "\n"))
        return Parsed(text: text, attachmentIds: attachmentIds, markers: markers(in: text))
    }

    /// The `[Image #N]` numbers a draft carries, 1-based, in text order, deduped.
    public static func markers(in text: String) -> [Int] {
        var found: [Int] = []
        let ns = text as NSString
        markerRegex.enumerateMatches(in: text, range: NSRange(location: 0, length: ns.length)) {
            match, _, _ in
            // A number too big for `Int` is not a marker anyone can resolve —
            // it is prose, and skipping it here is what leaves it as prose in
            // `segments` too (the run is re-emitted as text).
            guard let match, let index = Int(ns.substring(with: match.range(at: 1))) else { return }
            if !found.contains(index) { found.append(index) }
        }
        return found
    }

    /// The prose split on its `[Image #N]` markers, in order — what a viewer
    /// walks to render each marker as a chip inline with the words around it.
    /// Empty text runs are dropped; the marker numbers are NOT deduped here
    /// (each occurrence is its own chip).
    public enum Segment: Equatable, Sendable {
        case text(String)
        case marker(Int)
    }

    public static func segments(of text: String) -> [Segment] {
        var result: [Segment] = []
        var cursor = 0
        let ns = text as NSString
        markerRegex.enumerateMatches(in: text, range: NSRange(location: 0, length: ns.length)) {
            match, _, _ in
            // Overflow = prose: leaving `cursor` where it is re-emits the
            // literal `[Image #…]` in the next text run.
            guard let match, let index = Int(ns.substring(with: match.range(at: 1))) else { return }
            if match.range.location > cursor {
                result.append(.text(ns.substring(with: NSRange(
                    location: cursor, length: match.range.location - cursor
                ))))
            }
            result.append(.marker(index))
            cursor = match.range.location + match.range.length
        }
        if cursor < ns.length { result.append(.text(ns.substring(from: cursor))) }
        return result
    }

    /// Drops `[Image #index]` at `caret`, space-separated from whatever it
    /// lands against. Returns the new draft and the caret behind the insertion.
    /// Offsets are UTF-16 code units, matching what `UITextView`/`NSString`
    /// hand back (web counts UTF-16 too).
    public static func insertImageMarker(
        text: String, caret: Int, index: Int
    ) -> (text: String, caret: Int) {
        let ns = text as NSString
        let at = max(0, min(caret, ns.length))
        let before = ns.substring(to: at)
        let after = ns.substring(from: at)
        let marker = imageMarker(index)
        let lead = !before.isEmpty && !endsWithWhitespace(before) ? " " : ""
        let trail = !after.isEmpty && !startsWithWhitespace(after) ? " " : ""
        return (
            text: "\(before)\(lead)\(marker)\(trail)\(after)",
            caret: at + (lead as NSString).length + (marker as NSString).length
                + (trail as NSString).length
        )
    }

    /// Removing the `removedIndex`-th pending image renumbers the draft: its
    /// own markers go, and every higher one slides down one. Only a line that
    /// LOST a marker gets the gap it left tidied — untouched lines keep their
    /// spacing.
    public static func renumberImageMarkers(_ text: String, removedIndex: Int) -> String {
        text.components(separatedBy: "\n").map { line in
            var dropped = false
            var next = ""
            var cursor = 0
            let ns = line as NSString
            markerRegex.enumerateMatches(in: line, range: NSRange(location: 0, length: ns.length)) {
                match, _, _ in
                guard let match else { return }
                next += ns.substring(with: NSRange(
                    location: cursor, length: match.range.location - cursor
                ))
                cursor = match.range.location + match.range.length
                // A number that does not fit `Int` is prose (see `markers`),
                // so it is copied through untouched rather than renumbered.
                guard let raw = Int(ns.substring(with: match.range(at: 1))) else {
                    next += ns.substring(with: match.range)
                    return
                }
                if raw == removedIndex {
                    dropped = true
                } else if raw > removedIndex {
                    next += imageMarker(raw - 1)
                } else {
                    next += ns.substring(with: match.range)
                }
            }
            next += ns.substring(from: cursor)
            guard dropped else { return next }
            var tidied = collapseRuns(next)
            tidied = trimmingTrailingSpacesAndTabs(tidied)
            if line.hasPrefix(imageMarker(removedIndex)) {
                tidied = trimmingLeadingSpacesAndTabs(tidied)
            }
            return tidied
        }
        .joined(separator: "\n")
    }

    // MARK: - String helpers (kept private so the contract stays one surface)

    private static func embedAttachmentId(_ line: String) -> String? {
        let ns = line as NSString
        guard let match = embedLineRegex.firstMatch(
            in: line, range: NSRange(location: 0, length: ns.length)
        ) else { return nil }
        return ns.substring(with: match.range(at: 1))
    }

    /// JS `trimEnd()` — every trailing whitespace character, newlines included.
    private static func trimmingTrailingWhitespace(_ text: String) -> String {
        var result = Substring(text)
        while let last = result.last, last.isWhitespace { result = result.dropLast() }
        return String(result)
    }

    private static func endsWithWhitespace(_ text: String) -> Bool {
        text.last.map { $0.isWhitespace } ?? false
    }

    private static func startsWithWhitespace(_ text: String) -> Bool {
        text.first.map { $0.isWhitespace } ?? false
    }

    /// `/[ \t]{2,}/g → " "` — a run of two or more spaces/tabs becomes ONE
    /// space; a lone space or tab is left exactly as it is.
    private static func collapseRuns(_ text: String) -> String {
        var result = ""
        var run = ""
        func flush() {
            result += run.count >= 2 ? " " : run
            run = ""
        }
        for character in text {
            if character == " " || character == "\t" {
                run.append(character)
                continue
            }
            flush()
            result.append(character)
        }
        flush()
        return result
    }

    private static func trimmingTrailingSpacesAndTabs(_ text: String) -> String {
        var result = Substring(text)
        while let last = result.last, last == " " || last == "\t" { result = result.dropLast() }
        return String(result)
    }

    private static func trimmingLeadingSpacesAndTabs(_ text: String) -> String {
        var result = Substring(text)
        while let first = result.first, first == " " || first == "\t" { result = result.dropFirst() }
        return String(result)
    }
}
