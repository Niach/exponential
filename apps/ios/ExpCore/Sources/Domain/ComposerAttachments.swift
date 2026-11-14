import Foundation

/// #EXP-1841 (fixtures/composer-attachments.json, model = pure
/// `src/composer-attachments.ts`): THE attachment strip of EVERY composer ×4
/// (a new chat's, a session's, an agent chat's, the Assistant's, an app
/// editor's) and its positional `[Image #N]` placeholders (EXP-698). Pure:
/// the strip is a list of kinds; classifying a picked file (type, size)
/// stays the platform's, so `add` arrives already classified.
/// The ONE implementation of the marker edits: `SteerImageMessage`'s
/// `insertImageMarker` / `renumberImageMarkers` forward here.
public enum ComposerAttachments {
    public enum Kind: String, Equatable, Sendable {
        case image
        case file
    }

    public enum Pick: String, Equatable, Sendable {
        case image
        case file
        case rejected
    }

    /// Images and files each cap at four per message.
    public static let maxImages = SteerImageMessage.maxImages
    public static let maxFiles = SteerImageMessage.maxFiles

    /// The rejection toasts ×4.
    public static let rejectedToast = SteerImageMessage.attachmentRejectedMessage
    public static let imageCapToast = SteerImageMessage.imagesFullMessage
    public static let fileCapToast = SteerImageMessage.filesFullMessage

    private static let markerRegex = try! NSRegularExpression(
        pattern: SteerImageMessage.imageMarkerPattern
    )

    public static func imageMarker(_ index: Int) -> String {
        SteerImageMessage.imageMarker(index)
    }

    // MARK: - insert

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

    // MARK: - renumber

    /// Removing the `removedIndex`-th image renumbers the draft: its own
    /// markers go, and every higher one slides down one. Only a line that
    /// LOST a marker gets the gap it left tidied; untouched lines keep their
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
                // A number that does not fit `Int` is prose (see
                // `SteerImageMessage.markers`), copied through untouched.
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

    // MARK: - stage

    public struct Staged: Equatable, Sendable {
        /// The strip after the add.
        public let strip: [Kind]
        /// For each `add` entry: true when it joined the strip.
        public let taken: [Bool]
        /// The draft with one marker per TAKEN image at the caret.
        public let text: String
        /// The caret behind the last inserted marker (unchanged without one).
        public let caret: Int
        /// Picks refused for size or type (`rejected`).
        public let rejected: Int
        /// Images over the image cap.
        public let overflow: Int
        /// Files over the file cap.
        public let fileOverflow: Int

        public init(
            strip: [Kind], taken: [Bool], text: String, caret: Int,
            rejected: Int, overflow: Int, fileOverflow: Int
        ) {
            self.strip = strip
            self.taken = taken
            self.text = text
            self.caret = caret
            self.rejected = rejected
            self.overflow = overflow
            self.fileOverflow = fileOverflow
        }

        /// The toasts this stage owes, in order.
        public var toasts: [String] {
            ComposerAttachments.toasts(rejected: rejected, overflow: overflow, fileOverflow: fileOverflow)
        }
    }

    public static func imageCount(_ strip: [Kind]) -> Int {
        strip.filter { $0 == .image }.count
    }

    /// Stages a pick onto the strip. The image count BEFORE the add is the
    /// numbering base: image #1 is the strip's first IMAGE (files carry no
    /// marker and never count).
    public static func stage(strip input: [Kind], add: [Pick], text: String, caret: Int) -> Staged {
        let base = imageCount(input)
        var imageRoom = max(0, maxImages - base)
        var fileRoom = max(0, maxFiles - (input.count - base))
        var rejected = 0
        var overflow = 0
        var fileOverflow = 0
        var strip = input
        var taken: [Bool] = []
        var draft = text
        var at = max(0, min(caret, (text as NSString).length))
        var number = base
        for kind in add {
            switch kind {
            case .rejected:
                rejected += 1
                taken.append(false)
            case .image:
                guard imageRoom > 0 else {
                    overflow += 1
                    taken.append(false)
                    continue
                }
                imageRoom -= 1
                strip.append(.image)
                taken.append(true)
                number += 1
                let inserted = insertImageMarker(text: draft, caret: at, index: number)
                draft = inserted.text
                at = inserted.caret
            case .file:
                guard fileRoom > 0 else {
                    fileOverflow += 1
                    taken.append(false)
                    continue
                }
                fileRoom -= 1
                strip.append(.file)
                taken.append(true)
            }
        }
        return Staged(
            strip: strip, taken: taken, text: draft, caret: at,
            rejected: rejected, overflow: overflow, fileOverflow: fileOverflow
        )
    }

    // MARK: - drop

    /// The strip's X. An image's markers leave the draft and every higher one
    /// slides down, so the numbers keep matching the strip; a file has none.
    /// An out-of-range position is a no-op.
    public static func drop(strip: [Kind], at: Int, text: String) -> (strip: [Kind], text: String) {
        guard strip.indices.contains(at) else { return (strip, text) }
        var next = strip
        let kind = next.remove(at: at)
        guard kind == .image else { return (next, text) }
        let number = imageCount(Array(strip.prefix(at))) + 1
        return (next, renumberImageMarkers(text, removedIndex: number))
    }

    // MARK: - toasts

    /// The toasts a stage owes, in order.
    public static func toasts(rejected: Int, overflow: Int, fileOverflow: Int) -> [String] {
        var toasts: [String] = []
        if rejected > 0 { toasts.append(rejectedToast) }
        if overflow > 0 { toasts.append(imageCapToast) }
        if fileOverflow > 0 { toasts.append(fileCapToast) }
        return toasts
    }

    // MARK: - String helpers

    private static func endsWithWhitespace(_ text: String) -> Bool {
        text.last.map { $0.isWhitespace } ?? false
    }

    private static func startsWithWhitespace(_ text: String) -> Bool {
        text.first.map { $0.isWhitespace } ?? false
    }

    /// `/[ \t]{2,}/g → " "`: a run of two or more spaces/tabs becomes ONE
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
