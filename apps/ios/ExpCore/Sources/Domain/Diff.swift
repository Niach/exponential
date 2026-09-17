import Foundation

/// EXP-895 — the ONE diff model every client renders, and the ONE parser that
/// builds it. Hand-mirrored ×4 (TS `@exp/domain-contract` `diff.ts`, desktop
/// `domain::diff`, Android `domain/Diff.kt`, iOS here) and byte-locked by
/// `packages/domain-contract/fixtures/diff/cases.json` +
/// `fixtures/diff/summary.json`, which every client's test replays.
///
/// Three producers feed the same model:
///   1. `git diff` output from the desktop worktree (`diff --git` sections),
///   2. the steer relay's per-call tool diffs (bare `--- a/x` / `+++ b/x`
///      sections with no `diff --git` line, optionally cut with a trailing
///      `\ N more lines truncated` marker),
///   3. GitHub's PullFile `patch` (hunks only; the path and status arrive
///      beside it, never inside it) — `parsePatch` / `Status.fromPullFile`.
///
/// The parse rules below ARE the contract; a change here is a change on all
/// four clients. The projection `render` is what the fixture freezes, so every
/// platform can compare one array of strings instead of a whole object graph.
///
/// Everything here is pure Foundation — no regex engine: JavaScript's `$`,
/// `\s` and `\d` do not mean what ICU means by them, and the parser must match
/// the TS byte for byte. Text is walked as UNICODE SCALARS, never as
/// `Character`s: Swift folds `\r\n` into ONE grapheme, and the TS splits on
/// `\n` alone and keeps the `\r` in the line.
public enum Diff {
    // ── Model ───────────────────────────────────────────────────────────────

    /// What happened to a file. GitHub's `changed`/`unchanged` and anything a
    /// future API adds fold into `modified`.
    public enum Status: String, Codable, Sendable {
        case added
        case removed
        case modified
        case renamed
        case copied

        /// GitHub's PullFile status vocabulary → ours.
        public static func fromPullFile(_ raw: String) -> Status {
            Status(rawValue: raw) ?? .modified
        }
    }

    /// A row's role inside a hunk. `meta` is unified-diff metadata (the
    /// `\ No newline at end of file` marker), numbered on neither side.
    public enum LineKind: String, Codable, Sendable {
        case add
        case del
        case context
        case meta
    }

    public struct Line: Equatable, Sendable {
        public let kind: LineKind
        /// 1-based line number on the old side; absent on `add`/`meta`.
        public let oldNo: Int?
        /// 1-based line number on the new side; absent on `del`/`meta`.
        public let newNo: Int?
        /// The line's content, its one-character sign stripped.
        public let text: String

        public init(kind: LineKind, oldNo: Int? = nil, newNo: Int? = nil, text: String) {
            self.kind = kind
            self.oldNo = oldNo
            self.newNo = newNo
            self.text = text
        }
    }

    public struct Hunk: Equatable, Sendable {
        public let oldStart: Int
        public let oldLines: Int
        public let newStart: Int
        public let newLines: Int
        /// The verbatim `@@ … @@` line, section heading and all.
        public let header: String
        public var lines: [Line]

        public init(
            oldStart: Int,
            oldLines: Int,
            newStart: Int,
            newLines: Int,
            header: String,
            lines: [Line] = []
        ) {
            self.oldStart = oldStart
            self.oldLines = oldLines
            self.newStart = newStart
            self.newLines = newLines
            self.header = header
            self.lines = lines
        }
    }

    public struct File: Equatable, Sendable, Identifiable {
        public var path: String
        /// Only on `renamed`/`copied`: where the file came from.
        public var previousPath: String?
        public var status: Status
        public var additions: Int
        public var deletions: Int
        public var binary: Bool
        public var hunks: [Hunk]

        public var id: String { path }

