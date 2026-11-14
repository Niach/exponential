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

    /// Wave D (×4): one non-image attachment of a steer/start message — the
    /// upload's id and the server's sanitized `filename` (the link text).
    public struct File: Equatable, Sendable {
        public let id: String
        public let name: String

        public init(id: String, name: String) {
            self.id = id
            self.name = name
        }
    }

    /// How many non-image files one message may carry.
    public static let maxFiles = 4

    /// The ONE rejection copy ×4 for a pick over its cap (images 10 MB,
    /// files 50 MB).
    public static let attachmentRejectedMessage =
        "Images up to 10 MB and files up to 50 MB can be attached"

    /// The cap copy ×4 once a message already carries `maxFiles` files.
    public static let filesFullMessage = "Up to 4 files per message"

    /// The cap copy once a message already carries `maxImages` images
    /// (Android `STEER_IMAGES_CAP`).
    public static let imagesFullMessage = "Up to \(maxImages) images per message"

    /// A share handed more than one message takes (EXP-1635, the extension's
    /// composer): the cap it hit, images first; nil = everything fits.
    public static func capNote(images: Int, files: Int, moreFiles: Bool = false) -> String? {
        if images > maxImages { return imagesFullMessage }
        if moreFiles || files > maxFiles { return filesFullMessage }
        return nil
    }

    /// One file line: `[<name>](/api/attachments/<id>)`, never preceded by
    /// `!`. The name is backslash-escaped (`]` and `\`) on the way out.
    private static let fileLinePattern =
        "^\\[((?:[^\\]\\\\]|\\\\.)*)\\]\\(/api/attachments/([^)\\s]+)\\)$"
    private static let fileLineRegex = try! NSRegularExpression(pattern: fileLinePattern)

    /// `]` and `\` in a filename are backslash-escaped so the link text can
    /// never close early.
    public static func escapeFileName(_ name: String) -> String {
        var out = ""
        for character in name {
            if character == "\\" || character == "]" { out.append("\\") }
            out.append(character)
        }
        return out
    }

    private static func unescapeFileName(_ escaped: String) -> String {
        var out = ""
        var pending = false
        for character in escaped {
            if pending {
                out.append(character)
                pending = false
            } else if character == "\\" {
                pending = true
            } else {
                out.append(character)
            }
        }
        if pending { out.append("\\") }
        return out
    }

    /// Wave D (byte-identical ×4: web `buildSteerMessage`, desktop
    /// `domain::image_message`, Android `SteerImageMessage.kt`): the prose, a
    /// blank line, the image embed block exactly as `build` writes it, then
    /// one `[<filename>](/api/attachments/<id>)` line per file. Files carry
    /// no positional marker. With no files this IS `build`.
    public static func buildMessage(text: String, imageIds: [String], files: [File]) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let lines = imageIds.map { "![image](/api/attachments/\($0))" }
            + files.map { "[\(escapeFileName($0.name))](/api/attachments/\($0.id))" }
        if lines.isEmpty { return trimmed }
        let block = lines.joined(separator: "\n")
        if trimmed.isEmpty { return block }
        return "\(trimmed)\n\n\(block)"
    }

    /// The inverse of `build`: the prose without its trailing embed block, the
    /// attachment ids in embed order (image #1 is `attachmentIds[0]`), and the
    /// `[Image #N]` numbers the prose carries — 1-based, in text order,
    /// deduped. A number with no matching embed is still reported; the viewer
    /// decides what to do with a dangling reference. Wave D: `files` = the
    /// trailing file lines, peeled off BEFORE the image embeds.
    public struct Parsed: Equatable, Sendable {
        public let text: String
        public let attachmentIds: [String]
        public let markers: [Int]
        public let files: [File]

        public init(text: String, attachmentIds: [String], markers: [Int], files: [File] = []) {
            self.text = text
            self.attachmentIds = attachmentIds
            self.markers = markers
            self.files = files
        }
    }

    public static func parse(_ message: String) -> Parsed {
        var lines = message.components(separatedBy: "\n")
        var end = lines.count
        while end > 0, jsTrim(lines[end - 1]).isEmpty { end -= 1 }
        var files: [File] = []
        while end > 0 {
            let line = jsTrim(lines[end - 1])
            guard let file = fileLine(line) else { break }
            files.insert(file, at: 0)
            end -= 1
        }
        var attachmentIds: [String] = []
        while end > 0 {
            let line = jsTrim(lines[end - 1])
            guard let id = embedAttachmentId(line) else { break }
            attachmentIds.insert(id, at: 0)
            end -= 1
        }
        lines = Array(lines.prefix(end))
        let text = trimmingTrailingWhitespace(lines.joined(separator: "\n"))
        return Parsed(
            text: text, attachmentIds: attachmentIds, markers: markers(in: text), files: files
        )
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

    /// #EXP-1841: the marker edits live in `ComposerAttachments` (THE
    /// composer strip, every composer); these forward so there is ONE
    /// implementation.
    public static func insertImageMarker(
        text: String, caret: Int, index: Int
    ) -> (text: String, caret: Int) {
        ComposerAttachments.insertImageMarker(text: text, caret: caret, index: index)
    }

    public static func renumberImageMarkers(_ text: String, removedIndex: Int) -> String {
        ComposerAttachments.renumberImageMarkers(text, removedIndex: removedIndex)
    }

    // MARK: - String helpers (kept private so the contract stays one surface)

    private static func fileLine(_ line: String) -> File? {
        let ns = line as NSString
        guard let match = fileLineRegex.firstMatch(
            in: line, range: NSRange(location: 0, length: ns.length)
        ) else { return nil }
        return File(
            id: ns.substring(with: match.range(at: 2)),
            name: unescapeFileName(ns.substring(with: match.range(at: 1)))
        )
    }

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
}
