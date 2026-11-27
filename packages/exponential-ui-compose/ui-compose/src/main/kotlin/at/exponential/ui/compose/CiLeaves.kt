package at.exponential.ui.compose

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.FocusInteraction
import androidx.compose.foundation.interaction.HoverInteraction
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.flag
import at.exponential.ui.json.list
import at.exponential.ui.json.str
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.fire
import at.exponential.ui.primitives.CiLog
import at.exponential.ui.primitives.JobGraph
import at.exponential.ui.theme.PaintStyle
import at.exponential.ui.theme.PartStyle
import at.exponential.ui.theme.ResolvedTextStyle
import kotlin.math.max
import kotlin.math.min

// #EXP-2204 (packages/exponential-ui/docs/ci-components-contract.md §1,
// §4, §5): the LogLines and JobGraph leaves. The core builds each as ONE
// leaf (its state resolved: `open`, `query`, `follow`, `timestamps`,
// `selection`; JobGraph `compact`) and owns every write; the leaf paints
// from the theme's recipes with the primitives' pure rules (`CiLog`,
// `JobGraph`) and sends `toggle`, `reveal`, `search`, `follow`,
// `timestamps`, `select` (the SwiftUI painter's CiLeaves.swift).

/** A [JsonValue] as the plain Kotlin JSON the primitives read (maps, lists, Double, String, Boolean, null). */
internal fun plainJson(v: JsonValue): Any? = when (v) {
    JsonValue.Null -> null
    is JsonValue.Bool -> v.v
    is JsonValue.Num -> v.v
    is JsonValue.Str -> v.v
    is JsonValue.Arr -> v.v.map(::plainJson)
    is JsonValue.Obj -> v.v.mapValues { plainJson(it.value) }
}

/** A part's text style over [base] (size, weight, line height, family; tabular figures on demand). */
private fun partText(p: PartStyle, base: ResolvedTextStyle, tabular: Boolean = false): ResolvedTextStyle = ResolvedTextStyle(
    p.style.fontSize ?: p.px("fontSize") ?: base.fontSize,
    p.style.fontWeight ?: p.props["fontWeight"]?.number?.toInt() ?: base.fontWeight,
    p.style.lineHeight ?: p.px("lineHeight") ?: base.lineHeight,
    p.fontFamily ?: base.fontFamily,
    fontFeatures = if (tabular) "tnum" else base.fontFeatures,
)

/** A part's box: fill, hairline border, radius (no opacity: the caller fades content). */
private fun Modifier.ciBox(style: PaintStyle): Modifier {
    val shape = RoundedCornerShape(style.radius.dp)
    var m = this
    style.background?.let { m = m.background(it, shape) }
    if (style.borderWidth > 0f) style.borderColor?.let { m = m.border(style.borderWidth.dp, it, shape) }
    return m
}

/**
 * THE status mark (contract §1): the glyph of `style.json ciStatus` in the
 * recipe's colour; `running` turns once per `spinMs` (linear), static under
 * reduced motion. Decorative: the row names the status.
 */
@Composable
internal fun CiStatusGlyph(status: String, color: Color, size: Float, model: SurfaceModel) {
    val entry = CiLog.entry(status)
    if (entry.spin && !model.reducedMotion) {
        val turn = rememberInfiniteTransition(label = "ci-status")
        val angle by turn.animateFloat(0f, 360f, infiniteRepeatable(tween(CiLog.SPIN_MS.toInt(), easing = LinearEasing), RepeatMode.Restart), label = "ci-spin")
        IconView(entry.icon, size, color, model, Modifier.graphicsLayer { rotationZ = angle })
    } else {
        IconView(entry.icon, size, color, model)
    }
}

// ---------------------------------------------------------------------------
// LogLines
// ---------------------------------------------------------------------------

/**
 * The model, search and ANSI tokens of one LogLines, cached by their inputs
 * (a long log is parsed once, a line tokenized when it first shows).
 */
private class LogCache {
    private var linesKey: JsonValue? = null
    private var sectionsKey: JsonValue? = null
    private var queryKey: String? = null
    var version = 0
    var model = CiLog.Model()
    /** The widest content line, in code points of its escape-free text. */
    var widest = 0
    var matches: List<CiLog.Match> = emptyList()
    var byLine: Map<Int, List<Int>> = emptyMap()
    private val tokens = HashMap<Int, List<Pair<String, String>>>()