        public init(
            path: String,
            previousPath: String? = nil,
            status: Status = .modified,
            additions: Int = 0,
            deletions: Int = 0,
            binary: Bool = false,
            hunks: [Hunk] = []
        ) {
            self.path = path
            self.previousPath = previousPath
            self.status = status
            self.additions = additions
            self.deletions = deletions
            self.binary = binary
            self.hunks = hunks
        }
    }

    /// One parsed diff. (The TS calls this `Diff`; nested here it would shadow
    /// the namespace, so it is `Diff.Parsed`.)
    public struct Parsed: Equatable, Sendable {
        public var files: [File]
        /// Lines the PUBLISHER dropped, read back off its `\ N more lines
        /// truncated` marker (EXP-786). Absent when the diff is whole.
        public var truncatedLines: Int?

        public init(files: [File], truncatedLines: Int? = nil) {
            self.files = files
            self.truncatedLines = truncatedLines
        }
    }

    /// Every line number and count saturates here (i32::MAX) — the natives
    /// carry 32-bit counters and a hostile `@@` header must never wrap one.
    public static let lineMax = 2147483647

    // ── Path precedence ─────────────────────────────────────────────────────
    //
    // A higher-ranked source overwrites a lower-ranked one, and never the other
    // way round. `rename to`/`copy to` name the destination outright, `+++` is
    // the new side, `---` the old side (a fallback for a diff that never
    // reaches its `+++`), `diff --git`'s b-side is the last resort because a
    // path with a space makes that line ambiguous.
    private static let rankNone = -1
    private static let rankDiffGit = 0
    private static let rankOld = 1
    private static let rankNew = 2
    private static let rankRename = 3

    /// A file under construction: the model plus the rank of whatever named its
    /// path. A class so the loop below can hold one mutable cursor, exactly like
    /// the TS object it mirrors.
    private final class Building {
        var path: String
        var previousPath: String?
        var status: Status
        var additions = 0
        var deletions = 0
        var binary = false
        var hunks: [Hunk] = []
        var pathRank = Diff.rankNone

        init(path: String = "", status: Status = .modified) {
            self.path = path
            self.status = status
        }

        func setPath(_ path: String, _ rank: Int) {
            guard rank >= pathRank else { return }
            self.path = path
            pathRank = rank
        }

        func sealed() -> File {
            File(
                path: path,
                previousPath: previousPath,
                status: status,
                additions: additions,
                deletions: deletions,
                binary: binary,
                hunks: hunks
            )
        }
    }

    // ── Scalar helpers ──────────────────────────────────────────────────────

    /// Split on `\n` ONLY — never `components(separatedBy: .newlines)`, which
    /// also cuts on `\r`, `\u{2028}` and friends. A `\r` stays in the line, as
    /// it does in the TS.
    private static func splitLines(_ text: String) -> [String] {
        var out: [String] = []
        var current = String.UnicodeScalarView()
        for scalar in text.unicodeScalars {
            if scalar == "\n" {
                out.append(String(current))
                current = String.UnicodeScalarView()
            } else {
                current.append(scalar)
            }
        }
        out.append(String(current))
        return out
    }

    /// `String.hasPrefix` compares by canonically equivalent GRAPHEMES; the TS
    /// compares UTF-16 code units. Scalars are the faithful middle ground.
    private static func starts(_ s: String, _ prefix: String) -> Bool {
        s.unicodeScalars.starts(with: prefix.unicodeScalars)
    }

    /// `String.prototype.slice(n)` over scalars (never `dropFirst`, which drops
    /// whole grapheme clusters).
    private static func dropFirst(_ s: String, _ n: Int) -> String {
        String(String.UnicodeScalarView(s.unicodeScalars.dropFirst(n)))
    }

    private static func scalarCount(_ s: String) -> Int {
        s.unicodeScalars.count
    }

    private static func string(_ scalars: ArraySlice<Unicode.Scalar>) -> String {
        String(String.UnicodeScalarView(scalars))
    }

