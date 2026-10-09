package at.exponential.ui.compose

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.focusable
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import at.exponential.ui.paint.ChartModel
import at.exponential.ui.primitives.AvatarFallback
import at.exponential.ui.primitives.Rgba
import at.exponential.ui.theme.ResolvedTextStyle
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/** The colour of series / slice `i`: the core's resolved `seriesColors`, else the theme's chart1..8. */
private fun seriesColor(cx: LeafContext, i: Int): Color =
    cx.style.seriesColors?.getOrNull(i)
        ?: cx.themeColor("chart${i % 8 + 1}")
        ?: Rgba.hsl(AvatarFallback.seedHue(i.toString()) * 7, 0.6, 0.55).color

/**
 * `Chart` (round 1 §3, round 2 §7): bar / stackedBar / line / area / pie /
 * donut / sparkline from the core's `props.chart` numbers. The frame is the
 * WHOLE chart: the title line, the plot (grid lines and y labels at the
 * nice ticks, category labels, axis titles, value labels) and the legend
 * row share it. A tap (or the pointer) shows the category / slice tooltip;
 * focused (Tab), ArrowLeft / ArrowRight move it, Home / End jump, Escape
 * hides it. Numbers go through the surface formatter. In RTL the cartesian
 * kinds MIRROR (the first category on the right, the value axis on the
 * right); pie and donut do not. Missing points are skipped (as the web).
 */