    fun update(lines: JsonValue?, sections: JsonValue?, query: String) {
        if (lines != linesKey || sections != sectionsKey || version == 0) {
            linesKey = lines
            sectionsKey = sections
            model = CiLog.model(lines?.let(::plainJson), sections?.let(::plainJson))
            widest = model.lines.maxOfOrNull { CiLog.stripAnsi(it.text).let { t -> t.codePointCount(0, t.length) } } ?: 0
            tokens.clear()
            queryKey = null
            version++
        }
        if (query != queryKey) {
            queryKey = query
            matches = CiLog.search(model, query)
            val by = HashMap<Int, MutableList<Int>>()
            matches.forEachIndexed { i, m -> by.getOrPut(m.line) { ArrayList() }.add(i) }
            byLine = by
        }
    }

    /** The `ansi` tokens (text, kind) of content line [n]: the catalog's ONE tokenizer, no SGR state across lines. */
    fun tokens(n: Int): List<Pair<String, String>> = tokens.getOrPut(n) {
        val line = model.line(n) ?: return@getOrPut emptyList()
        val parsed = runCatching { JsonValue.parse(at.exponential.ui.ffi.tokenizeCodeJson(line.text, "ansi")) }.getOrNull()
        val first = parsed?.array?.firstOrNull()?.array ?: emptyList()
        first.mapNotNull { t -> t["text"]?.string?.takeIf { it.isNotEmpty() }?.let { it to (t["kind"]?.string ?: "plain") } }
    }
}

/** A code-point offset into [s] as a UTF-16 index (clamped). */
private fun cpIndex(s: String, cp: Int): Int = runCatching { s.offsetByCodePoints(0, cp) }.getOrElse { s.length }

@Composable
internal fun LogLinesLeaf(cx: LeafContext) {
    val p = cx.props
    val model = cx.model
    val query = p.str("query")
    val cache = remember { LogCache() }
    cache.update(p["lines"], p["sections"], query)
    val log = cache.model
    val version = cache.version
    val open = p.list("open").mapNotNull { it.number?.toInt() }
    val rows = remember(version, open) { CiLog.rows(log, open) }
    val follow = p["follow"]?.bool ?: true
    val timestamps = p.flag("timestamps")
    val selectionText = p["selection"]?.string
    val selection = CiLog.parseAnchor(selectionText)
    val total = cache.matches.size
    var current by remember { mutableIntStateOf(0) }
    // A line to show in the top third (a selection), a match line to show.
    var pendingTop by remember { mutableStateOf<Int?>(null) }
    var pendingMatch by remember { mutableStateOf<Int?>(null) }
    var handledSelection by remember { mutableStateOf<String?>(null) }
    var ownSelection by remember { mutableStateOf<String?>(null) }

    fun fire(event: String, payload: Map<String, JsonValue>) = model.fire(cx.index, event, JsonValue.Obj(payload))
    fun step(by: Int) {
        if (total == 0) return
        current = ((current + by) % total + total) % total
        val line = cache.matches[current].line
        fire("reveal", mapOf("line" to JsonValue.Num(line.toDouble())))
        pendingMatch = line
    }

    // Typing a query: the current match is the first, and it shows.
    LaunchedEffect(query) {
        current = 0
        if (cache.matches.isNotEmpty()) {
            val line = cache.matches[0].line
            fire("reveal", mapOf("line" to JsonValue.Num(line.toDouble())))
            pendingMatch = line
        }
    }
    // A new selection (initial or from outside): its section opens, its
    // start shows in the top third, Follow turns off.
    LaunchedEffect(selectionText) {
        val sel = selectionText ?: return@LaunchedEffect
        if (sel == handledSelection) return@LaunchedEffect
        handledSelection = sel
        if (sel == ownSelection) return@LaunchedEffect
        val r = CiLog.parseAnchor(sel) ?: return@LaunchedEffect
        fire("reveal", mapOf("line" to JsonValue.Num(r.start.toDouble())))
        if (follow) fire("follow", mapOf("follow" to JsonValue.Bool(false)))
        pendingTop = r.start
    }

    val root = cx.part("LogLines", "root", emptyMap())
    val gap = if (root.style.gap > 0f) root.style.gap else cx.spacing("sm")
    Column(Modifier.size(cx.size.width.dp, cx.size.height.dp)) {
        LogToolbar(cx, query, total, current, follow, timestamps, p.flag("compact"), ::step) { e, v -> fire(e, v) }
        Spacer(Modifier.height(gap.dp))
        Box(Modifier.fillMaxWidth().weight(1f)) {
            LogBody(
                cx, cache, rows, open, follow, timestamps, selection,
                current = current,
                pendingTop = pendingTop, clearTop = { pendingTop = null },
                pendingMatch = pendingMatch, clearMatch = { pendingMatch = null },
                fire = { e, v -> fire(e, v) },
                select = { n ->
                    val anchor = CiLog.extendAnchor(selectionText, n, false)
                    ownSelection = anchor
                    fire("select", mapOf("line" to JsonValue.Num(n.toDouble()), "shift" to JsonValue.Bool(false)))
                    // A pressed number would scroll away under Follow.
                    if (follow) fire("follow", mapOf("follow" to JsonValue.Bool(false)))
                },
            )
        }
    }
}