    /// JavaScript's `\s` (and therefore `String.prototype.trim`): the ECMAScript
    /// WhiteSpace ∪ LineTerminator set, which is NOT `CharacterSet.whitespaces`.
    private static func isSpace(_ s: Unicode.Scalar) -> Bool {
        switch s.value {
        case 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x20, 0xA0, 0x1680, 0x2028, 0x2029,
             0x202F, 0x205F, 0x3000, 0xFEFF:
            return true
        default:
            return s.value >= 0x2000 && s.value <= 0x200A
        }
    }

    private static func trim(_ s: String) -> String {
        let scalars = Array(s.unicodeScalars)
        var start = 0
        var end = scalars.count
        while start < end, isSpace(scalars[start]) { start += 1 }
        while end > start, isSpace(scalars[end - 1]) { end -= 1 }
        return string(scalars[start..<end])
    }

    private static func isDigit(_ s: Unicode.Scalar) -> Bool {
        s.value >= 0x30 && s.value <= 0x39
    }

    /// `Number(digits)` then the TS `clamp`: a run of ASCII digits, saturated at
    /// `lineMax`. Parsed as `Double` deliberately — it is exact for everything
    /// at or below the ceiling, and an absurdly long run overflows to infinity,
    /// which is by definition past the ceiling (never zero), so no path can
    /// trap or wrap.
    private static func clampDigits(_ digits: String) -> Int {
        guard let n = Double(digits), n.isFinite else { return lineMax }
        return clamp(n)
    }

    private static func clamp(_ n: Double) -> Int {
        if !n.isFinite || n <= 0 { return 0 }
        return n > Double(lineMax) ? lineMax : Int(n.rounded(.down))
    }

    /// Advance a 1-based line counter, saturating rather than wrapping.
    private static func step(_ n: Int) -> Int {
        n >= lineMax ? lineMax : n + 1
    }

    private static func matchPrefix(_ s: [Unicode.Scalar], _ i: inout Int, _ needle: String) -> Bool {
        let n = Array(needle.unicodeScalars)
        guard i + n.count <= s.count else { return false }
        for k in 0..<n.count where s[i + k] != n[k] { return false }
        i += n.count
        return true
    }

    private static func takeDigits(_ s: [Unicode.Scalar], _ i: inout Int) -> String? {
        let start = i
        while i < s.count, isDigit(s[i]) { i += 1 }
        return i > start ? string(s[start..<i]) : nil
    }

    // ── Header parsing ──────────────────────────────────────────────────────

    /// A `---`/`+++`/`diff --git` payload → a display path: drop ONE trailing
    /// `\r` (a CRLF-framed patch, split on `\n` alone), cut at the first TAB
    /// (GNU diff's timestamp column), then unwrap surrounding double quotes (git
    /// quotes a path carrying control or non-ASCII bytes). The `a/`/`b/` prefix
    /// is stripped by `stripAb` — only where git actually writes one.
    private static func cutPath(_ raw: String) -> String {
        var scalars = Array(raw.unicodeScalars)
        if scalars.last == "\r" { scalars.removeLast() }
        if let tab = scalars.firstIndex(of: "\t") { scalars = Array(scalars[..<tab]) }
        if scalars.count >= 2, scalars.first == "\"", scalars.last == "\"" {
            scalars = Array(scalars[1..<(scalars.count - 1)])
        }
        return string(scalars[...])
    }

    /// Drop the one `a/`/`b/` prefix git puts on `---`, `+++` and `diff --git`
    /// paths. NOT applied to `rename from`/`rename to`/`copy from`/`copy to`,
    /// which git writes bare — stripping there would eat a real top-level `a/`
    /// directory.
    private static func stripAb(_ s: String) -> String {
        starts(s, "a/") || starts(s, "b/") ? dropFirst(s, 2) : s
    }