@Composable
internal fun ChartLeaf(cx: LeafContext) {
    val chart = rememberPainted(cx, cx.props) { ChartModel(cx.props) } ?: return
    val gap = cx.spacing("xs")
    val axis = cx.part("Chart", "axis")
    val axisColor = axis.color ?: cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.6f)
    val axisSize = min(axis.px("fontSize") ?: 12f, 12f)
    val gridPart = cx.part("Chart", "grid")
    val gridColor = gridPart.color ?: gridPart.style.borderColor ?: cx.themeColor("border") ?: cx.ink.copy(alpha = 0.15f)
    val legendPart = cx.part("Chart", "legend")
    val legendColor = legendPart.color ?: axisColor
    val legendLh = legendPart.px("lineHeight") ?: 16f
    val valueColor = cx.part("Chart", "valueLabel").color ?: cx.ink
    val n = chart.count
    val fmt: (Double) -> String = { ChartModel.format(it, cx.model.formatter) }
    val rtl = cx.rtl && !chart.slices
    val sliceValues = chart.sliceValues
    val colors = (0 until max(max(chart.series.size, sliceValues.size), 1)).map { seriesColor(cx, it) }
    val titleH = if (chart.title.isEmpty()) 0f else cx.textStyle.lineHeight + gap
    val legendH = if (chart.legend.isEmpty()) 0f else legendLh + gap
    val r = cx.inner
    val width = r.width
    val plotH = max(1f, r.height - titleH - legendH)
    val axisTs = ResolvedTextStyle(axisSize, 400, 14f, cx.textStyle.fontFamily)
    val shaper = cx.model.textShaper()
    val labelW = if (chart.showAxes) (chart.tickLabels.maxOfOrNull { shaper.maxContent(it, axisTs) } ?: 0f) + 8f else 0f
    val pl = if (chart.spark) 1f else max(labelW, 4f)
    val pr = if (chart.spark) 1f else 8f
    val pt = if (chart.spark) 2f else if (chart.showValues || chart.yLabel.isNotEmpty()) 16f else 8f
    val pb = when {
        chart.spark || chart.slices -> 2f
        chart.showAxes -> 20f + if (chart.xLabel.isEmpty()) 0f else 14f
        else -> 8f
    }
    val innerW = max(1f, width - pl - pr)
    val innerH = max(1f, plotH - pt - pb)
    val span = max(chart.max - chart.min, 1e-9)
    // Every cartesian x goes through `mx` (the web's mirror).
    fun mx(x: Float) = if (rtl) width - x else x
    fun xOf(i: Int) = mx(pl + innerW * (i + 0.5f) / n)
    fun yOf(v: Double) = (pt + innerH - innerH * (v - chart.min) / span).toFloat()
    var hovered by remember(cx.node.id) { mutableStateOf<Int?>(null) }
    fun hit(p: Offset): Int? = when {
        chart.slices -> ChartModel.sliceAt(p.x - width / 2f, p.y - plotH / 2f, min(innerW, innerH) / 2f, chart.hole.toFloat(), sliceValues)
        chart.spark -> null
        else -> ChartModel.categoryAt(mx(p.x) - pl, innerW, n)
    }
    val axisStyle = cx.composeTextStyle(ts = axisTs, color = axisColor)
    val hoverBand = cx.ink.copy(alpha = 0.06f)
    InnerBox(cx) {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(gap.dp)) {
            if (chart.title.isNotEmpty()) {
                val title = cx.part("Chart", "title")
                val ts = cx.textStyle.copy(fontWeight = (title.props["fontWeight"]?.number ?: 600.0).toInt())
                Box(Modifier.height(cx.textStyle.lineHeight.dp)) { LeafLine(cx, chart.title, ts = ts) }
            }
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(plotH.dp)
                    .then(
                        if (chart.spark) {
                            Modifier
                        } else {
                            Modifier
                                .onFocusChanged { if (!it.isFocused) hovered = null }
                                .onKeyEvent { e ->
                                    if (e.type != KeyEventType.KeyDown || n <= 0) return@onKeyEvent false
                                    val count = if (chart.slices) sliceValues.size.coerceAtLeast(1) else n
                                    when (e.key) {
                                        Key.DirectionRight, Key.DirectionLeft -> {
                                            val delta = if ((e.key == Key.DirectionRight) != rtl) 1 else -1
                                            hovered = hovered?.let { (it + delta + count) % count } ?: if (delta > 0) 0 else count - 1
                                            true
                                        }
                                        Key.MoveHome -> { hovered = 0; true }
                                        Key.MoveEnd -> { hovered = count - 1; true }
                                        Key.Escape -> if (hovered != null) { hovered = null; true } else false
                                        else -> false
                                    }
                                }
                                .focusable()
                        },
                    )
                    .pointerInput(chart, width, plotH) {
                        detectTapGestures { p -> hovered = hit(p / density).let { if (it == hovered) null else it } }
                    }
                    .pointerInput(chart, width, plotH, "hover") {
                        awaitPointerEventScope {
                            while (true) {
                                val e = awaitPointerEvent()
                                when (e.type) {
                                    PointerEventType.Move, PointerEventType.Enter -> e.changes.firstOrNull()?.let { hovered = hit(it.position / density) }
                                    PointerEventType.Exit -> hovered = null
                                    else -> {}
                                }
                            }
                        }
                    },
            ) {
                Canvas(Modifier.fillMaxSize()) {
                    val d = density
                    fun at(x: Float, y: Float) = Offset(x * d, y * d)
                    if (chart.showGrid) {
                        for (t in chart.ticks) drawRect(gridColor, at(if (rtl) width - pl - innerW else pl, floor(yOf(t))), Size(innerW * d, max(1f, d)))
                    }
                    val h = hovered
                    if (h != null && !chart.slices && !chart.spark) {
                        val slot = innerW / n
                        drawRect(hoverBand, at(mx(pl + slot * h) - if (rtl) slot else 0f, pt), Size(slot * d, innerH * d))
                    }
                    when (chart.kind) {
                        "pie", "donut" -> drawSlices(sliceValues, colors, gridColor, width, plotH, min(innerW, innerH) / 2f, chart.hole.toFloat(), hovered)
                        "line", "area", "sparkline" -> chart.series.forEachIndexed { si, s ->
                            val color = colors.getOrElse(si) { gridColor }
                            val pts = s.values.mapIndexedNotNull { i, v -> if (v.isFinite()) at(xOf(i), yOf(v)) else null }
                            if (pts.isEmpty()) return@forEachIndexed
                            if (chart.kind == "area") {
                                val base = yOf(min(max(chart.min, 0.0), chart.max))
                                val area = Path().apply {
                                    moveTo(pts.first().x, base * d)
                                    pts.forEach { lineTo(it.x, it.y) }
                                    lineTo(pts.last().x, base * d)
                                    close()
                                }
                                drawPath(area, color.copy(alpha = color.alpha * 0.2f))
                            }
                            if (pts.size > 1) {
                                val line = Path().apply {
                                    moveTo(pts[0].x, pts[0].y)
                                    for (p in pts.drop(1)) lineTo(p.x, p.y)
                                }
                                drawPath(line, color, style = Stroke(width = (if (chart.spark) 1.5f else 2f) * d))
                            } else {
                                drawCircle(color, 2f * d, pts[0])
                            }
                            if (h != null && !chart.spark) s.values.getOrNull(h)?.takeIf { it.isFinite() }?.let { drawCircle(color, 3.5f * d, at(xOf(h), yOf(it))) }
                        }
                        "stackedBar" -> {
                            val slot = innerW / n
                            val bw = slot * 0.6f
                            for (i in 0 until n) {
                                var acc = 0.0
                                chart.series.forEachIndexed { si, s ->
                                    val v = s.values.getOrElse(i) { 0.0 }.let { if (it.isFinite()) max(it, 0.0) else 0.0 }
                                    val y0 = yOf(acc)
                                    val y1 = yOf(acc + v)
                                    acc += v
                                    drawRect(colors.getOrElse(si) { gridColor }, at(mx(pl + slot * i + slot * 0.2f) - if (rtl) bw else 0f, y1), Size(bw * d, max(0f, y0 - y1) * d))
                                }
                            }
                        }
                        else -> {
                            val slot = innerW / n
                            val bw = slot * 0.7f / max(chart.series.size, 1)
                            val zero = yOf(0.0.coerceIn(chart.min, chart.max))
                            chart.series.forEachIndexed { si, s ->
                                s.values.forEachIndexed { i, v ->
                                    if (!v.isFinite()) return@forEachIndexed
                                    val y = yOf(v)
                                    val top = min(y, zero)
                                    val bh = kotlin.math.abs(zero - y)
                                    val x = mx(pl + slot * i + slot * 0.15f + bw * si) - if (rtl) bw else 0f
                                    drawRoundRect(colors.getOrElse(si) { gridColor }, at(x, top), Size(bw * d, bh * d), androidx.compose.ui.geometry.CornerRadius(2f * d))
                                }
                            }
                        }
                    }
                    val measurer = shaper.measurer
                    fun label(text: String, x: Float, y: Float, w: Float, align: TextAlign, color: Color = axisColor) {
                        if (text.isEmpty() || w <= 0f) return
                        val layout = measurer.measure(text, axisStyle.copy(color = color, textAlign = align), overflow = TextOverflow.Ellipsis, softWrap = false, maxLines = 1, constraints = Constraints.fixedWidth(max(1, (w * d).toInt())))
                        drawText(layout, topLeft = at(x, y))
                    }
                    if (chart.showAxes) {
                        chart.ticks.forEachIndexed { i, t ->
                            val text = chart.tickLabels.getOrElse(i) { fmt(t) }
                            if (rtl) label(text, width - pl + 6f, yOf(t) - 7f, pl - 6f, TextAlign.Left) else label(text, 0f, yOf(t) - 7f, pl - 6f, TextAlign.Right)
                        }
                        val slot = innerW / n
                        chart.categories.forEachIndexed { i, c -> label(c, xOf(i) - slot / 2f, pt + innerH + 4f, slot, TextAlign.Center) }
                        if (chart.xLabel.isNotEmpty()) label(chart.xLabel, if (rtl) width - pl - innerW else pl, pt + innerH + 20f, innerW, TextAlign.Center)
                        if (chart.yLabel.isNotEmpty()) label(chart.yLabel, 0f, 0f, width, if (rtl) TextAlign.Right else TextAlign.Left)
                    }
                    if (chart.showValues && !chart.slices && !chart.spark) {
                        val slot = innerW / n
                        val bw = slot * 0.7f / max(chart.series.size, 1)
                        chart.series.forEachIndexed { si, s ->
                            s.values.forEachIndexed { i, v ->
                                if (!v.isFinite()) return@forEachIndexed
                                val x = if (chart.kind == "bar") mx(pl + slot * i + slot * 0.15f + bw * si + bw / 2f) else xOf(i)
                                label(fmt(v), x - 20f, yOf(v) - 15f, 40f, TextAlign.Center, valueColor)
                            }
                        }
                    }
                }
                hovered?.let { h -> ChartTooltip(cx, chart, h, colors, if (chart.slices) width / 2f else xOf(h), pt, width, fmt) }
            }
            if (chart.legend.isNotEmpty()) {
                val lts = ResolvedTextStyle(axisSize, 400, legendLh, cx.textStyle.fontFamily)
                Row(Modifier.height(legendLh.dp), horizontalArrangement = Arrangement.spacedBy(cx.spacing("sm").dp), verticalAlignment = Alignment.CenterVertically) {
                    chart.legend.forEachIndexed { i, name ->
                        Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
                            Box(Modifier.size(8.dp).background(colors.getOrElse(i) { gridColor }, RoundedCornerShape(2.dp)))
                            LeafLine(cx, name, color = legendColor, ts = lts)
                        }
                    }
                }
            }
        }
    }
}

