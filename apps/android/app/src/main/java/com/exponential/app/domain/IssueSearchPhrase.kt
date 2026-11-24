package com.exponential.app.domain

/**
 * #NEW-133: the natural-language PHRASE GRAMMAR of the issue search. A
 * sentence (`offene Issues von Lukas zum Editor aus letzter Woche`) becomes
 * the structured query the engine already runs (`is:open by:lukas created:7d
 * editor`, [IssueSearch]). Deterministic, no model, German + English.
 * Hand-mirrored from web `translateIssueSearchPhrase`
 * (`apps/web/src/lib/issue-search-phrase.ts`, which owns the WORD LISTS, byte
 * for byte the same here), byte-locked by
 * `packages/domain-contract/fixtures/issue-search-phrase.json` (its `$comment`
 * is the spec; read it before changing a rule).
 */
object IssueSearchPhrase {

    /** One team member: a name after `von`/`by` that is one of them turns into `by:`. */
    data class Member(val name: String? = null, val email: String? = null)

    private fun set(words: String): Set<String> = words.split(" ").toSet()

    private val FILTER_PREFIXES = listOf("by:", "is:", "status:", "created:", "updated:", "similar:")
    private const val STRIP = ".,;:!?\"'()"

    private val OPEN = set(
        "open offen offene offenen offener offenes unresolved ungelöst ungelöste ungelösten unfinished unerledigt unerledigte unerledigten",
    )
    private val CLOSED = set("closed geschlossen geschlossene geschlossenen resolved gelöst gelöste gelösten")
    private val STATUS_WORDS: List<Pair<String, Set<String>>> = listOf(
        "backlog" to set("backlog"),
        "done" to set(
            "done erledigt erledigte erledigten fertig fertige fertigen completed finished abgeschlossen abgeschlossene abgeschlossenen",
        ),
        "cancelled" to set(
            "cancelled canceled abgebrochen abgebrochene abgebrochenen storniert stornierte stornierten",
        ),
        "duplicate" to set("duplicate duplicates duplikat duplikate duplikaten"),
    )

    /** Two-word statuses: `in progress`, `in arbeit`, `in review`. */
    private val STATUS_PAIRS: List<Triple<String, String, String>> = listOf(
        Triple("in", "progress", "progress"),
        Triple("in", "arbeit", "progress"),
        Triple("in", "bearbeitung", "progress"),
        Triple("in", "review", "review"),
        Triple("im", "review", "review"),
    )
    private val CREATOR_LEAD = set("von vom by from")

    /** A creator lead-in that is dropped even when no member follows. */
    private val CREATOR_DROP = set("von vom by")
    private val DATE_LEAD =
        set("aus seit since from in im during within während innerhalb von vom der dem den des the")
    private val MONTH_LEAD = set("in im from since seit aus during während")
    private val LAST = set(
        "letzter letzten letzte letztem letztes last past previous vergangene vergangenen vergangener vergangenes",
    )
    private val THIS = set("diese dieser diesen diesem dieses this current aktuelle aktuellen aktueller")
    private val UNITS: List<Pair<String, Set<String>>> = listOf(
        "d" to set("tag tage tagen day days"),
        "w" to set("woche wochen week weeks"),
        "m" to set("monat monate monaten month months"),
        "y" to set("jahr jahre jahren year years"),
    )
    private val TODAY = set("heute today")
    private val YESTERDAY = set("gestern yesterday")
    private val FROM = set("seit since ab after nach from")
    private val UNTIL = set("vor before bis until")
    private val MONTHS: List<List<String>> = listOf(
        listOf("januar", "january"),
        listOf("februar", "february"),
        listOf("märz", "march"),
        listOf("april"),
        listOf("mai", "may"),
        listOf("juni", "june"),
        listOf("juli", "july"),
        listOf("august"),
        listOf("september"),
        listOf("oktober", "october"),
        listOf("november"),
        listOf("dezember", "december"),
    )
    private val UPDATED = set(
        "updated changed modified edited geändert geänderte geänderten aktualisiert aktualisierte aktualisierten bearbeitet bearbeitete bearbeiteten",
    )
    private val NOUNS = set("issue issues ticket tickets aufgabe aufgaben task tasks vorgang vorgänge")
    private val NOISE = set(
        "created erstellt erstellte erstellten angelegt angelegte angelegten filed opened eröffnet gemeldet gemeldete gemeldeten reported " +
            "alle all zeige zeig show me mir find finde suche such search list liste the der die das den dem des a an ein eine einen einem einer " +
            "that which were was wurden wurde sind are is ist hat haben have has been worden und and mit with zum zur zu über about regarding on " +
            "for für wegen bezüglich in im aus from of to welche welcher bitte please give gib any es gibt there",
    )