/**
 * The toolbar: find (search, `n of m`, prev, next) then the Follow and
 * Timestamps toggles, end-aligned. The core's `compact` (the surface below
 * `md`; it sized the leaf for it) = two rows `$spacing.sm` apart: the search
 * fills row 1, the toggles take row 2.
 */
@Composable
private fun LogToolbar(
    cx: LeafContext,
    query: String,
    total: Int,
    current: Int,
    follow: Boolean,
    timestamps: Boolean,
    compact: Boolean,
    step: (Int) -> Unit,
    fire: (String, Map<String, JsonValue>) -> Unit,
) {
    val toolbar = cx.part("LogLines", "toolbar", emptyMap())
    val barGap = if (toolbar.style.gap > 0f) toolbar.style.gap else cx.spacing("sm")
    val barHeight = toolbar.height ?: cx.control("input", 36f)
    val countPart = cx.part("LogLines", "count", emptyMap())
    val cts = partText(countPart, cx.textStyle, tabular = true)
    val shown = if (total == 0) 0 else min(current, total - 1) + 1
    val countText = cx.string("matchCount", mapOf("current" to shown, "total" to total))
    val find: @Composable RowScope.() -> Unit = {
        // The search field: compact = the rest of row 1; else style.json
        // `logView.searchWidth`, shrinking to what is left.
        val field = if (compact) Modifier.weight(1f, fill = true) else Modifier.weight(1f, fill = false).widthIn(max = CiLog.SEARCH_WIDTH.toFloat().dp).fillMaxWidth()
        LogSearchField(cx, query, barHeight, field, step, fire)
        if (query.isNotEmpty()) {
            BasicText(countText, style = cx.composeTextStyle(ts = cts, color = countPart.color ?: cx.ink), maxLines = 1, softWrap = false)
        }
        LogStepButton(cx, "prev", "ui-chevron-up", "previousMatch", total > 0) { step(-1) }
        LogStepButton(cx, "next", "ui-chevron-down", "nextMatch", total > 0) { step(1) }
    }
    val toggleRow: @Composable RowScope.() -> Unit = {
        LogToggle(cx, "follow", follow) { fire("follow", mapOf("follow" to JsonValue.Bool(!follow))) }
        LogToggle(cx, "timestamps", timestamps) { fire("timestamps", mapOf("timestamps" to JsonValue.Bool(!timestamps))) }
    }
    if (compact) {
        Row(Modifier.fillMaxWidth().height(barHeight.dp), horizontalArrangement = Arrangement.spacedBy(barGap.dp), verticalAlignment = Alignment.CenterVertically) { find() }
        Spacer(Modifier.height(cx.spacing("sm").dp))
        Row(Modifier.fillMaxWidth().height(barHeight.dp), horizontalArrangement = Arrangement.spacedBy(barGap.dp, Alignment.End), verticalAlignment = Alignment.CenterVertically) { toggleRow() }
    } else {
        Row(Modifier.fillMaxWidth().height(barHeight.dp), horizontalArrangement = Arrangement.spacedBy(barGap.dp), verticalAlignment = Alignment.CenterVertically) {
            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(barGap.dp), verticalAlignment = Alignment.CenterVertically) { find() }
            toggleRow()
        }
    }
}

/** The search field: an Input look (`search` part, `focus` state), `$string.searchLog`, Enter = next, Shift+Enter = previous. */
@Composable
private fun LogSearchField(cx: LeafContext, query: String, height: Float, modifier: Modifier, step: (Int) -> Unit, fire: (String, Map<String, JsonValue>) -> Unit) {
    var text by remember { mutableStateOf(query) }
    var focused by remember { mutableStateOf(false) }
    LaunchedEffect(query) { if (query != text) text = query }
    val look = cx.part("LogLines", "search", emptyMap(), if (focused) listOf("focus") else emptyList())
    val ts = partText(look, cx.textStyle)
    val ink = look.color ?: cx.ink
    val placeholder = cx.string("searchLog")
    val style = cx.composeTextStyle(ts = ts, color = ink)
    BasicTextField(
        value = text,
        onValueChange = { t ->
            text = t
            if (t != query) fire("search", mapOf("query" to JsonValue.Str(t)))
        },
        singleLine = true,
        textStyle = style,
        cursorBrush = SolidColor(ink),
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search, autoCorrectEnabled = false),
        keyboardActions = KeyboardActions(onSearch = { step(1) }, onDone = { step(1) }),
        modifier = modifier
            .height((look.height ?: height).dp)
            .onPreviewKeyEvent { e ->
                if (e.type == KeyEventType.KeyDown && (e.key == Key.Enter || e.key == Key.NumPadEnter)) {
                    step(if (e.isShiftPressed) -1 else 1)
                    true
                } else {
                    false
                }
            }
            .onFocusChanged { focused = it.isFocused }
            .semantics { contentDescription = placeholder },
        decorationBox = { inner ->
            Box(Modifier.fillMaxSize().ciBox(look.style).padding(horizontal = look.style.paddingHorizontal.dp), contentAlignment = Alignment.CenterStart) {
                if (text.isEmpty()) {
                    BasicText(placeholder, style = cx.composeTextStyle(ts = ts, color = cx.themeColor("mutedForeground") ?: ink.copy(alpha = 0.6f)), maxLines = 1, softWrap = false, overflow = TextOverflow.Ellipsis)
                }
                inner()
            }
        },
    )
}

