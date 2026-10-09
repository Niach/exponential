package at.exponential.ui.format

import android.icu.text.DateFormat
import android.icu.text.DateFormatSymbols
import android.icu.text.MeasureFormat
import android.icu.text.NumberFormat
import android.icu.text.PluralRules
import android.icu.text.RelativeDateTimeFormatter
import android.icu.util.Calendar
import android.icu.util.Currency
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import android.icu.util.TimeZone
import android.icu.util.ULocale
import android.os.Build
import at.exponential.ui.ffi.HostFormatter
import at.exponential.ui.ffi.formatPatternJson
import at.exponential.ui.json.JsonValue
import java.math.BigDecimal
import java.math.RoundingMode

/**
 * The surface [HostFormatter] over `android.icu` (round 2, §3): numbers,
 * currencies, percents, dates, relative times and plural categories in
 * [locale] (BCP 47) and [timeZone] (IANA; null = the device's). The core
 * parses values and picks the relative unit and the plural arm; this only
 * localizes. Rounding is half away from zero on the shortest round-trip
 * decimal; `format` patterns go through the core's TR35 subset
 * (`formatPatternJson`) with ICU's month and weekday names; a time's
 * narrow no-break space before AM/PM is a plain space (the contract's
 * en-US output).
 */
class IcuFormatter(val locale: String, val timeZone: String? = null) : HostFormatter {
    private val ulocale: ULocale = ULocale.forLanguageTag(locale.ifEmpty { "en-US" })
    private val zone: TimeZone = timeZone?.let { TimeZone.getTimeZone(it) } ?: TimeZone.getDefault()
    private val utc: TimeZone = TimeZone.getTimeZone("UTC")
    private val plurals: PluralRules = PluralRules.forLocale(ulocale)
    private val names: String by lazy { namesJson(ulocale) }

    override fun locale(): String = ulocale.toLanguageTag()

    private fun decimal(value: Double): BigDecimal = BigDecimal(value.toString())

    private fun NumberFormat.digits(min: Int, max: Int): NumberFormat {
        minimumFractionDigits = min
        maximumFractionDigits = max
        roundingMode = RoundingMode.HALF_UP.ordinal
        return this
    }

    override fun number(value: Double, decimals: UInt?, grouping: Boolean): String {
        if (!value.isFinite()) return ""
        val f = NumberFormat.getNumberInstance(ulocale)
        val d = decimals?.toInt()
        f.digits(d ?: 0, d ?: 3)
        f.isGroupingUsed = grouping
        return clean(f.format(decimal(value)))
    }

    override fun currency(value: Double, code: String, decimals: UInt?, grouping: Boolean): String {
        if (!value.isFinite()) return ""
        val currency = runCatching { Currency.getInstance(code.uppercase()) }.getOrNull() ?: return ""
        val f = NumberFormat.getCurrencyInstance(ulocale)
        f.currency = currency
        val d = decimals?.toInt() ?: currency.defaultFractionDigits
        f.digits(d, d)
        f.isGroupingUsed = grouping
        return clean(f.format(decimal(value)))
    }

    override fun percent(value: Double, decimals: UInt?): String {
        if (!value.isFinite()) return ""
        val f = NumberFormat.getPercentInstance(ulocale)
        val d = decimals?.toInt() ?: 0
        f.digits(d, d)
        return clean(f.format(decimal(value)))
    }

    override fun date(epochMs: Double, dateOnly: Boolean, format: String?, style: String?, time: Boolean): String {
        val tz = if (dateOnly) utc else zone
        val ms = epochMs.toLong()
        if (format != null) {
            val cal = Calendar.getInstance(tz, ULocale.ROOT)
            cal.timeInMillis = ms
            val fields = JsonValue.Obj(
                mapOf(
                    "year" to JsonValue.Num(cal.get(Calendar.YEAR).toDouble()),
                    "month" to JsonValue.Num((cal.get(Calendar.MONTH) + 1).toDouble()),
                    "day" to JsonValue.Num(cal.get(Calendar.DAY_OF_MONTH).toDouble()),
                    "weekday" to JsonValue.Num((cal.get(Calendar.DAY_OF_WEEK) - 1).toDouble()),
                    "hour" to JsonValue.Num(cal.get(Calendar.HOUR_OF_DAY).toDouble()),
                    "minute" to JsonValue.Num(cal.get(Calendar.MINUTE).toDouble()),
                    "second" to JsonValue.Num(cal.get(Calendar.SECOND).toDouble()),
                ),
            )
            return runCatching { formatPatternJson(format, fields.json, names) }.getOrDefault("")
        }
        val s = when (style) {
            "short" -> DateFormat.SHORT
            "long" -> DateFormat.LONG
            "full" -> DateFormat.FULL
            else -> DateFormat.MEDIUM
        }
        val f = if (time) DateFormat.getDateTimeInstance(s, DateFormat.SHORT, ulocale) else DateFormat.getDateInstance(s, ulocale)
        f.timeZone = tz
        return clean(f.format(java.util.Date(ms)))
    }