/** Pie / donut slices clockwise from twelve o'clock; the hovered one grows 3 dp. */
private fun DrawScope.drawSlices(values: List<Double>, colors: List<Color>, fallback: Color, w: Float, h: Float, r: Float, hole: Float, hovered: Int?) {
    val total = max(values.sum(), 1e-9)
    val d = density
    val cx0 = w / 2f
    val cy0 = h / 2f
    var angle = -PI.toFloat() / 2f
    for ((i, v) in values.withIndex()) {
        val sweep = (2 * PI * v / total).toFloat()
        val steps = max(2, kotlin.math.ceil(sweep / (2 * PI.toFloat()) * 96f).toInt())
        val grow = if (hovered == i) 3f else 0f
        val inner = hole * r
        val p = Path()
        if (inner > 0f) p.moveTo((cx0 + inner * cos(angle)) * d, (cy0 + inner * sin(angle)) * d) else p.moveTo(cx0 * d, cy0 * d)
        for (k in 0..steps) {
            val a = angle + sweep * k / steps
            p.lineTo((cx0 + (r + grow) * cos(a)) * d, (cy0 + (r + grow) * sin(a)) * d)
        }
        if (inner > 0f) {
            for (k in steps downTo 0) {
                val a = angle + sweep * k / steps
                p.lineTo((cx0 + inner * cos(a)) * d, (cy0 + inner * sin(a)) * d)
            }
        }
        p.close()
        drawPath(p, colors.getOrElse(i) { fallback })
        angle += sweep
    }
}