    /// The `"…" "…"` form of a `diff --git` line, i.e. the TS regex
    /// `^(?:"(?:[^"]*)"|\S+)\s+"([^"]*)"$` — the b-side capture, or nil.
    private static func quotedPairTail(_ rest: String) -> String? {
        let s = Array(rest.unicodeScalars)
        guard s.count >= 2, s.last == "\"" else { return nil }
        // `[^"]*` cannot span a quote, so the opening quote of the LAST token is
        // the last `"` before the closing one.
        var open = s.count - 2
        while open >= 0, s[open] != "\"" { open -= 1 }
        guard open >= 0 else { return nil }
        // `\s+` between the two tokens, then the first token is all that is left.
        var head = open
        while head > 0, isSpace(s[head - 1]) { head -= 1 }
        guard head < open else { return nil }
        let first = s[0..<head]
        let quotedFirst =
            first.count >= 2 && first.first == "\"" && first.last == "\""
                && !first.dropFirst().dropLast().contains("\"")
        let bareFirst = !first.isEmpty && !first.contains(where: isSpace)
        guard quotedFirst || bareFirst else { return nil }
        return string(s[(open + 1)..<(s.count - 1)])
    }

    /// The b-side of a `diff --git <a> <b>` line. Quoted pairs parse exactly;
    /// otherwise the last ` b/` wins (git's own ambiguity — an unquoted path
    /// with a space cannot be split reliably, which is why this is the lowest
    /// rank).
    private static func diffGitNewPath(_ rest: String) -> String? {
        if let quoted = quotedPairTail(rest) { return stripAb(quoted) }
        let s = Array(rest.unicodeScalars)
        let needle: [Unicode.Scalar] = [" ", "b", "/"]
        if s.count >= needle.count {
            var at = s.count - needle.count
            while at >= 0 {
                if s[at] == needle[0], s[at + 1] == needle[1], s[at + 2] == needle[2] {
                    return string(s[(at + 3)...])
                }
                at -= 1
            }
        }
        let trimmed = trim(rest)
        return trimmed.isEmpty ? nil : stripAb(trimmed)
    }

    private struct HunkHead {
        let oldStart: Int
        let oldLines: Int
        let newStart: Int
        let newLines: Int
    }

    /// `^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@`, hand-rolled. Trailing
    /// content (git's section heading) is ignored, and a `,` with no digits
    /// behind it fails the whole header, exactly as the regex backtracks.
    private static func parseHunkHeader(_ raw: String) -> HunkHead? {
        let s = Array(raw.unicodeScalars)
        var i = 0
        guard matchPrefix(s, &i, "@@ -"), let oldStart = takeDigits(s, &i) else { return nil }
        // A count the header omits is 1 — `@@ -1 +1 @@` is one line each side.
        var oldLines = 1
        if i < s.count, s[i] == "," {
            i += 1
            guard let digits = takeDigits(s, &i) else { return nil }
            oldLines = clampDigits(digits)
        }
        guard matchPrefix(s, &i, " +"), let newStart = takeDigits(s, &i) else { return nil }
        var newLines = 1
        if i < s.count, s[i] == "," {
            i += 1
            guard let digits = takeDigits(s, &i) else { return nil }
            newLines = clampDigits(digits)
        }
        guard matchPrefix(s, &i, " @@") else { return nil }
        return HunkHead(
            oldStart: clampDigits(oldStart),
            oldLines: oldLines,
            newStart: clampDigits(newStart),
            newLines: newLines
        )
    }

