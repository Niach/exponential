package at.exponential.ui.paint

import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.flag
import at.exponential.ui.json.list
import at.exponential.ui.json.str
import kotlin.math.PI
import kotlin.math.atan2
import kotlin.math.floor
import kotlin.math.min
import kotlin.math.sqrt

/**
 * The chart a `Chart` leaf paints, from its props and the numbers the core
 * put in `props.chart` (round 1 §3: extent, nice ticks + their formatted
 * labels, the donut hole, whether a legend shows), so every painter draws
 * the same axes.
 */
class ChartModel(props: Props) {
    /** One data series. */
    data class Series(val name: String, val values: List<Double>, val tone: String?)

    /** `bar` (default) `stackedBar line area pie donut sparkline`. */
    val kind: String = props.str("kind").ifEmpty { "bar" }
    val categories: List<String> = props.list("categories").map { it.displayText }
    val series: List<Series> = props.list("series").map { s ->
        Series(
            name = s["name"]?.displayText ?: "",
            // A missing / non-numeric point = NaN: skipped when drawn (as the web).
            values = (s["values"]?.array ?: emptyList()).map { it.number ?: Double.NaN },
            tone = s["tone"]?.string,
        )
    }
    val title: String = props.str("title")
    val xLabel: String = props.str("xLabel")
    val yLabel: String = props.str("yLabel")
    private val chart: Map<String, JsonValue> = props["chart"]?.obj ?: emptyMap()

    val slices: Boolean get() = kind == "pie" || kind == "donut"
    val spark: Boolean get() = kind == "sparkline"
    val showAxes: Boolean = !spark && !slices && props["showAxes"]?.bool != false
    val showGrid: Boolean = !spark && !slices && props["showGrid"]?.bool != false
    val showValues: Boolean = props.flag("showValues")

    /** The slice values of a pie / donut (negatives and missing count as 0). */
    val sliceValues: List<Double> get() = (series.firstOrNull()?.values ?: emptyList()).map { if (it.isFinite()) maxOf(it, 0.0) else 0.0 }

    /** Categories along x (at least 1). */
    val count: Int get() = maxOf(categories.size, series.maxOfOrNull { it.values.size } ?: 0, 1)

    val min: Double = chart["min"]?.number ?: 0.0
    val max: Double = chart["max"]?.number ?: maxOf(series.flatMap { it.values }.filter { it.isFinite() }.maxOrNull() ?: 1.0, 1.0)
    val ticks: List<Double> = chart["ticks"]?.array?.mapNotNull { it.number } ?: listOf(min, max)

    /** The tick labels the core formatted (the surface formatter, the step's decimals). */
    val tickLabels: List<String> = chart["tickLabels"]?.array?.map { it.displayText } ?: ticks.map(::formatValue)
    val hole: Double = chart["hole"]?.number ?: if (kind == "donut") 0.6 else 0.0

    /** The legend entries (2+ series, or the slices) when the core says one shows. */
    val legend: List<String> = if (chart["legend"]?.bool == false || spark || props["showLegend"]?.bool == false) {
        emptyList()
    } else if (slices) {
        categories
    } else if (series.size > 1) {
        series.map { it.name }
    } else {
        emptyList()
    }

    /**
     * What `$string.chartSummary` is filled with (`chartSummaryParams` in
     * the TS reference): the non-empty series names, ", "-joined as the
     * web does, and the extent of the finite data (0 when none).
     */
    fun summaryParams(): Triple<String, Double, Double> {
        val finite = series.flatMap { it.values }.filter { it.isFinite() }
        return Triple(series.map { it.name }.filter { it.isNotEmpty() }.joinToString(", "), finite.minOrNull() ?: 0.0, finite.maxOrNull() ?: 0.0)
    }

    companion object {
        /**
         * A number through the surface formatter (round 2 §3: the locale's
         * digits and grouping, 0..3 decimals), else [formatValue]; "" for a
         * missing point.
         */
        fun format(v: Double, formatter: at.exponential.ui.ffi.HostFormatter?): String {
            if (!v.isFinite()) return ""
            if (formatter != null) runCatching { return formatter.number(v, null, true) }
            return formatValue(v)
        }

        /** A value label without a formatter: integers plain, else up to 2 decimals. */
        fun formatValue(v: Double): String {
            if (kotlin.math.abs(v - kotlin.math.round(v)) < 1e-4) return kotlin.math.round(v).toLong().toString()
            return "%.2f".format(java.util.Locale.ROOT, v).trimEnd('0').trimEnd('.')
        }

        /** The category under x in a plot `width` wide with `n` slots. */
        fun categoryAt(x: Float, width: Float, n: Int): Int? {
            if (n <= 0 || width <= 0f || x < 0f || x > width) return null
            return min(floor(x / width * n).toInt(), n - 1)
        }

        /** The slice under (dx, dy) from the centre of a pie of radius r (clockwise from 12). */
        fun sliceAt(dx: Float, dy: Float, r: Float, hole: Float, values: List<Double>): Int? {
            val d = sqrt(dx * dx + dy * dy)
            if (d > r || d < hole * r) return null
            val total = values.sum()
            if (total <= 0.0) return null
            var a = atan2(dy, dx) + PI.toFloat() / 2f
            if (a < 0f) a += 2f * PI.toFloat()
            var acc = 0.0
            for ((i, v) in values.withIndex()) {
                acc += 2 * PI * v / total
                if (a <= acc) return i
            }
            return values.size - 1
        }
    }
}
