import Foundation

/// #NEW-133: the natural-language PHRASE GRAMMAR of the issue search. A
/// sentence (`offene Issues von Lukas zum Editor aus letzter Woche`) becomes
/// the structured query the engine already runs (`is:open by:lukas created:7d
/// editor`, `IssueSearch`). Deterministic, no model, German + English.
/// Hand-mirrored from the web `apps/web/src/lib/issue-search-phrase.ts` (the
/// reference, which owns the WORD LISTS; Android `IssueSearchPhrase.kt`,
/// desktop `domain::issue_search_phrase`), byte-locked by
/// `packages/domain-contract/fixtures/issue-search-phrase.json` (its
/// `$comment` is the spec; read it before changing a rule).
public enum IssueSearchPhrase {
    /// A team member: a name after `von`/`by` that is one of them turns into
    /// `by:`.
    public struct Member: Sendable, Equatable {
        public let name: String?
        public let email: String?

        public init(name: String? = nil, email: String? = nil) {
            self.name = name
            self.email = email
        }
    }

    private static func set(_ words: String) -> Set<String> {
        Set(words.split(separator: " ").map(String.init))
    }

    private static let filterPrefixes = ["by:", "is:", "status:", "created:", "updated:", "similar:"]
    private static let strip: Set<Character> = [".", ",", ";", ":", "!", "?", "\"", "'", "(", ")"]

    private static let open = set(
        "open offen offene offenen offener offenes unresolved ungelöst ungelöste ungelösten unfinished unerledigt unerledigte unerledigten"
    )
    private static let closed = set("closed geschlossen geschlossene geschlossenen resolved gelöst gelöste gelösten")
    private static let statusWords: [(String, Set<String>)] = [
        ("backlog", set("backlog")),
        (
            "done",
            set(
                "done erledigt erledigte erledigten fertig fertige fertigen completed finished abgeschlossen abgeschlossene abgeschlossenen"
            )
        ),
        (
            "cancelled",
            set("cancelled canceled abgebrochen abgebrochene abgebrochenen storniert stornierte stornierten")
        ),
        ("duplicate", set("duplicate duplicates duplikat duplikate duplikaten")),
    ]
    /// Two-word statuses: `in progress`, `in arbeit`, `in review`.
    private static let statusPairs: [(String, String, String)] = [
        ("in", "progress", "progress"),
        ("in", "arbeit", "progress"),
        ("in", "bearbeitung", "progress"),
        ("in", "review", "review"),
        ("im", "review", "review"),
    ]
    private static let creatorLead = set("von vom by from")
    /// A creator lead-in that is dropped even when no member follows.
    private static let creatorDrop = set("von vom by")
    private static let dateLead = set("aus seit since from in im during within während innerhalb von vom der dem den des the")
    private static let monthLead = set("in im from since seit aus during während")
    private static let last = set("letzter letzten letzte letztem letztes last past previous vergangene vergangenen vergangener vergangenes")
    private static let this = set("diese dieser diesen diesem dieses this current aktuelle aktuellen aktueller")
    private static let units: [(String, Set<String>)] = [
        ("d", set("tag tage tagen day days")),
        ("w", set("woche wochen week weeks")),
        ("m", set("monat monate monaten month months")),
        ("y", set("jahr jahre jahren year years")),
    ]
    private static let today = set("heute today")
    private static let yesterday = set("gestern yesterday")
    private static let from = set("seit since ab after nach from")
    private static let until = set("vor before bis until")
    private static let months: [[String]] = [
        ["januar", "january"],
        ["februar", "february"],
        ["märz", "march"],
        ["april"],
        ["mai", "may"],
        ["juni", "june"],
        ["juli", "july"],
        ["august"],
        ["september"],
        ["oktober", "october"],
        ["november"],
        ["dezember", "december"],
    ]
    private static let updated = set(
        "updated changed modified edited geändert geänderte geänderten aktualisiert aktualisierte aktualisierten bearbeitet bearbeitete bearbeiteten"
    )
    private static let nouns = set("issue issues ticket tickets aufgabe aufgaben task tasks vorgang vorgänge")
    private static let noise = set(
        "created erstellt erstellte erstellten angelegt angelegte angelegten filed opened eröffnet gemeldet gemeldete gemeldeten reported "
            + "alle all zeige zeig show me mir find finde suche such search list liste the der die das den dem des a an ein eine einen einem einer "
            + "that which were was wurden wurde sind are is ist hat haben have has been worden und and mit with zum zur zu über about regarding on "
            + "for für wegen bezüglich in im aus from of to welche welcher bitte please give gib any es gibt there"
    )

