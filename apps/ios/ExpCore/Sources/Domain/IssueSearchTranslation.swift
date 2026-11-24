import Foundation

/// #NEW-136: the MODEL FALLBACK of the issue search, its pure half. When the
/// phrase grammar (`IssueSearchPhrase`) does not recognise a sentence, iOS 26
/// and the desktop app offer `Translate to a search`: ONE short call to a
/// model on the user's own device or login (never typing, never our server).
/// This decides when the row shows (`wants`), builds the ONE prompt both
/// callers send (`prompt`) and turns the model's answer into a query the
/// engine accepts (`sanitize`: the output is never trusted). The call itself
/// is the app's `SearchTranslator` (Apple Foundation Models).
///
/// Hand-mirrored from the web `apps/web/src/lib/issue-search-translation.ts`
/// (the reference) and desktop `domain::issue_search_translation`, byte-locked
/// by `packages/domain-contract/fixtures/issue-search-translation.json` (its
/// `$comment` is the spec; read it before changing a rule).
public enum IssueSearchTranslation {
    /// The fixture's `limits`: the numbers every caller shares.
    public enum Limits {
        public static let minWords = 4
        public static let maxTextLength = 200
        public static let maxTokens = 32
        public static let maxWords = 8
        public static let maxWordLength = 40
        public static let maxNames = 40
        /// The call's hard deadline.
        public static let timeoutSeconds = 15
    }

    /// The instruction line the model gets beside the prompt.
    public static let system = "You turn sentences into issue search queries."

    /// The fixture's `hints`: the words that say a sentence names a TIME or
    /// a STATE. A date or a state the model answers without one is dropped.
    static let timeHints = "heute today gestern yesterday morgen tomorrow tag tage tagen day days woche wochen week weeks monat monate monaten month months jahr jahre jahren year years quartal quarter letzter letzten letzte letztem letztes last past previous vergangene vergangenen vergangener vergangenes diese dieser diesen diesem dieses this seit since ab after nach vor before bis until ago her recently lately neulich kürzlich zuletzt letztens januar january februar february märz march april mai may juni june juli july august september oktober october november dezember december montag monday dienstag tuesday mittwoch wednesday donnerstag thursday freitag friday samstag saturday sonntag sunday wochenende weekend old older oldest alt alte alten älter ältere älteste new newer newest neu neue neuen neuer neueste neuesten latest aktuell aktuelle aktuellen"
    static let stateHints = "open offen offene offenen offener offenes unresolved ungelöst ungelöste ungelösten unfinished unerledigt unerledigte unerledigten closed close geschlossen geschlossene geschlossenen resolved gelöst gelöste gelösten backlog done erledigt erledigte erledigten fertig fertige fertigen completed finished abgeschlossen abgeschlossene abgeschlossenen cancelled canceled abgebrochen abgebrochene abgebrochenen storniert stornierte stornierten duplicate duplicates duplikat duplikate duplikaten progress arbeit bearbeitung review still noch yet pending ausstehend todo fixed behoben gefixt started begonnen angefangen unstarted active aktiv aktive aktiven working wip shipped landed merged"
    private static let timeWords = Set(timeHints.split(separator: " ").map(String.init))
    private static let stateWords = Set(stateHints.split(separator: " ").map(String.init))

    private static let filterPrefixes = ["by:", "is:", "status:", "created:", "updated:", "similar:"]
    /// What a model wraps its answer in: quotes, backticks, emphasis.
    private static let wrap: Set<Character> = ["`", "\"", "'", "*", "“", "”", "„", "‘", "’", "«", "»"]
    private static let labels = ["search:", "query:"]
    private static let strip: Set<Character> = [".", ",", ";", ":", "!", "?", "\"", "'", "(", ")"]

    private static func trimming(_ text: Substring, _ drop: (Character) -> Bool) -> String {
        var piece = text
        while let first = piece.first, drop(first) { piece = piece.dropFirst() }
        while let last = piece.last, drop(last) { piece = piece.dropLast() }
        return String(piece)
    }

    private static func stripWrap(_ text: Substring) -> String {
        trimming(text) { wrap.contains($0) || $0.isWhitespace }
    }

    /// A token without its leading and trailing punctuation (the grammar's).
    private static func stripPunctuation(_ token: Substring) -> String {
        trimming(token) { strip.contains($0) }
    }

    /// Whether `value` is one word of letters and digits (the engine's words).
    private static func isWord(_ value: String) -> Bool {
        IssueSearch.words(value) == [value]
    }

