import Foundation

// EXP-1324: the CodeBlock's `ansi` language (the Terminal pattern: command
// output), mirrored from the eUI built-in tokenizer
// (`packages/exponential-ui/src/code.ts` `ansiRuns`/`tokenizeCode`, rule in
// `catalog/code.json` `$comment`). The app does not link the eUI core, so
// only this ONE language lives here; `AnsiCodeTests` replays the `ansi`
// cases of `packages/exponential-ui/fixtures/code-tokens.json`.
//
// Before scanning, every ANSI CSI escape is REMOVED and the text split into
// runs by SGR colour (31/91 error, 32/92 success, 33/93 warning, 2/90
// comment, 0/39/none reset, anything else keeps the colour; params apply
// left to right). A coloured run is that kind verbatim; an uncoloured run is
// scanned with the `ansi` spec, which has only `lineKinds`: at a line start
// (after spaces/tabs) the LONGEST listed prefix makes the rest of the line
// that kind, everything else is plain. Tokens split into lines and adjacent
// tokens of one kind merge.

public struct CodeToken: Equatable, Sendable {
    /// A contract `codeToken` kind (`plain`, `comment`, `success`, …).
    public let kind: String
    public let text: String

    public init(kind: String, text: String) {
        self.kind = kind
        self.text = text
    }
}

public enum AnsiCode {
    /// `catalog/code.json` `languages.ansi.lineKinds`.
    public static let lineKinds: [(prefix: String, kind: String)] = [
        ("$ ", "comment"),
        ("> ", "comment"),
        ("BUILD SUCCESSFUL", "success"),
        ("BUILD FAILED", "error"),
        ("PASS ", "success"),
        ("FAIL ", "error"),
        ("FAILED", "error"),
        ("(pass)", "success"),
        ("(fail)", "error"),
        ("test result: ok", "success"),
        ("test result: FAILED", "error"),
        ("error:", "error"),
        ("error[", "error"),
        ("Error:", "error"),
        ("warning:", "warning"),
        ("\u{2713}", "success"),
        ("\u{2714}", "success"),
        ("\u{2717}", "error"),
        ("\u{2718}", "error"),
    ]

    /// One run of the escape-free text with the SGR colour it was printed in.
    public struct Run: Equatable, Sendable {
        public let kind: String?
        public let text: String
    }

    private static func sgrKind(_ params: String, _ kind: String?) -> String? {
        var out = kind
        for raw in params.split(separator: ";", omittingEmptySubsequences: false) {
            let p = raw.isEmpty ? 0 : (Int(raw) ?? -1)
            switch p {
            case 0, 39: out = nil
            case 31, 91: out = "error"
            case 32, 92: out = "success"
            case 33, 93: out = "warning"
            case 2, 90: out = "comment"
            default: break
            }
        }
        return out
    }

    /// The source with every CSI escape removed, split into runs by colour.
    /// Adjacent runs of one colour merge; an incomplete escape stays text.
    public static func runs(_ source: String) -> [Run] {
        // UTF-16 units, like the TS reference's string indexing.
        let units = Array(source.utf16)
        let n = units.count
        var runs: [Run] = []
        var kind: String?
        var text: [UInt16] = []
        func flush() {
            guard !text.isEmpty else { return }
            let chunk = String(decoding: text, as: UTF16.self)
            if let last = runs.last, last.kind == kind {
                runs[runs.count - 1] = Run(kind: kind, text: last.text + chunk)
            } else {
                runs.append(Run(kind: kind, text: chunk))
            }
            text = []
        }
        var i = 0
        while i < n {
            if units[i] == 0x1B, i + 1 < n, units[i + 1] == 0x5B {
                var j = i + 2
                while j < n, (0x30...0x3F).contains(units[j]) { j += 1 }
                let params = String(decoding: units[(i + 2)..<j], as: UTF16.self)
                while j < n, (0x20...0x2F).contains(units[j]) { j += 1 }
                if j < n, (0x40...0x7E).contains(units[j]) {
                    if units[j] == 0x6D {
                        let next = sgrKind(params, kind)
                        if next != kind {
                            flush()
                            kind = next
                        }
                    }
                    i = j + 1
                    continue
                }
            }
            text.append(units[i])
            i += 1
        }
        flush()
        return runs
    }