    private static func unitOf(_ token: String?) -> String? {
        guard let token else { return nil }
        return units.first(where: { $0.1.contains(token) })?.0
    }

    /// 1…12, or 0 when the token is no month name.
    private static func monthOf(_ token: String?) -> Int {
        guard let token else { return 0 }
        return (months.firstIndex(where: { $0.contains(token) }) ?? -1) + 1
    }

    private static func isDigits(_ token: String, _ lengths: ClosedRange<Int>) -> Bool {
        let bytes = token.utf8
        return lengths.contains(bytes.count) && bytes.allSatisfy { $0 >= 48 && $0 <= 57 }
    }

    private static func isYear(_ token: String?) -> Bool {
        token.map { isDigits($0, 4...4) } ?? false
    }

    /// The value of a 1…4 digit count of at least 1, or nil.
    private static func count(_ token: String?) -> Int? {
        guard let token, isDigits(token, 1...4), let value = Int(token), value >= 1 else { return nil }
        return value
    }

    static func pad(_ value: Int, _ width: Int) -> String {
        let digits = String(value)
        return String(repeating: "0", count: max(0, width - digits.count)) + digits
    }

    static func floorDiv(_ a: Int, _ b: Int) -> Int {
        Int((Double(a) / Double(b)).rounded(.down))
    }

    /// The UTC civil date of a day number (Howard Hinnant's civil_from_days).
    static func civilFromDays(_ days: Int) -> (year: Int, month: Int, day: Int) {
        let z = days + 719_468
        let era = floorDiv(z, 146_097)
        let doe = z - era * 146_097
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
        let mp = (5 * doy + 2) / 153
        let month = mp < 10 ? mp + 3 : mp - 9
        return (yoe + era * 400 + (month <= 2 ? 1 : 0), month, doy - (153 * mp + 2) / 5 + 1)
    }

    /// A date phrase starting at `tokens[i]`: its `created:`/`updated:` value
    /// and how many tokens it spans, or nil.
    private static func dateAt(_ tokens: [String], _ i: Int, today day: Int) -> (value: String, length: Int)? {
        func token(_ at: Int) -> String? { at < tokens.count ? tokens[at] : nil }
        let t = tokens[i]
        let next = token(i + 1)
        if today.contains(t) { return ("today", 1) }
        if yesterday.contains(t) { return ("yesterday", 1) }
        if last.contains(t) {
            if let unit = unitOf(next) { return (unit == "w" ? "7d" : "1\(unit)", 2) }
            if let n = count(next), let counted = unitOf(token(i + 2)) { return ("\(n)\(counted)", 3) }
            return nil
        }
        if this.contains(t) {
            let unit = unitOf(next)
            let now = civilFromDays(day)
            if unit == "d" { return ("today", 2) }
            if unit == "w" {
                // Monday of the current UTC week (1970-01-01 was a Thursday).
                let monday = civilFromDays(day - (((day + 3) % 7) + 7) % 7)
                return (">=\(pad(monday.year, 4))-\(pad(monday.month, 2))-\(pad(monday.day, 2))", 2)
            }
            if unit == "m" { return ("\(pad(now.year, 4))-\(pad(now.month, 2))", 2) }
            if unit == "y" { return (">=\(pad(now.year, 4))-01-01", 2) }
            return nil
        }
        if from.contains(t), let next, IssueSearch.parseDate(next) != nil { return (">=\(next)", 2) }
        if until.contains(t), let next, IssueSearch.parseDate(next) != nil { return ("<\(next)", 2) }
        if IssueSearch.parseDate(t) != nil { return (t, 1) }
        // A month name counts only after a lead-in (`im Mai`, `in may`) or
        // before a year (`mai 2026`): `may` and `march` are words too.
        let month = monthOf(next)
        if monthLead.contains(t), month > 0 {
            if let year = token(i + 2), isYear(year) { return ("\(year)-\(pad(month, 2))", 3) }
            let now = civilFromDays(day)
            return ("\(pad(month > now.month ? now.year - 1 : now.year, 4))-\(pad(month, 2))", 2)
        }
        let bare = monthOf(t)
        if bare > 0, let next, isYear(next) { return ("\(next)-\(pad(bare, 2))", 2) }
        return nil
    }

    /// The structured query a sentence means, or nil when the grammar does
    /// not recognise one (plain words, a query that already carries a filter
    /// token, or nothing that would change). `now` = the clock `this
    /// week|month|year` and month names resolve against.
    public static func translate(_ text: String, members: [Member] = [], now: Date = Date()) -> String? {
        translate(text, members: members, nowMs: Int64((now.timeIntervalSince1970 * 1000).rounded(.down)))
    }