/** A prev / next icon button (`disabled` without matches). */
@Composable
private fun LogStepButton(cx: LeafContext, part: String, icon: String, label: String, enabled: Boolean, action: () -> Unit) {
    val look = cx.part("LogLines", part, emptyMap(), if (enabled) emptyList() else listOf("disabled"))
    val side = look.width ?: cx.control("buttonSm", 32f)
    val name = cx.string(label)
    Box(
        Modifier
            .size(side.dp, (look.height ?: side).dp)
            .ciBox(look.style)
            .alpha(look.style.opacity ?: 1f)
            .clickable(enabled = enabled, role = Role.Button, onClickLabel = name, onClick = action)
            .semantics { contentDescription = name },
        contentAlignment = Alignment.Center,
    ) {
        IconView(icon, cx.control("iconSm", 16f), look.color ?: cx.ink, cx.model)
    }
}

/** Follow / Timestamps: the Toggle outline look, `selected` when on. */
@Composable
private fun LogToggle(cx: LeafContext, part: String, on: Boolean, action: () -> Unit) {
    val look = cx.part("LogLines", part, emptyMap(), if (on) listOf("selected") else emptyList())
    val ts = partText(look, cx.textStyle)
    val name = cx.string(part)
    Box(
        Modifier
            .height((look.height ?: cx.control("buttonSm", 32f)).dp)
            .ciBox(look.style)
            .clickable(role = Role.Button, onClick = action)
            .semantics {
                contentDescription = name
                selected = on
            }
            .padding(horizontal = (look.style.paddingHorizontal + look.style.borderWidth).dp),
        contentAlignment = Alignment.Center,
    ) {
        BasicText(name, style = cx.composeTextStyle(ts = ts, color = look.color ?: cx.ink), maxLines = 1, softWrap = false, modifier = Modifier.clearAndSetSemantics {})
    }
}

/**
 * The body: the windowed rows of `CiLog.rows` (a LazyColumn of fixed-height
 * rows: a step row 40, a line the `line` recipe's lineHeight), scrolling
 * both ways (lines never wrap; a step row stays the body's width). Follow
 * sticks it to the bottom; a person scrolling up turns Follow off.
 */