    /// One line of the model's answer without its fence, quotes and label.
    private static func cleanLine(_ line: String) -> String {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        // A code fence line (``` or ```text) is not the answer.
        if trimmed.hasPrefix("```"), !(trimmed.unicodeScalars.count > 3 && trimmed.hasSuffix("`")) {
            return ""
        }
        let bare = stripWrap(Substring(trimmed))
        for label in labels where bare.prefix(label.count).lowercased() == label {
            return stripWrap(bare.dropFirst(label.count))
        }
        return bare
    }

    /// Each member's name words and email local part, lowercased.
    private static func people(_ members: [IssueSearchPhrase.Member]) -> [(words: [String], local: String)] {
        members.map { member in
            let email = (member.email ?? "").lowercased()
            let local = email.firstIndex(of: "@").map { String(email[..<$0]) } ?? email
            return (IssueSearch.words((member.name ?? "").lowercased()), local)
        }
    }

    private static func add(_ list: inout [String], _ value: String) {
        if !list.contains(value) { list.append(value) }
    }

    /// Whether the global search offers `Translate to a search` for `text`:
    /// the text has `minWords` or more words, is at most `maxTextLength`
    /// long, carries no filter token, and the phrase grammar does not
    /// recognise it.
    public static func wants(_ text: String, members: [IssueSearchPhrase.Member] = [], now: Date = Date()) -> Bool {
        if text.trimmingCharacters(in: .whitespacesAndNewlines).unicodeScalars.count > Limits.maxTextLength {
            return false
        }
        var count = 0
        for raw in text.lowercased().split(whereSeparator: { $0.isWhitespace }) {
            let token = stripPunctuation(raw)
            if filterPrefixes.contains(where: { token.hasPrefix($0) }) { return false }
            if !IssueSearch.words(token).isEmpty { count += 1 }
        }
        return count >= Limits.minWords && IssueSearchPhrase.translate(text, members: members, now: now) == nil
    }

    /// The query a model's answer means, or nil. Only tokens the engine
    /// accepts AND the typed text backs survive (a person, a word, a time or
    /// a state the sentence does not name is the model's invention), in the
    /// phrase grammar's order (is:, status:, by:, the dates, the words); nil
    /// when nothing is left or nothing changed against `text`.
    public static func sanitize(
        _ output: String,
        text: String,
        members: [IssueSearchPhrase.Member] = [],
        now: Date = Date()
    ) -> String? {
        let people = people(members)
        func isMember(_ word: String) -> Bool {
            people.contains { $0.words.contains(word) || (!$0.local.isEmpty && $0.local == word) }
        }

        // What the typed text names: its words, a time, a state.
        let lowered = text.lowercased()
        let textWords = IssueSearch.words(lowered)
        let textTokens = lowered.split(whereSeparator: { $0.isWhitespace }).map { stripPunctuation($0) }
        let namesTime = textTokens.contains { token in
            token.unicodeScalars.contains { $0.value >= 48 && $0.value <= 57 } || timeWords.contains(token)
        }
        let namesState = textTokens.contains { stateWords.contains($0) }
        // A word of the answer is the text's when one starts with the other
        // (`crash` for `crashes`); a text word under three letters is no stem.
        func inText(_ token: String) -> Bool {
            let parts = IssueSearch.words(token)
            return !parts.isEmpty && parts.allSatisfy { word in
                textWords.contains { typed in
                    typed.hasPrefix(word) || (typed.unicodeScalars.count >= 3 && word.hasPrefix(typed))
                }
            }
        }

        // Split on the LINE FEED scalar: a Swift Character holds `\r\n` whole.
        var line = ""
        for raw in output.unicodeScalars.split(separator: "\n", omittingEmptySubsequences: false) {
            line = cleanLine(String(String.UnicodeScalarView(raw)))
            if !line.isEmpty { break }
        }

        var states: [String] = []
        var statuses: [String] = []
        var creators: [String] = []
        var dates: [String] = []
        var words: [String] = []
        func plain(_ token: String) {
            let word = stripPunctuation(Substring(token))
            if !inText(word) || word.unicodeScalars.count > Limits.maxWordLength { return }
            if words.count >= Limits.maxWords, !words.contains(word) { return }
            add(&words, word)
        }

        for token in IssueSearch.tokens(line).prefix(Limits.maxTokens) {
            if token.hasPrefix(IssueSearch.statePrefix) {
                let value = String(token.dropFirst(IssueSearch.statePrefix.count))
                // One state at most: two would never match together.
                if value == "open" || value == "closed", namesState, states.isEmpty { states.append(value) }
            } else if token.hasPrefix(IssueSearch.statusPrefix) {
                let value = String(token.dropFirst(IssueSearch.statusPrefix.count))
                let valid = isWord(value) && value.unicodeScalars.count <= Limits.maxWordLength
                if valid, namesState, statuses.isEmpty { statuses.append(value) }
            } else if token.hasPrefix(IssueSearch.creatorPrefix) {
                var value = String(token.dropFirst(IssueSearch.creatorPrefix.count))
                if value.hasPrefix("@") { value = String(value.dropFirst()) }
                // A name that is no member is a plain word, like in the grammar.
                if isMember(value), inText(value) {
                    add(&creators, value)
                } else if !value.contains(":") {
                    plain(value)
                }
            } else if token.hasPrefix(IssueSearch.createdPrefix) {
                if namesTime,
                   IssueSearch.parseRange(String(token.dropFirst(IssueSearch.createdPrefix.count)), now: now) != nil {
                    add(&dates, token)
                }
            } else if token.hasPrefix(IssueSearch.updatedPrefix) {
                if namesTime,
                   IssueSearch.parseRange(String(token.dropFirst(IssueSearch.updatedPrefix.count)), now: now) != nil {
                    add(&dates, token)
                }
            } else if !token.contains(":") {
                plain(token)
            }
        }

        let query = (
            states.map { IssueSearch.statePrefix + $0 }
                + statuses.map { IssueSearch.statusPrefix + $0 }
                + creators.map { IssueSearch.creatorPrefix + $0 }
                + dates
                + words
        ).joined(separator: " ")
        if query.isEmpty || query == IssueSearch.tokens(text).joined(separator: " ") { return nil }
        return query
    }