    /// `translate` on an epoch-ms clock (what the fixture replays).
    public static func translate(_ text: String, members: [Member], nowMs: Int64) -> String? {
        let day = floorDiv(Int(nowMs), Int(IssueSearch.daySeconds) * 1000)

        // The engine's own lowercasing and whitespace split (`IssueSearch.tokens`).
        var tokens: [String] = []
        for raw in text.lowercased().split(whereSeparator: { $0.isWhitespace }) {
            var piece = raw
            while let first = piece.first, strip.contains(first) { piece = piece.dropFirst() }
            while let last = piece.last, strip.contains(last) { piece = piece.dropLast() }
            if piece.isEmpty { continue }
            let token = String(piece)
            if filterPrefixes.contains(where: { token.hasPrefix($0) }) { return nil }
            tokens.append(token)
        }

        // Each member's name words and email local part, lowercased.
        let people: [(words: [String], local: String)] = members.map { member in
            let email = (member.email ?? "").lowercased()
            let local = email.firstIndex(of: "@").map { String(email[..<$0]) } ?? email
            return (IssueSearch.words((member.name ?? "").lowercased()), local)
        }
        func names(_ at: Int, _ token: String) -> Bool {
            people[at].words.contains(token) || (!people[at].local.isEmpty && people[at].local == token)
        }

        var states: [String] = []
        var statuses: [String] = []
        var creators: [String] = []
        var dates: [String] = []
        var words: [String] = []
        var isUpdated = false
        var noun = false
        var strong = 0
        var weak = 0
        func add(_ list: inout [String], _ value: String) {
            if !list.contains(value) { list.append(value) }
        }

        var i = 0
        while i < tokens.count {
            let t = tokens[i]
            let next: String? = i + 1 < tokens.count ? tokens[i + 1] : nil

            // 1. A date phrase, behind up to two lead-in words (`aus der letzten Woche`).
            var lead = 0
            var date = dateAt(tokens, i, today: day)
            while date == nil, lead < 2, i + lead < tokens.count, dateLead.contains(tokens[i + lead]) {
                lead += 1
                if i + lead < tokens.count { date = dateAt(tokens, i + lead, today: day) }
            }
            if let date {
                add(&dates, date.value)
                strong += 1
                i += lead + date.length
                continue
            }

            // 2. A two-word status, then the one-word states and statuses.
            if let pair = statusPairs.first(where: { $0.0 == t && $0.1 == next }) {
                add(&statuses, pair.2)
                weak += 1
                i += 2
                continue
            }
            if open.contains(t) {
                add(&states, "open")
                weak += 1
                i += 1
                continue
            }
            if closed.contains(t) {
                add(&states, "closed")
                weak += 1
                i += 1
                continue
            }
            if let status = statusWords.first(where: { $0.1.contains(t) }) {
                add(&statuses, status.0)
                weak += 1
                i += 1
                continue
            }

            // 3. `von|by|from <member>`: every following word that is a name
            //    word of the same member joins (`von Lukas Mai`).
            if creatorLead.contains(t) {
                var matched = next.map { word in people.indices.filter { names($0, word) } } ?? []
                if !matched.isEmpty {
                    var j = i + 1
                    while j < tokens.count {
                        let narrowed = matched.filter { names($0, tokens[j]) }
                        if narrowed.isEmpty { break }
                        matched = narrowed
                        add(&creators, tokens[j])
                        j += 1
                    }
                    strong += 1
                    i = j
                    continue
                }
                if creatorDrop.contains(t) {
                    i += 1
                    continue
                }
            }

            // 4. The date field, the nouns and the filler words.
            if updated.contains(t) {
                isUpdated = true
                i += 1
                continue
            }
            if nouns.contains(t) {
                noun = true
                i += 1
                continue
            }
            if noise.contains(t) {
                i += 1
                continue
            }
            words.append(t)
            i += 1
        }

        // A date or a member is a sentence on its own; a state or status word
        // needs the noun beside it (`open issues`, not `open file dialog`).
        if strong == 0, !(weak > 0 && noun) { return nil }
        let field = isUpdated ? IssueSearch.updatedPrefix : IssueSearch.createdPrefix
        let query = (
            states.map { "\(IssueSearch.statePrefix)\($0)" }
                + statuses.map { "\(IssueSearch.statusPrefix)\($0)" }
                + creators.map { "\(IssueSearch.creatorPrefix)\($0)" }
                + dates.map { "\(field)\($0)" }
                + words
        ).joined(separator: " ")
        return query == tokens.joined(separator: " ") ? nil : query
    }
}