@Composable
private fun LogBody(
    cx: LeafContext,
    cache: LogCache,
    rows: List<CiLog.Row>,
    open: List<Int>,
    follow: Boolean,
    timestamps: Boolean,
    selection: CiLog.LineRange?,
    current: Int,
    pendingTop: Int?,
    clearTop: () -> Unit,
    pendingMatch: Int?,
    clearMatch: () -> Unit,
    fire: (String, Map<String, JsonValue>) -> Unit,
    select: (Int) -> Unit,
) {
    val log = cache.model
    val shaper = cx.model.textShaper()
    val look = cx.part("LogLines", "body", emptyMap())
    val lineLook = cx.part("LogLines", "line", emptyMap())
    val gutterLook = cx.part("LogLines", "gutter", emptyMap())
    val timeLook = cx.part("LogLines", "time", emptyMap())
    val sectionLook = cx.part("LogLines", "section", emptyMap())
    val lh = lineLook.px("lineHeight") ?: lineLook.style.lineHeight ?: 22.75f
    val sectionHeight = sectionLook.height ?: 40f
    val code = partText(look, cx.textStyle).copy(lineHeight = lh)
    val ch = shaper.maxContent("0", code)
    val digits = log.lines.size.toString().length
    val gutterWidth = (digits + 1) * ch + 2 * gutterLook.style.paddingHorizontal
    val timeWidth = if (timestamps) shaper.maxContent("00:00:00", code) + 2 * timeLook.style.paddingHorizontal else 0f
    val lead = max(0f, lineLook.style.paddingHorizontal - gutterLook.style.paddingHorizontal)
    val border = look.style.borderWidth
    val viewport = max(0f, cx.size.width - 2 * border)
    val indent = if (log.sections.isNotEmpty()) CiLog.INDENT.toFloat() else 0f
    val contentWidth = max(viewport, lead + indent + gutterWidth + timeWidth + cache.widest * ch + lineLook.style.paddingHorizontal)
    val listState = rememberLazyListState()
    val hs = rememberScrollState()
    val density = androidx.compose.ui.platform.LocalDensity.current.density
    val padV = look.style.paddingVertical

    // Follow: the end shows as lines arrive and when it turns on.
    LaunchedEffect(follow, rows.size) {
        if (follow && rows.isNotEmpty() && pendingTop == null) listState.scrollToItem(rows.lastIndex)
    }
    // A selection's start: the top third of the body.
    LaunchedEffect(pendingTop, rows) {
        val n = pendingTop ?: return@LaunchedEffect
        val at = rows.indexOf(CiLog.Row.LineRow(n))
        if (at < 0) return@LaunchedEffect
        val viewportPx = listState.layoutInfo.viewportSize.height
        val third = if (viewportPx > 0) (viewportPx / density / 3f / lh).toInt() else 0
        listState.scrollToItem(max(0, at - third))
        clearTop()
    }
    // A match stepped to: shown when out of view (centred).
    LaunchedEffect(pendingMatch, rows, current) {
        val n = pendingMatch ?: return@LaunchedEffect
        val at = rows.indexOf(CiLog.Row.LineRow(n))
        if (at < 0) return@LaunchedEffect
        val visible = listState.layoutInfo.visibleItemsInfo
        val inView = visible.any { it.index == at && it.offset >= 0 && it.offset + it.size <= listState.layoutInfo.viewportEndOffset }
        if (!inView) {
            val count = max(1, visible.size)
            listState.scrollToItem(max(0, at - count / 2))
        }
        clearMatch()
    }
    val followNow by rememberUpdatedState(follow)
    val fireNow by rememberUpdatedState(fire)
    val userScroll = remember {
        object : NestedScrollConnection {
            var sent = false
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (!followNow) sent = false
                // A person scrolling UP (content moving down) leaves the end.
                if (source == NestedScrollSource.UserInput && available.y > 0.5f && followNow && !sent) {
                    sent = true
                    fireNow("follow", mapOf("follow" to JsonValue.Bool(false)))
                }
                return Offset.Zero
            }
        }
    }
    val shape = RoundedCornerShape(look.style.radius.dp)
    Box(Modifier.fillMaxSize().ciBox(look.style).padding(border.dp).clip(shape)) {
        Box(Modifier.fillMaxSize().horizontalScroll(hs)) {
            LazyColumn(
                state = listState,
                modifier = Modifier
                    .width(contentWidth.dp)
                    .fillMaxHeight()
                    .nestedScroll(userScroll)
                    .semantics { liveRegion = LiveRegionMode.Polite },
                contentPadding = PaddingValues(vertical = padV.dp),
            ) {
                itemsIndexed(rows, key = { _, r -> if (r is CiLog.Row.SectionRow) "s${r.section}" else "l${(r as CiLog.Row.LineRow).n}" }) { _, row ->
                    when (row) {
                        is CiLog.Row.SectionRow -> LogSectionRow(cx, log.sections[row.section], row.section in open, viewport, sectionHeight, hs) {
                            fire("toggle", mapOf("section" to JsonValue.Num(row.section.toDouble())))
                        }
                        is CiLog.Row.LineRow -> LogLineRow(
                            cx, cache, row.n, code, lh, lead, indent, gutterWidth, timestamps, timeWidth, contentWidth,
                            selected = selection?.contains(row.n) == true,
                            current = current,
                            select = select,
                        )
                    }
                }
            }
        }
    }
}

