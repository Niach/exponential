package at.exponential.ui.paint

import at.exponential.ui.json.Props
import at.exponential.ui.json.list
import at.exponential.ui.json.num
import at.exponential.ui.json.str

/** The chart data the `Chart` native paints (bar / line / area / pie). */
class ChartModel(props: Props) {
    /** One data series. */
    data class Series(val name: String, val values: List<Double>, val tone: String?)

    /** `bar` (default), `line`, `area`, `pie`. */
    val kind: String = props.str("kind").ifEmpty { "bar" }
    val categories: List<String> = props.list("categories").map { it.displayText }
    val series: List<Series> = props.list("series").map { s ->
        Series(
            name = s["name"]?.displayText ?: "",
            values = (s["values"]?.array ?: emptyList()).mapNotNull { it.number },
            tone = s["tone"]?.string,
        )
    }
    val title: String = props.str("title")

    /** The plot height (dp, default 200). */
    val height: Float = (props.num("height") ?: 200.0).toFloat()

    /** The largest value (1 when nothing is positive). */
    val maxValue: Double
        get() {
            val m = series.flatMap { it.values }.maxOrNull() ?: 0.0
            return if (m > 0) m else 1.0
        }

    companion object {
        /** The legend entries (pie: categories; else series names when > 1). */
        fun legend(props: Props): List<String> {
            val series = props.list("series")
            if (props.str("kind") == "pie") return props.list("categories").map { it.displayText }
            if (series.size > 1) return series.map { it["name"]?.displayText ?: "" }
            return emptyList()
        }
    }
}