    /// The one marker the steer relay appends to a cut patch (EXP-786); web
    /// `splitTruncatedDiff` and desktop `truncated_marker_count` spell it the
    /// same way. The TS regex is
    /// `/(?:^|\n)\\ (\d+) more lines? truncated\s*$/`, anchored to the END of
    /// the text: only a TRAILING marker counts. Matched backwards here because
    /// ICU's `$` — unlike JavaScript's — also matches before a final line
    /// terminator.
    ///
    /// Returns the cut index (the JS match index, in scalars) and the count.
    private static func truncationCut(_ s: [Unicode.Scalar]) -> (index: Int, lines: Int)? {
        var i = s.count
        // `\s*$`
        while i > 0, isSpace(s[i - 1]) { i -= 1 }
        guard endsWith(s, i, " truncated") else { return nil }
        i -= scalarCount(" truncated")
        if endsWith(s, i, "lines") {
            i -= 5
        } else if endsWith(s, i, "line") {
            i -= 4
        } else {
            return nil
        }
        guard endsWith(s, i, " more ") else { return nil }
        i -= scalarCount(" more ")
        let digitsEnd = i
        while i > 0, isDigit(s[i - 1]) { i -= 1 }
        guard i < digitsEnd, endsWith(s, i, "\\ ") else { return nil }
        let digits = string(s[i..<digitsEnd])
        i -= 2 // the `\ ` itself: where the match begins.
        // `(?:^|\n)`: at the start of the text, or right after a newline (which
        // the match then swallows, so the cut lands before it).
        if i == 0 { return (0, clampDigits(digits)) }
        guard s[i - 1] == "\n" else { return nil }
        return (i - 1, clampDigits(digits))
    }

    private static func endsWith(_ s: [Unicode.Scalar], _ end: Int, _ needle: String) -> Bool {
        let n = Array(needle.unicodeScalars)
        guard end >= n.count else { return false }
        for k in 0..<n.count where s[end - n.count + k] != n[k] { return false }
        return true
    }

    // ── The parser ──────────────────────────────────────────────────────────