/** A step row: chevron, status glyph, title, duration; pressing it folds the section. */
@Composable
private fun LogSectionRow(cx: LeafContext, s: CiLog.Section, open: Boolean, width: Float, height: Float, scrollX: ScrollState, toggle: () -> Unit) {
    val props: Props = mapOf("status" to JsonValue.Str(s.status), "open" to JsonValue.Bool(open))
    val hover = remember { MutableInteractionSource() }
    val hovered by hover.collectIsHoveredAsState()
    val look = cx.part("LogLines", "section", props, if (hovered) listOf("hover") else emptyList())
    val chevron = cx.part("LogLines", "chevron", props)
    val icon = cx.part("LogLines", "sectionIcon", props)
    val title = cx.part("LogLines", "sectionTitle", props)
    val meta = cx.part("LogLines", "sectionMeta", props)
    val base = partText(look, cx.textStyle)
    val tts = partText(title, base)
    val mts = partText(meta, base, tabular = true)
    val iconSize = icon.width ?: cx.control("iconSm", 16f)
    val name = "${s.title}, ${cx.string(CiLog.entry(s.status).label)}"
    Row(
        Modifier
            // Sticky at the inline start while long lines scroll sideways.
            .graphicsLayer { translationX = scrollX.value.toFloat() }
            .width(width.dp)
            .height(height.dp)
            .background(look.style.background ?: Color.Transparent)
            .hoverable(hover)
            .clickable(interactionSource = hover, indication = null, role = Role.Button, onClick = toggle)
            .clearAndSetSemantics {
                contentDescription = name
                role = Role.Button
                if (open) collapse { toggle(); true } else expand { toggle(); true }
                onClick { toggle(); true }
            }
            .padding(horizontal = look.style.paddingHorizontal.dp),
        horizontalArrangement = Arrangement.spacedBy((if (look.style.gap > 0f) look.style.gap else cx.spacing("sm")).dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.graphicsLayer { rotationZ = if (open) 90f else 0f }) {
            IconView("ui-chevron-right", chevron.width ?: iconSize, chevron.color ?: cx.ink, cx.model)
        }
        CiStatusGlyph(s.status, icon.color ?: cx.ink, iconSize, cx.model)
        BasicText(s.title, style = cx.composeTextStyle(ts = tts, color = title.color ?: cx.ink), maxLines = 1, softWrap = false, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = true))
        s.duration?.let { d -> BasicText(d, style = cx.composeTextStyle(ts = mts, color = meta.color ?: cx.ink), maxLines = 1, softWrap = false) }
    }
}

/** A content line: gutter number (a link selecting the line), time, the ansi tokens in their colours, the search marks behind them. */
@Composable
private fun LogLineRow(
    cx: LeafContext,
    cache: LogCache,
    n: Int,
    code: ResolvedTextStyle,
    lh: Float,
    lead: Float,
    indent: Float,
    gutterWidth: Float,
    timestamps: Boolean,
    timeWidth: Float,
    width: Float,
    selected: Boolean,
    current: Int,
    select: (Int) -> Unit,
) {
    val line = cache.model.line(n)
    val look = cx.part("LogLines", "line", emptyMap(), if (selected) listOf("selected") else emptyList())
    val hover = remember { MutableInteractionSource() }
    val hovered by hover.collectIsHoveredAsState()
    val gutterLook = cx.part("LogLines", "gutter", emptyMap(), if (hovered) listOf("hover") else emptyList())
    val timeLook = cx.part("LogLines", "time", emptyMap())
    val hits = cache.byLine[n]
    val currentHere = hits?.contains(current) == true
    val theme = cx.model.effectiveTheme
    val mode = cx.model.mode
    val text = remember(n, cache.version, hits, if (currentHere) current else -1, theme, mode, line?.kind) {
        lineText(cx, cache, n, line?.kind, hits, current)
    }
    val linkName = cx.string("lineAnchor", mapOf("line" to n))
    Row(
        Modifier.width(width.dp).height(lh.dp).background(look.style.background ?: Color.Transparent),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Spacer(Modifier.width((lead + if ((line?.section ?: -1) >= 0) indent else 0f).dp))
        Box(
            Modifier
                .width(gutterWidth.dp)
                .height(lh.dp)
                .hoverable(hover)
                .clickable(interactionSource = hover, indication = null) { select(n) }
                .clearAndSetSemantics {
                    contentDescription = linkName
                    onClick { select(n); true }
                }
                .padding(horizontal = gutterLook.style.paddingHorizontal.dp),
            contentAlignment = Alignment.CenterEnd,
        ) {
            BasicText(n.toString(), style = cx.composeTextStyle(ts = code, color = gutterLook.color ?: cx.ink), maxLines = 1, softWrap = false, modifier = Modifier.alpha(gutterLook.style.opacity ?: 1f))
        }
        if (timestamps) {
            Box(Modifier.width(timeWidth.dp).padding(horizontal = timeLook.style.paddingHorizontal.dp), contentAlignment = Alignment.CenterStart) {
                BasicText(line?.time ?: "", style = cx.composeTextStyle(ts = code, color = timeLook.color ?: cx.ink), maxLines = 1, softWrap = false)
            }
        }
        BasicText(text, style = cx.composeTextStyle(ts = code), maxLines = 1, softWrap = false, overflow = TextOverflow.Visible)
    }
}