/** The tooltip of the hovered category (each series' value) or slice, the `Chart/tooltip` recipe. */
@Composable
private fun ChartTooltip(cx: LeafContext, chart: ChartModel, h: Int, colors: List<Color>, anchorX: Float, top: Float, width: Float, fmt: (Double) -> String) {
    val tip = cx.part("Chart", "tooltip")
    val fg = tip.color ?: cx.ink
    val fs = tip.px("fontSize") ?: 12f
    val pad = tip.px("padding") ?: 8f
    val bg = tip.style.background ?: cx.themeColor("popover") ?: Color.Black
    val ts = ResolvedTextStyle(fs, 400, fs + 4f, cx.textStyle.fontFamily)
    val label = chart.categories.getOrNull(h) ?: ""
    val lines: List<Pair<String, Color>> = if (chart.slices) {
        listOf("$label: ${chart.sliceValues.getOrNull(h)?.let(fmt) ?: ""}" to colors.getOrElse(h) { fg })
    } else {
        chart.series.mapIndexed { i, s ->
            // A missing point reads "—" (the web's).
            val v = s.values.getOrNull(h)?.let { if (it.isFinite()) fmt(it) else "—" } ?: "—"
            (if (s.name.isEmpty()) v else "${s.name}: $v") to colors.getOrElse(i) { fg }
        }
    }
    val left = min(anchorX + 8f, max(width - 120f, 0f))
    Column(
        Modifier
            .absoluteOffset(left.dp, top.dp)
            .wrapContentSize(unbounded = true)
            .background(bg, RoundedCornerShape(tip.style.radius.dp))
            .padding(pad.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        if (!chart.slices && label.isNotEmpty()) LeafLine(cx, label, color = fg, ts = ts.copy(fontWeight = 600))
        for ((text, color) in lines) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(8.dp).background(color, RoundedCornerShape(2.dp)))
                BasicText(text, style = cx.composeTextStyle(ts = ts, color = fg), maxLines = 1, softWrap = false)
            }
        }
    }
}