    /// The one state machine behind `parse` and `parsePatch`.
    ///
    /// `seed` = the caller already knows the path and status (a GitHub patch),
    /// so every header line is ignored (except the binary marker) and no
    /// `---`/`diff --git` line may ever open a second file.
    private static func parseSections(
        _ text: String,
        seed: (path: String, status: Status)? = nil
    ) -> [File] {
        var files: [File] = []
        var cur: Building? = seed.map { Building(path: $0.path, status: $0.status) }
        var hunkIndex: Int?
        var remOld = 0
        var remNew = 0
        var oldNo = 0
        var newNo = 0

        let lines = splitLines(text)
        for (i, raw) in lines.enumerated() {
            // 1. `diff --git` starts the next file unconditionally — even
            //    mid-hunk, where a truncated patch can leave us.
            if seed == nil, starts(raw, "diff --git ") {
                let file = Building()
                if let path = diffGitNewPath(dropFirst(raw, scalarCount("diff --git "))) {
                    file.setPath(path, rankDiffGit)
                }
                if let open = cur { files.append(open.sealed()) }
                cur = file
                hunkIndex = nil
                remOld = 0
                remNew = 0
                continue
            }

            // 2. A hunk header. Body lines always carry a sign, so a line
            //    literally starting with `@@` is unambiguous.
            if starts(raw, "@@") {
                // A `@@` line, and a new file, always end the hunk in progress.
                hunkIndex = nil
                remOld = 0
                remNew = 0
                guard let head = parseHunkHeader(raw) else { continue }
                let file = cur ?? Building()
                cur = file
                file.hunks.append(
                    Hunk(
                        oldStart: head.oldStart,
                        oldLines: head.oldLines,
                        newStart: head.newStart,
                        newLines: head.newLines,
                        header: raw
                    )
                )
                hunkIndex = file.hunks.count - 1
                remOld = head.oldLines
                remNew = head.newLines
                oldNo = head.oldStart
                newNo = head.newStart
                continue
            }

            // 3. `\ No newline at end of file`: unified-diff metadata. Kept as a
            //    row (a reader wants to see it), numbered on neither side, and it
            //    never consumes a count — so it may legally trail a hunk whose
            //    counts are already spent, as it does when BOTH sides lack the
            //    final newline.
            if let index = hunkIndex, let file = cur, starts(raw, "\\") {
                file.hunks[index].lines.append(
                    Line(kind: .meta, text: starts(raw, "\\ ") ? dropFirst(raw, 2) : dropFirst(raw, 1))
                )
                continue
            }

            // 4. The hunk body, bounded by the header's counts. Past them the
            //    hunk is over, whatever the next line looks like — that is what
            //    lets a bare steer diff start its next file on a plain `--- a/…`.
            //    Inside them a `--- ` line with a `+++ ` line right behind it
            //    (one-line lookahead) is STILL the next bare section's opener,
            //    not a deletion of `-- …`: a header whose counts overshoot its
            //    body must not swallow the file after it. A `---` body line
            //    followed by anything else stays a deletion.
            let bareOpener = seed == nil
                && starts(raw, "--- ")
                && i + 1 < lines.count
                && starts(lines[i + 1], "+++ ")
            if let index = hunkIndex, let file = cur, remOld > 0 || remNew > 0, !bareOpener {
                let sign = raw.unicodeScalars.first
                if sign == "+" {
                    file.additions += 1
                    file.hunks[index].lines.append(
                        Line(kind: .add, newNo: newNo, text: dropFirst(raw, 1))
                    )
                    newNo = step(newNo)
                    remNew -= 1
                    continue
                }
                if sign == "-" {
                    file.deletions += 1
                    file.hunks[index].lines.append(
                        Line(kind: .del, oldNo: oldNo, text: dropFirst(raw, 1))
                    )
                    oldNo = step(oldNo)
                    remOld -= 1
                    continue
                }
                // A context line is ` ` + content; a producer that trimmed
                // trailing whitespace emits the empty string for an empty context
                // line, and inside a hunk with counts left that is exactly what
                // it means.
                if sign == " " || raw.isEmpty {
                    file.hunks[index].lines.append(
                        Line(
                            kind: .context,
                            oldNo: oldNo,
                            newNo: newNo,
                            text: raw.isEmpty ? "" : dropFirst(raw, 1)
                        )
                    )
                    oldNo = step(oldNo)
                    newNo = step(newNo)
                    remOld -= 1
                    remNew -= 1
                    continue
                }
                // Anything else inside a hunk means the counts lied: end the hunk
                // and let the line be read as a header below.
                hunkIndex = nil
                remOld = 0
                remNew = 0
            }

            if seed != nil {
                if let file = cur, file.hunks.isEmpty,
                   starts(raw, "Binary files ") || starts(raw, "GIT binary patch") {
                    file.binary = true
                }
                continue
            }

            // 5. The header region. `---` is the one header line that may also
            //    OPEN a file: a bare steer section has no `diff --git` to
            //    announce it.
            if starts(raw, "--- ") || raw == "---" {
                if cur == nil || cur?.hunks.isEmpty == false {
                    if let open = cur { files.append(open.sealed()) }
                    cur = Building()
                    hunkIndex = nil
                    remOld = 0
                    remNew = 0
                }
                guard let file = cur else { continue }
                let payload = cutPath(scalarCount(raw) > 4 ? dropFirst(raw, 4) : "")
                if payload == "/dev/null" {
                    file.status = .added
                } else if !payload.isEmpty {
                    file.setPath(stripAb(payload), rankOld)
                }
                continue
            }
            guard let file = cur else { continue }
            // Everything below is honoured only BEFORE the first hunk of a file —
            // past it these words are just content that lost its sign.
            if !file.hunks.isEmpty { continue }
            if starts(raw, "+++ ") {
                let payload = cutPath(dropFirst(raw, 4))
                if payload == "/dev/null" {
                    file.status = .removed
                } else if !payload.isEmpty {
                    file.setPath(stripAb(payload), rankNew)
                }
            } else if starts(raw, "new file mode") {
                file.status = .added
            } else if starts(raw, "deleted file mode") {
                file.status = .removed
            } else if starts(raw, "rename from ") {
                file.previousPath = cutPath(dropFirst(raw, scalarCount("rename from ")))
                file.status = .renamed
            } else if starts(raw, "rename to ") {
                file.setPath(cutPath(dropFirst(raw, scalarCount("rename to "))), rankRename)
                file.status = .renamed
            } else if starts(raw, "copy from ") {
                file.previousPath = cutPath(dropFirst(raw, scalarCount("copy from ")))
                file.status = .copied
            } else if starts(raw, "copy to ") {
                file.setPath(cutPath(dropFirst(raw, scalarCount("copy to "))), rankRename)
                file.status = .copied
            } else if starts(raw, "Binary files ") || starts(raw, "GIT binary patch") {
                file.binary = true
            }
        }

        if let open = cur { files.append(open.sealed()) }
        return files
    }