/** Line [n]'s tokens in the `token {kind}` colours (a prefixed line paints every token with its kind), the match marks as backgrounds. */
private fun lineText(cx: LeafContext, cache: LogCache, n: Int, kind: String?, hits: List<Int>?, current: Int): AnnotatedString {
    val b = AnnotatedString.Builder()
    val looks = HashMap<String, SpanStyle>()
    for ((t, k0) in cache.tokens(n)) {
        val k = kind ?: k0
        val span = looks.getOrPut(k) {
            val p = cx.part("LogLines", "token", mapOf("kind" to JsonValue.Str(k)))
            SpanStyle(
                color = p.color ?: cx.ink,
                fontWeight = p.style.fontWeight?.let { FontWeight(it) },
                fontStyle = if (p.style.fontStyle == "italic") FontStyle.Italic else null,
            )
        }
        val start = b.length
        b.append(t)
        b.addStyle(span, start, b.length)
    }
    if (!hits.isNullOrEmpty()) {
        val s = b.toAnnotatedString().text
        val match = cx.part("LogLines", "match", emptyMap()).style.background
        val now = cx.part("LogLines", "match", emptyMap(), listOf("selected")).style.background
        for (i in hits) {
            val m = cache.matches[i]
            val lo = cpIndex(s, m.start)
            val hi = cpIndex(s, m.end)
            if (hi <= lo || hi > s.length) continue
            val fill = (if (i == current) now else match) ?: cx.ink.copy(alpha = 0.2f)
            b.addStyle(SpanStyle(background = fill), lo, hi)
        }
    }
    return b.toAnnotatedString()
}

// ---------------------------------------------------------------------------
// JobGraph
// ---------------------------------------------------------------------------

@Composable
internal fun JobGraphLeaf(cx: LeafContext) {
    val p = cx.props
    val compact = p.flag("compact")
    val jobs = p["jobs"]
    val layout = rememberPainted(cx, jobs, compact) { JobGraph.layout(jobs?.let(::plainJson), compact) } ?: return
    var hoverId by remember { mutableStateOf<String?>(null) }
    var focusId by remember { mutableStateOf<String?>(null) }
    var heldId by remember { mutableStateOf<String?>(null) }
    val value = p.str("value")
    fun select(id: String) = cx.model.fire(cx.index, "select", JsonValue.Obj(mapOf("id" to JsonValue.Str(id))))
    fun label(n: JobGraph.Node): String = listOfNotNull(n.name, cx.string(CiLog.entry(n.status).label), n.count?.let { "${it.passed}/${it.total}" }).joinToString(", ")
    Box(Modifier.size(cx.size.width.dp, cx.size.height.dp)) {
        if (compact) {
            Column(Modifier.fillMaxWidth()) {
                for (n in layout.nodes) key(n.id) {
                    val src = remember(n.id) { MutableInteractionSource() }
                    val hovered by src.collectIsHoveredAsState()
                    val props: Props = mapOf("status" to JsonValue.Str(n.status))
                    val states = buildList {
                        if (hovered) add("hover")
                        if (value == n.id) add("selected")
                    }
                    val look = cx.part("JobGraph", "row", props, states)
                    val divider = look.style.borderColor
                    val hairline = max(look.style.borderWidths?.getOrNull(2) ?: 0f, look.style.borderWidth).takeIf { it > 0f } ?: 1f
                    val name = label(n)
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .height(n.height.toFloat().dp)
                            .background(look.style.background ?: Color.Transparent)
                            .hoverable(src)
                            .clickable(interactionSource = src, indication = null, role = Role.Button) { select(n.id) }
                            .clearAndSetSemantics {
                                contentDescription = name
                                role = Role.Button
                                selected = value == n.id
                                onClick { select(n.id); true }
                            },
                    ) {
                        JobContent(cx, n, props, look, chevron = true, Modifier.fillMaxSize().padding(horizontal = look.style.paddingHorizontal.dp))
                        if (divider != null) Box(Modifier.align(Alignment.BottomStart).fillMaxWidth().height(hairline.dp).background(divider))
                    }
                }
            }
        } else {
            val active = hoverId ?: focusId ?: heldId
            val lit = remember(layout, active) { active?.let { JobGraph.upstream(layout, it).toSet() } ?: emptySet() }
            val stage = cx.part("JobGraph", "stage", emptyMap())
            val sts = partText(stage, cx.textStyle)
            val edgeColor = cx.part("JobGraph", "edge", emptyMap()).color ?: cx.ink
            val g = JobGraph.GEOMETRY
            Box(Modifier.fillMaxSize().horizontalScroll(rememberScrollState())) {
                Box(Modifier.size(layout.width.toFloat().dp, layout.height.toFloat().dp)) {
                    for (s in layout.stages) {
                        Box(Modifier.absoluteOffset(s.x.toFloat().dp, 0.dp).size(g.nodeWidth.toFloat().dp, g.stageHeight.toFloat().dp), contentAlignment = Alignment.CenterStart) {
                            BasicText(s.title, style = cx.composeTextStyle(ts = sts, color = stage.color ?: cx.ink), maxLines = 1, softWrap = false, overflow = TextOverflow.Ellipsis)
                        }
                    }
                    // Connectors only for the hovered / focused / held node's upstream set.
                    if (lit.isNotEmpty()) {
                        Canvas(Modifier.matchParentSizeCompat(layout)) {
                            val k = density
                            for (e in layout.edges) {
                                if (e.from !in lit || e.to !in lit) continue
                                val path = Path().apply {
                                    moveTo(e.start.x.toFloat() * k, e.start.y.toFloat() * k)
                                    cubicTo(e.c1.x.toFloat() * k, e.c1.y.toFloat() * k, e.c2.x.toFloat() * k, e.c2.y.toFloat() * k, e.end.x.toFloat() * k, e.end.y.toFloat() * k)
                                }
                                drawPath(path, edgeColor, style = Stroke(width = g.edgeWidth.toFloat() * k))
                            }
                        }
                    }
                    for (n in layout.nodes) key(n.id) {
                        val src = remember(n.id) { MutableInteractionSource() }
                        LaunchedEffect(src) {
                            src.interactions.collect { i ->
                                when (i) {
                                    is HoverInteraction.Enter -> hoverId = n.id
                                    is HoverInteraction.Exit -> if (hoverId == n.id) hoverId = null
                                    is FocusInteraction.Focus -> focusId = n.id
                                    is FocusInteraction.Unfocus -> if (focusId == n.id) focusId = null
                                    else -> {}
                                }
                            }
                        }
                        val props: Props = mapOf("status" to JsonValue.Str(n.status))
                        val states = buildList {
                            if (hoverId == n.id) add("hover")
                            if (focusId == n.id) add("focus")
                            if (n.id in lit) add("highlight")
                            if (value == n.id) add("selected")
                        }
                        val look = cx.part("JobGraph", "node", props, states)
                        val name = label(n)
                        JobNode(cx, n, props, look, name, value == n.id, src, press = { select(n.id) }, hold = { heldId = if (heldId == n.id) null else n.id })
                    }
                }
            }
        }
    }
}