    private static func normalize(_ source: String) -> String {
        source.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
    }

    /// The text the block shows and copies: CRLF → LF, escapes removed.
    public static func text(_ source: String) -> String {
        runs(normalize(source)).map(\.text).joined()
    }

    private static func isSpace(_ c: Character) -> Bool { c == " " || c == "\t" }
    private static func isSpaceUnit(_ unit: UInt16) -> Bool { unit == 0x20 || unit == 0x09 }

    /// An uncoloured run through the `ansi` scanner. `lineStart` = whether
    /// its first character starts a line.
    private static func scan(_ code: String, lineStart: Bool) -> [CodeToken] {
        var out: [CodeToken] = []
        // Lines keep their separators as plain tokens.
        let lines = code.split(separator: "\n", omittingEmptySubsequences: false)
        for (index, line) in lines.enumerated() {
            if index > 0 { out.append(CodeToken(kind: "plain", text: "\n")) }
            let atStart = index > 0 || lineStart
            // UTF-16 units, like `runs()` and the TS reference: Swift's
            // `String.hasPrefix` compares graphemes, so an emoji-presentation
            // `✔️` (U+2714 U+FE0F) would NOT start with `✔` while
            // `startsWith` (TS) and `starts_with` (Rust) say it does.
            let units = Array(line.utf16)
            var k = 0
            while k < units.count, isSpaceUnit(units[k]) { k += 1 }
            let restUnits = units[k...]
            if atStart, !restUnits.isEmpty,
               let hit = lineKinds.filter({ restUnits.starts(with: Array($0.prefix.utf16)) })
                   .max(by: { $0.prefix.utf16.count < $1.prefix.utf16.count }) {
                if k > 0 {
                    out.append(CodeToken(kind: "plain", text: String(decoding: units[..<k], as: UTF16.self)))
                }
                out.append(CodeToken(kind: hit.kind, text: String(decoding: restUnits, as: UTF16.self)))
            } else if !line.isEmpty {
                out.append(CodeToken(kind: "plain", text: String(line)))
            }
        }
        return out
    }

    /// The source tokenized per LINE: each line's tokens concatenate to its
    /// escape-free text; an empty line is an empty list.
    public static func tokenize(_ source: String) -> [[CodeToken]] {
        var flat: [CodeToken] = []
        var atLine = true
        for run in runs(normalize(source)) {
            if let kind = run.kind {
                flat.append(CodeToken(kind: kind, text: run.text))
            } else {
                flat.append(contentsOf: scan(run.text, lineStart: atLine))
            }
            // Only spaces/tabs since the last line break (or none at all and
            // the run started a line) keep the next run at a line start.
            let tail = run.text.split(separator: "\n", omittingEmptySubsequences: false).last ?? ""
            let onlySpaces = tail.allSatisfy(isSpace)
            atLine = run.text.contains("\n") ? onlySpaces : (atLine && onlySpaces)
        }
        var lines: [[CodeToken]] = [[]]
        for token in flat {
            let pieces = token.text.split(separator: "\n", omittingEmptySubsequences: false)
            for (index, piece) in pieces.enumerated() {
                if index > 0 { lines.append([]) }
                if piece.isEmpty { continue }
                if let last = lines[lines.count - 1].last, last.kind == token.kind {
                    lines[lines.count - 1][lines[lines.count - 1].count - 1] =
                        CodeToken(kind: last.kind, text: last.text + piece)
                } else {
                    lines[lines.count - 1].append(CodeToken(kind: token.kind, text: String(piece)))
                }
            }
        }
        return lines
    }
}
