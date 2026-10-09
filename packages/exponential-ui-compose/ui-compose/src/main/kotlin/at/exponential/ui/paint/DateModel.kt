package at.exponential.ui.paint

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset
import kotlin.math.floor
import kotlin.math.max

/** ISO dates (`YYYY-MM-DD`) the DatePicker speaks, in UTC. */
object DateModel {
    /** `"2026-10-14"` → (2026, 10, 14); null when malformed. */
    fun parseISO(s: String): Triple<Int, Int, Int>? {
        val parts = s.trim().split("-")
        if (parts.size != 3) return null
        val y = parts[0].toIntOrNull() ?: return null
        val m = parts[1].toIntOrNull() ?: return null
        val d = parts[2].toIntOrNull() ?: return null
        if (m !in 1..12 || d !in 1..31) return null
        return Triple(y, m, d)
    }

    /** (y, m, d) → `"YYYY-MM-DD"`. */
    fun iso(y: Int, m: Int, d: Int): String = String.format("%04d-%02d-%02d", y, m, d)

    /** The calendar date of an ISO string. */
    fun date(fromISO: String): LocalDate? {
        val (y, m, d) = parseISO(fromISO) ?: return null
        return runCatching { LocalDate.of(y, m, d) }.getOrNull()
    }

    /** UTC midnight of an ISO date in epoch millis (the Material date picker's unit). */
    fun millis(fromISO: String): Long? = date(fromISO)?.atStartOfDay(ZoneOffset.UTC)?.toInstant()?.toEpochMilli()

    /** The ISO date of a calendar date. */
    fun iso(date: LocalDate): String = iso(date.year, date.monthValue, date.dayOfMonth)

    /** The ISO date (UTC) of epoch millis. */
    fun iso(fromMillis: Long): String = iso(Instant.ofEpochMilli(fromMillis).atZone(ZoneOffset.UTC).toLocalDate())

    /** `"2026-10-14"` → `"Oct 14, 2026"`. */
    fun label(value: String): String? {
        val (y, m, d) = parseISO(value) ?: return null
        val months = listOf("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")
        return "${months[m - 1]} $d, $y"
    }

    /** Milliseconds → `m:ss` / `h:mm:ss`. */
    fun formatDuration(ms: Double): String {
        val total = floor(max(0.0, ms / 1000) + 0.5).toInt()
        val h = total / 3600
        val m = (total / 60) % 60
        val s = total % 60
        return if (h > 0) String.format("%d:%02d:%02d", h, m, s) else String.format("%d:%02d", m, s)
    }
}