/** The graph's own box (the layout's size, dp). */
private fun Modifier.matchParentSizeCompat(layout: JobGraph.Layout): Modifier = this.size(layout.width.toFloat().dp, layout.height.toFloat().dp)

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun JobNode(cx: LeafContext, n: JobGraph.Node, props: Props, look: PartStyle, name: String, isSelected: Boolean, src: MutableInteractionSource, press: () -> Unit, hold: () -> Unit) {
    Box(
        Modifier
            .absoluteOffset(n.x.toFloat().dp, n.y.toFloat().dp)
            .size(n.width.toFloat().dp, n.height.toFloat().dp)
            .ciBox(look.style)
            .hoverable(src)
            .combinedClickable(interactionSource = src, indication = null, role = Role.Button, onLongClick = hold, onClick = press)
            .clearAndSetSemantics {
                contentDescription = name
                role = Role.Button
                selected = isSelected
                onClick { press(); true }
            },
    ) {
        JobContent(cx, n, props, look, chevron = false, Modifier.fillMaxSize().padding(horizontal = look.style.paddingHorizontal.dp))
    }
}

/** A node's (or list row's) content: status glyph, name (truncated), meta (duration or `3/5`), the list's chevron. */
@Composable
private fun JobContent(cx: LeafContext, n: JobGraph.Node, props: Props, look: PartStyle, chevron: Boolean, modifier: Modifier) {
    val icon = cx.part("JobGraph", "icon", props)
    val nameLook = cx.part("JobGraph", "name", props)
    val meta = cx.part("JobGraph", "meta", props)
    val nts = partText(nameLook, cx.textStyle)
    val mts = partText(meta, cx.textStyle, tabular = true)
    val size = icon.width ?: cx.control("iconSm", 16f)
    Row(modifier, horizontalArrangement = Arrangement.spacedBy((if (look.style.gap > 0f) look.style.gap else cx.spacing("sm")).dp), verticalAlignment = Alignment.CenterVertically) {
        CiStatusGlyph(n.status, icon.color ?: cx.ink, size, cx.model)
        BasicText(n.name, style = cx.composeTextStyle(ts = nts, color = nameLook.color ?: cx.ink), maxLines = 1, softWrap = false, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = true))
        n.meta?.let { m -> BasicText(m, style = cx.composeTextStyle(ts = mts, color = meta.color ?: cx.ink), maxLines = 1, softWrap = false) }
        if (chevron) {
            val c = cx.part("JobGraph", "chevron", props)
            IconView("ui-chevron-right", c.width ?: size, c.color ?: cx.ink, cx.model)
        }
    }
}