    /// Read any of the three forms into the model, auto-detected:
    ///
    /// - a full `git diff` (`diff --git` sections),
    /// - bare steer sections that start straight at `--- a/x` / `+++ b/x`,
    /// - hunks-only text (the first non-blank line is a `@@` header) → ONE file
    ///   with an EMPTY path and status `modified`; a caller that knows the path
    ///   uses `parsePatch` instead.
    ///
    /// Garbage, whitespace and the empty string all yield no files.
    public static func parse(_ text: String) -> Parsed {
        if text.isEmpty { return Parsed(files: []) }
        var body = text
        var truncatedLines: Int?
        let scalars = Array(text.unicodeScalars)
        if let cut = truncationCut(scalars) {
            body = string(scalars[0..<cut.index])
            truncatedLines = cut.lines
        }
        // A section that named neither a path nor a hunk is noise, not a file.
        let files = parseSections(body).filter { file in
            !file.path.isEmpty || !file.hunks.isEmpty || file.binary
        }
        if files.isEmpty { return Parsed(files: []) }
        return Parsed(files: files, truncatedLines: truncatedLines)
    }

    /// A hunks-only patch whose path and status the CALLER knows (GitHub's
    /// PullFile, the desktop's per-file `git diff` wrappers). Nothing in `patch`
    /// may change either one. A missing or empty patch is a file with no hunks —
    /// binary, too large for GitHub to send, or a pure rename.
    public static func parsePatch(path: String, status: Status, patch: String?) -> File {
        guard let patch, !patch.isEmpty else {
            return File(path: path, status: status)
        }
        let files = parseSections(patch, seed: (path: path, status: status))
        return files.first ?? File(path: path, status: status)
    }

    /// One GitHub PullFile → one `File` (web `fromPullFile`; the fixture's
    /// `pullFile` form). An empty `previousFilename` is no previous path. When
    /// the patch carries no hunks (absent, empty, or a pure rename) GitHub's own
    /// counts are kept, clamped to `0...lineMax` — they are the only counts
    /// there are.
    public static func fromPullFile(
        filename: String,
        previousFilename: String?,
        status: String,
        additions: Int,
        deletions: Int,
        patch: String?
    ) -> File {
        var file = parsePatch(path: filename, status: Status.fromPullFile(status), patch: patch)
        if let previousFilename, !previousFilename.isEmpty {
            file.previousPath = previousFilename
        }
        if file.hunks.isEmpty {
            file.additions = min(max(0, additions), lineMax)
            file.deletions = min(max(0, deletions), lineMax)
        }
        return file
    }

    // ── Derivations ─────────────────────────────────────────────────────────

    public struct Totals: Equatable, Sendable {
        public let files: Int
        public let additions: Int
        public let deletions: Int

        public init(files: Int, additions: Int, deletions: Int) {
            self.files = files
            self.additions = additions
            self.deletions = deletions
        }
    }

    public static func totals(_ files: [File]) -> Totals {
        var additions = 0
        var deletions = 0
        for file in files {
            additions += file.additions
            deletions += file.deletions
        }
        return Totals(files: files.count, additions: additions, deletions: deletions)
    }