    private fun unitOf(token: String?): String? =
        if (token == null) null else UNITS.firstOrNull { token in it.second }?.first

    /** 1..12, or 0 when [token] is no month name. */
    private fun monthOf(token: String?): Int =
        if (token == null) 0 else MONTHS.indexOfFirst { token in it } + 1

    private fun isAsciiDigits(token: String): Boolean = token.all { it in '0'..'9' }

    private fun isYear(token: String?): Boolean =
        token != null && token.length == 4 && isAsciiDigits(token)

    /** A count 1..9999 written in 1 to 4 ASCII digits, or null. */
    private fun countOf(token: String?): Int? {
        if (token == null || token.length !in 1..4 || !isAsciiDigits(token)) return null
        return token.toInt().takeIf { it >= 1 }
    }

    private fun pad(value: Long, width: Int): String = value.toString().padStart(width, '0')

    private data class Civil(val year: Long, val month: Long, val day: Long)

    /** The UTC civil date of a day number (Howard Hinnant's civil_from_days). */
    private fun civilFromDays(days: Long): Civil {
        val z = days + 719_468
        val era = Math.floorDiv(z, 146_097L)
        val doe = z - era * 146_097
        val yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365
        val doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
        val mp = (5 * doy + 2) / 153
        val month = if (mp < 10) mp + 3 else mp - 9
        return Civil(yoe + era * 400 + (if (month <= 2) 1 else 0), month, doy - (153 * mp + 2) / 5 + 1)
    }

    private data class DatePhrase(val value: String, val length: Int)

    /**
     * A date phrase starting at `tokens[i]`: its `created:`/`updated:` value
     * and how many tokens it spans, or null.
     */
    private fun dateAt(tokens: List<String>, i: Int, today: Long): DatePhrase? {
        val t = tokens[i]
        val next = tokens.getOrNull(i + 1)
        if (t in TODAY) return DatePhrase("today", 1)
        if (t in YESTERDAY) return DatePhrase("yesterday", 1)
        if (t in LAST) {
            val unit = unitOf(next)
            if (unit != null) return DatePhrase(if (unit == "w") "7d" else "1$unit", 2)
            val count = countOf(next)
            val counted = unitOf(tokens.getOrNull(i + 2))
            if (count != null && counted != null) return DatePhrase("$count$counted", 3)
            return null
        }
        if (t in THIS) {
            val now = civilFromDays(today)
            return when (unitOf(next)) {
                "d" -> DatePhrase("today", 2)
                "w" -> {
                    // Monday of the current UTC week (1970-01-01 was a Thursday).
                    val monday = civilFromDays(today - Math.floorMod(today + 3, 7L))
                    DatePhrase(">=${pad(monday.year, 4)}-${pad(monday.month, 2)}-${pad(monday.day, 2)}", 2)
                }
                "m" -> DatePhrase("${pad(now.year, 4)}-${pad(now.month, 2)}", 2)
                "y" -> DatePhrase(">=${pad(now.year, 4)}-01-01", 2)
                else -> null
            }
        }
        if (t in FROM && next != null && IssueSearch.isDate(next)) return DatePhrase(">=$next", 2)
        if (t in UNTIL && next != null && IssueSearch.isDate(next)) return DatePhrase("<$next", 2)
        if (IssueSearch.isDate(t)) return DatePhrase(t, 1)
        // A month name counts only after a lead-in (`im Mai`, `in may`) or before
        // a year (`mai 2026`): `may` and `march` are words too.
        val month = monthOf(next)
        if (t in MONTH_LEAD && month > 0) {
            val year = tokens.getOrNull(i + 2)
            if (isYear(year)) return DatePhrase("$year-${pad(month.toLong(), 2)}", 3)
            val now = civilFromDays(today)
            return DatePhrase("${pad(if (month > now.month) now.year - 1 else now.year, 4)}-${pad(month.toLong(), 2)}", 2)
        }
        val bare = monthOf(t)
        if (bare > 0 && isYear(next)) return DatePhrase("$next-${pad(bare.toLong(), 2)}", 2)
        return null
    }

    private class Names(val words: List<String>, val local: String) {
        fun has(token: String): Boolean = token in words || local == token
    }