    override fun relativeTime(value: Long, unit: String): String {
        val f = RelativeDateTimeFormatter.getInstance(ulocale)
        if (Build.VERSION.SDK_INT >= 28) {
            val u = when (unit) {
                "second" -> RelativeDateTimeFormatter.RelativeDateTimeUnit.SECOND
                "minute" -> RelativeDateTimeFormatter.RelativeDateTimeUnit.MINUTE
                "hour" -> RelativeDateTimeFormatter.RelativeDateTimeUnit.HOUR
                "day" -> RelativeDateTimeFormatter.RelativeDateTimeUnit.DAY
                "week" -> RelativeDateTimeFormatter.RelativeDateTimeUnit.WEEK
                "month" -> RelativeDateTimeFormatter.RelativeDateTimeUnit.MONTH
                else -> RelativeDateTimeFormatter.RelativeDateTimeUnit.YEAR
            }
            return clean(f.format(value.toDouble(), u))
        }
        return clean(legacyRelative(f, value, unit))
    }

    /** API 26–27: `numeric: auto` by hand (the words ICU has for ±1 day / week / month / year and 0 seconds). */
    private fun legacyRelative(f: RelativeDateTimeFormatter, value: Long, unit: String): String {
        val absolute = when (unit) {
            "day" -> RelativeDateTimeFormatter.AbsoluteUnit.DAY
            "week" -> RelativeDateTimeFormatter.AbsoluteUnit.WEEK
            "month" -> RelativeDateTimeFormatter.AbsoluteUnit.MONTH
            "year" -> RelativeDateTimeFormatter.AbsoluteUnit.YEAR
            else -> null
        }
        if (unit == "second" && value == 0L) return f.format(RelativeDateTimeFormatter.Direction.PLAIN, RelativeDateTimeFormatter.AbsoluteUnit.NOW)
        if (absolute != null && value in -1L..1L) {
            val dir = when (value) {
                -1L -> RelativeDateTimeFormatter.Direction.LAST
                1L -> RelativeDateTimeFormatter.Direction.NEXT
                else -> RelativeDateTimeFormatter.Direction.THIS
            }
            return f.format(dir, absolute)
        }
        val relative = when (unit) {
            "second" -> RelativeDateTimeFormatter.RelativeUnit.SECONDS
            "minute" -> RelativeDateTimeFormatter.RelativeUnit.MINUTES
            "hour" -> RelativeDateTimeFormatter.RelativeUnit.HOURS
            "day" -> RelativeDateTimeFormatter.RelativeUnit.DAYS
            "week" -> RelativeDateTimeFormatter.RelativeUnit.WEEKS
            "month" -> RelativeDateTimeFormatter.RelativeUnit.MONTHS
            else -> RelativeDateTimeFormatter.RelativeUnit.YEARS
        }
        val dir = if (value < 0) RelativeDateTimeFormatter.Direction.LAST else RelativeDateTimeFormatter.Direction.NEXT
        return f.format(kotlin.math.abs(value).toDouble(), dir, relative)
    }

    override fun plural(value: Double): String = plurals.select(value)

    /** A byte size in the locale's short unit (`Intl` `style: unit`: en `47.1 kB`, de `500 Byte`). */
    override fun bytes(value: Double, unit: String): String {
        if (!value.isFinite()) return ""
        val u = when (unit) {
            "kilobyte" -> MeasureUnit.KILOBYTE
            "megabyte" -> MeasureUnit.MEGABYTE
            "gigabyte" -> MeasureUnit.GIGABYTE
            else -> MeasureUnit.BYTE
        }
        val n = NumberFormat.getNumberInstance(ulocale).digits(0, 1)
        return clean(MeasureFormat.getInstance(ulocale, MeasureFormat.FormatWidth.SHORT, n).format(Measure(decimal(value), u)))
    }

    companion object {
        /**
         * The ICU formatter in `locale` / `timeZone`, or null where
         * `android.icu` is not usable (a plain-JVM unit test): the surface then
         * formats through the core's English fallback.
         */
        fun create(locale: String, timeZone: String?): IcuFormatter? = runCatching {
            IcuFormatter(locale, timeZone).also { check(it.number(1.5, null, true).isNotEmpty()) }
        }.getOrNull()

        /** A time's narrow no-break space (ICU 72+, before AM/PM) → a plain space. */
        private fun clean(s: String): String = s.replace(" PM", " PM").replace(" AM", " AM")

        /** ICU's month / weekday / day-period names for `format_pattern` (`DateNames`). */
        internal fun namesJson(locale: ULocale): String {
            val sym = DateFormatSymbols.getInstance(locale)
            fun arr(a: Array<String>, from: Int = 0, count: Int = a.size - from) = JsonValue.Arr(a.drop(from).take(count).map(JsonValue::Str))
            return JsonValue.Obj(
                mapOf(
                    "months" to arr(sym.months, 0, 12),
                    "monthsShort" to arr(sym.shortMonths, 0, 12),
                    // ICU weekdays are 1-based (index 0 empty): Sunday first.
                    "weekdays" to arr(sym.weekdays, 1, 7),
                    "weekdaysShort" to arr(sym.shortWeekdays, 1, 7),
                    "dayPeriods" to arr(sym.amPmStrings, 0, 2),
                ),
            ).json
        }
    }
}