    /// Fold sections that name the SAME path into one file, in order of first
    /// appearance. A publisher may emit one section per edit, so the same file
    /// arrives several times in one transcript; the reader wants one card.
    /// Hunks concatenate in arrival order, counts sum, and the LATER section's
    /// status, binary flag and (when it has one) previousPath win.
    public static func mergeFilesByPath(_ files: [File]) -> [File] {
        var out: [File] = []
        var at: [String: Int] = [:]
        for file in files {
            guard let seen = at[file.path] else {
                at[file.path] = out.count
                out.append(file)
                continue
            }
            out[seen].hunks.append(contentsOf: file.hunks)
            out[seen].additions += file.additions
            out[seen].deletions += file.deletions
            out[seen].status = file.status
            out[seen].binary = file.binary
            if let previous = file.previousPath, !previous.isEmpty {
                out[seen].previousPath = previous
            }
        }
        return out
    }

    /// Unchanged lines above a file's FIRST hunk — the count a "show more"
    /// affordance offers to expand.
    public static func unchangedBefore(_ first: Hunk) -> Int {
        max(0, first.newStart - 1)
    }

    /// Unchanged lines between two consecutive hunks of one file.
    public static func unchangedBetween(_ prev: Hunk, _ next: Hunk) -> Int {
        max(0, next.newStart - (prev.newStart + prev.newLines))
    }

    public static func unchangedLabel(_ n: Int) -> String {
        "\(n) unchanged \(n == 1 ? "line" : "lines")"
    }

    public static func additionsLabel(_ n: Int) -> String {
        "+\(n)"
    }

    /// U+2212 MINUS SIGN, not a hyphen: the deletion count sits beside `+n` in a
    /// proportional font and a hyphen reads a full notch lighter.
    public static func deletionsLabel(_ n: Int) -> String {
        "\u{2212}\(n)"
    }

    public static func summaryLabel(files: Int, additions: Int, deletions: Int) -> String {
        if files == 0 { return DomainContract.diffUiNoChanges }
        let noun = files == 1 ? "file" : "files"
        return "\(files) \(noun) \(additionsLabel(additions)) \(deletionsLabel(deletions))"
    }

    /// The byte-lock projection: one string per row, the whole `Parsed`
    /// flattened. `fixtures/diff/cases.json` stores exactly this, so every
    /// platform compares `[String]` instead of reimplementing structural
    /// equality. Deliberately ASCII (an ASCII `-` for the deletion count, unlike
    /// `deletionsLabel`) and deliberately delimited (`|…|` around content) so
    /// trailing whitespace in a diff line survives the round trip.
    public static func render(_ parsed: Parsed) -> [String] {
        var out: [String] = []
        if let truncated = parsed.truncatedLines {
            out.append("truncated \(truncated)")
        }
        for file in parsed.files {
            var from = ""
            if let previous = file.previousPath, !previous.isEmpty { from = " <- \(previous)" }
            let binary = file.binary ? " binary" : ""
            out.append(
                "file \(file.status.rawValue) \(file.path)\(from)\(binary) +\(file.additions) -\(file.deletions)"
            )
            for hunk in file.hunks {
                out.append(
                    "hunk \(hunk.oldStart),\(hunk.oldLines) \(hunk.newStart),\(hunk.newLines) |\(hunk.header)|"
                )
                for line in hunk.lines {
                    let old = line.oldNo.map(String.init) ?? "-"
                    let next = line.newNo.map(String.init) ?? "-"
                    let tag: String
                    switch line.kind {
                    case .context: tag = "ctx"
                    case .add: tag = "add"
                    case .del: tag = "del"
                    case .meta: tag = "meta"
                    }
                    out.append("\(tag) \(old) \(next) |\(line.text)|")
                }
            }
        }
        return out
    }
}