    /**
     * The phrase's tokens: lowercased, split on [Char.isWhitespace] (the
     * engine's own split, [IssueSearch.tokens]), each stripped of leading and
     * trailing [STRIP] characters, empties dropped.
     */
    private fun tokensOf(text: String): List<String> {
        val lowered = text.lowercase()
        val out = mutableListOf<String>()
        var start = 0
        for (i in 0..lowered.length) {
            if (i < lowered.length && !lowered[i].isWhitespace()) continue
            if (i > start) {
                val token = lowered.substring(start, i).trim { it in STRIP }
                if (token.isNotEmpty()) out.add(token)
            }
            start = i + 1
        }
        return out
    }

    /**
     * The structured query a sentence means, or null when the grammar does not
     * recognise one (plain words, a query that already carries a filter token,
     * or nothing that would change). [now] = epoch ms.
     */
    fun translate(
        text: String,
        members: List<Member> = emptyList(),
        now: Long = System.currentTimeMillis(),
    ): String? {
        val today = Math.floorDiv(now, IssueSearch.DAY_MS)
        val tokens = tokensOf(text)
        if (tokens.any { token -> FILTER_PREFIXES.any { token.startsWith(it) } }) return null

        // Each member's name words and email local part, lowercased.
        val names = members.map { member ->
            val email = (member.email ?: "").lowercase()
            val at = email.indexOf('@')
            Names(IssueSearch.words((member.name ?: "").lowercase()), if (at >= 0) email.substring(0, at) else email)
        }

        val states = mutableListOf<String>()
        val statuses = mutableListOf<String>()
        val creators = mutableListOf<String>()
        val dates = mutableListOf<String>()
        val words = mutableListOf<String>()
        var updated = false
        var noun = false
        var strong = 0
        var weak = 0
        fun add(list: MutableList<String>, value: String) {
            if (value !in list) list.add(value)
        }

        var i = 0
        while (i < tokens.size) {
            val t = tokens[i]
            val next = tokens.getOrNull(i + 1)

            // 1. A date phrase, behind up to two lead-in words (`aus der letzten Woche`).
            var lead = 0
            var date = dateAt(tokens, i, today)
            while (date == null && lead < 2 && i + lead < tokens.size && tokens[i + lead] in DATE_LEAD) {
                lead += 1
                if (i + lead < tokens.size) date = dateAt(tokens, i + lead, today)
            }
            if (date != null) {
                add(dates, date.value)
                strong += 1
                i += lead + date.length
                continue
            }

            // 2. A two-word status, then the one-word states and statuses.
            val pair = STATUS_PAIRS.firstOrNull { it.first == t && it.second == next }
            if (pair != null) {
                add(statuses, pair.third)
                weak += 1
                i += 2
                continue
            }
            if (t in OPEN) {
                add(states, "open")
                weak += 1
                i += 1
                continue
            }
            if (t in CLOSED) {
                add(states, "closed")
                weak += 1
                i += 1
                continue
            }
            val status = STATUS_WORDS.firstOrNull { t in it.second }
            if (status != null) {
                add(statuses, status.first)
                weak += 1
                i += 1
                continue
            }

            // 3. `von|by|from <member>`: every following word that is a name word of
            //    the same member joins (`von Lukas Mai`).
            if (t in CREATOR_LEAD) {
                var matched = if (next == null) emptyList() else names.filter { it.has(next) }
                if (matched.isNotEmpty()) {
                    var j = i + 1
                    while (j < tokens.size) {
                        val narrowed = matched.filter { it.has(tokens[j]) }
                        if (narrowed.isEmpty()) break
                        matched = narrowed
                        add(creators, tokens[j])
                        j += 1
                    }
                    strong += 1
                    i = j
                    continue
                }
                if (t in CREATOR_DROP) {
                    i += 1
                    continue
                }
            }

            // 4. The date field, the nouns and the filler words.
            when (t) {
                in UPDATED -> updated = true
                in NOUNS -> noun = true
                in NOISE -> Unit
                else -> words.add(t)
            }
            i += 1
        }

        // A date or a member is a sentence on its own; a state or status word
        // needs the noun beside it (`open issues`, not `open file dialog`).
        if (strong == 0 && !(weak > 0 && noun)) return null
        val field = if (updated) "updated:" else "created:"
        val query = (
            states.map { "is:$it" } +
                statuses.map { "status:$it" } +
                creators.map { "by:$it" } +
                dates.map { "$field$it" } +
                words
            ).joinToString(" ")
        return if (query == tokens.joinToString(" ")) null else query
    }
}