    /// The members' FIRST names (lowercased, no repeats, at most `maxNames`):
    /// all the prompt tells a model about the team. Never an email.
    private static func firstNames(_ members: [IssueSearchPhrase.Member]) -> [String] {
        var names: [String] = []
        for person in people(members) {
            guard let first = person.words.first else { continue }
            if first.unicodeScalars.count > Limits.maxWordLength || names.contains(first) { continue }
            if names.count >= Limits.maxNames { break }
            names.append(first)
        }
        return names
    }

    /// THE prompt (iOS and desktop send this exact string): the token syntax,
    /// the members' first names, today's UTC date, "one line, the query
    /// only", German and English examples, then the sentence (whitespace
    /// collapsed, capped).
    public static func prompt(_ text: String, members: [IssueSearchPhrase.Member] = [], now: Date = Date()) -> String {
        let nowMs = Int((now.timeIntervalSince1970 * 1000).rounded(.down))
        let today = IssueSearchPhrase.civilFromDays(
            IssueSearchPhrase.floorDiv(nowMs, Int(IssueSearch.daySeconds) * 1000)
        )
        let pad = IssueSearchPhrase.pad
        let names = firstNames(members)
        let collapsed = text.split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
        let sentence = String(String.UnicodeScalarView(collapsed.unicodeScalars.prefix(Limits.maxTextLength)))
        var lines = [
            "Translate the sentence into one issue search query.",
            "Tokens:",
            "is:open or is:closed, at most one",
            "status:<word>, at most one of backlog, progress, review, done, cancelled, duplicate",
        ]
        if !names.isEmpty {
            lines.append(
                "by:<name> for who created the issue, only when the sentence names one of \(names.joined(separator: ", "))"
            )
        }
        lines += [
            "created:<range> or updated:<range>, only when the sentence names a time; a range is 7d, 2w, 1m, 1y, today, yesterday, YYYY-MM-DD, YYYY-MM, >=YYYY-MM-DD, <YYYY-MM-DD or YYYY-MM-DD..YYYY-MM-DD",
            "any other word is a plain search word: keep only the words that name the topic, as the sentence spells them",
            "Use no other tokens. Never add a person, a status or a time the sentence does not name. Today is \(pad(today.year, 4))-\(pad(today.month, 2))-\(pad(today.day, 2)) (UTC).",
            "Answer with one line, the query only.",
            "Examples:",
            "offene Issues zum Editor aus letzter Woche -> is:open created:7d editor",
            "which crashes did we close since yesterday -> is:closed updated:yesterday crashes",
            "was wurde im letzten Monat zur Abrechnung erledigt -> status:done updated:1m abrechnung",
            "everything in review about the login page -> status:review login page",
            "why is the export to csv so slow -> export csv slow",
            "welche Fehler beim Drucken von Rechnungen sind noch offen -> is:open fehler drucken rechnungen",
        ]
        if let first = names.first {
            lines.append("what did \(first) file about sync this year -> by:\(first) created:1y sync")
        }
        lines.append("Sentence: \(sentence)")
        return lines.joined(separator: "\n")
    }
}
