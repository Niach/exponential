package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.PaddingValues
import com.exponential.app.ui.theme.ExpMono
import com.exponential.app.ui.components.SectionBand
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import at.exponential.ui.measure.middleTruncateWidth
import com.exponential.app.data.api.PullFile
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

// EXP-895 — ONE file's diff, the whole of the per-file rendering ×4 (web
// `FileDiffCard`): a tappable `letter · path · +a −b · chevron` header over the
// unified body ([PatchLines]). It takes a [Diff.File] off the shared parser and
// nothing else — no PullFile, no patch strings — so every surface that shows a
// diff (the Review page, both Changes faces, a transcript's Edit row) draws the
// same card.

/**
 * GitHub's PullFile → the shared model through [Diff.fromPullFile], the ONE
 * mapping the contract fixture's `pullFile` cases lock ×4 (web `fromPullFile`,
 * iOS `PrFile.diffFile`). With hunks the parser's counts win, so the header can
 * never disagree with the rows under it; without, GitHub's own counts stand.
 */
fun PullFile.toDiffFile(): Diff.File = Diff.fromPullFile(
    filename = filename,
    previousFilename = previousFilename,
    status = status,
    additions = additions.toLong(),
    deletions = deletions.toLong(),
    patch = patch,
)

/** The one-letter status a file list leads with. */
fun diffStatusLetter(status: Diff.Status): String = when (status) {
    Diff.Status.ADDED -> "A"
    Diff.Status.REMOVED -> "D"
    Diff.Status.MODIFIED -> "M"
    Diff.Status.RENAMED -> "R"
    Diff.Status.COPIED -> "C"
}

/** The directory of a path WITH its trailing slash (`apps/web/src/`), empty at
 *  the repo root. */
fun diffPathDir(path: String): String {
    val slash = path.lastIndexOf('/')
    return if (slash >= 0) path.substring(0, slash + 1) else ""
}

/** The basename — the part a trailing ellipsis must never eat. */
fun diffPathBase(path: String): String {
    val slash = path.lastIndexOf('/')
    return path.substring(slash + 1)
}

/** A file with more hunk lines than this starts folded (web `COLLAPSE_THRESHOLD`). */
private const val COLLAPSE_THRESHOLD = DomainContract.diffUiCollapseThresholdLines

/** The hunk lines of a file — what the cap above is measured in. */
fun diffLineCount(file: Diff.File): Int {
    var n = 0
    for (hunk in file.hunks) n += hunk.lines.size
    return n
}

/**
 * Whether a file opens by default at this surface's setting (web
 * `diffOpensByDefault`). A review queue passes [defaultCollapsed] and every
 * card starts shut; a run's own output opens — except for the one file so long
 * that opening it buries everything after it.
 */
fun diffOpensByDefault(file: Diff.File, defaultCollapsed: Boolean): Boolean =
    !defaultCollapsed && diffLineCount(file) <= COLLAPSE_THRESHOLD

private val PathFontSize = 12.sp
private val CountFontSize = 11.sp
/** #EXP-1789: web `text-xs`'s 16px line, a flush row's type. */
private val FlushLineHeight = 16.sp

/**
 * `apps/web/src/` dimmed, `file.kt` at full weight — ONE monospace run.
 * EXP-698 + #EXP-1789, the web `DiffPath` / iOS `DiffPathLabel` rule ×4: a
 * trailing ellipsis eats the only part of a path that identifies the file, so
 * the DIRECTORY gives way first, middle-cut by the genUI Text
 * `truncate: "middle"` rule (`middleTruncateWidth`, never a platform ellipsis)
 * to what the name leaves, never below the width of `00`; a name that alone
 * fills the row keeps a lone `…` before it and only then cuts at its end. The
 * label fills the room up to the counts; the cut directory hugs the name.
 */
@Composable
internal fun DiffPathLabel(path: String, failed: Boolean, modifier: Modifier = Modifier) {
    val dir = remember(path) { diffPathDir(path) }
    val base = remember(path) { diffPathBase(path) }
    val dim = if (failed) {
        DesignTokens.Semantic.Red.copy(alpha = TextEmphasis.Secondary)
    } else {
        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    }
    val style = LocalTextStyle.current.merge(TextStyle(fontFamily = ExpMono, fontSize = PathFontSize))
    val measurer = rememberTextMeasurer()
    // Per-character widths (px), the rule sums them; one cache per style.
    val charWidths = remember(style, measurer) { HashMap<String, Double>() }
    BoxWithConstraints(modifier.semantics(mergeDescendants = true) { contentDescription = path }) {
        val width = constraints.maxWidth
        val shownDir = remember(dir, base, width, style, measurer) {
            if (dir.isEmpty() || width == Constraints.Infinity) {
                dir
            } else {
                val measure = { ch: String ->
                    charWidths.getOrPut(ch) {
                        measurer.measure(ch, style, softWrap = false, maxLines = 1).size.width.toDouble()
                    }
                }
                val baseWidth = measurer.measure(base, style, softWrap = false, maxLines = 1).size.width.toDouble()
                val twoCh = measurer.measure("00", style, softWrap = false, maxLines = 1).size.width.toDouble()
                val room = maxOf(width - baseWidth, twoCh)
                middleTruncateWidth(dir, room, measure = measure)
            }
        }
        Row {
            if (shownDir.isNotEmpty()) {
                Text(
                    shownDir,
                    style = style,
                    color = dim,
                    maxLines = 1,
                    softWrap = false,
                )
            }
            Text(
                base,
                style = style,
                color = if (failed) DesignTokens.Semantic.Red else MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                softWrap = false,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/**
 * The status letter. Only the two states that ARE a colour in the diff body
 * carry one: `A` the addition green, `D` the deletion red. `M`/`R`/`C` stay
 * muted — an amber "modified" and a sky "renamed" (the pre-EXP-895 palette)
 * invented two accent hues the token set does not have.
 */
@Composable
fun DiffStatusLetter(
    status: Diff.Status,
    modifier: Modifier = Modifier,
    /** EXP-916: the card's call FAILED — the letter goes with its header. */
    danger: Boolean = false,
    fontSize: TextUnit = CountFontSize,
) {
    Text(
        diffStatusLetter(status),
        color = when {
            danger -> DesignTokens.Semantic.Red
            status == Diff.Status.ADDED -> DesignTokens.Diff.AddFg
            status == Diff.Status.REMOVED -> DesignTokens.Diff.DelFg
            else -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
        },
        fontFamily = ExpMono,
        fontSize = fontSize,
        fontWeight = FontWeight.Bold,
        modifier = modifier,
    )
}

/** `+12 −2` — the deletion count is U+2212, never a hyphen ([Diff.deletionsLabel]). */
@Composable
fun DiffCounts(
    additions: Int,
    deletions: Int,
    modifier: Modifier = Modifier,
    fontSize: TextUnit = CountFontSize,
    gap: androidx.compose.ui.unit.Dp = 4.dp,
) {
    Row(modifier = modifier, horizontalArrangement = Arrangement.spacedBy(gap)) {
        Text(
            Diff.additionsLabel(additions),
            color = DesignTokens.Diff.AddFg,
            fontFamily = ExpMono,
            fontSize = fontSize,
        )
        Text(
            Diff.deletionsLabel(deletions),
            color = DesignTokens.Diff.DelFg,
            fontFamily = ExpMono,
            fontSize = fontSize,
        )
    }
}

/**
 * EXP-916: what the card KNOWS about its file. A `Ready` card is the ordinary
 * one; the three others exist only inside an edited-files card, where a call
 * may name a file before (or without) ever publishing a patch for it —
 * `Pending` while it runs, `Done` when it settled carrying none (a delete, a
 * move, an edit that changed nothing), `Failed` when the call itself failed.
 */
enum class DiffCardState { Ready, Pending, Done, Failed }

/**
 * One changed file — the one per-file unit ×4. [compact] is the transcript
 * rung: a tighter header and a body with no old-side gutter, for a card that
 * lives inside a transcript row rather than on a page of its own.
 *
 * EXP-916: [flush] drops the card's own border and radius so a stack of them
 * reads as the ROWS of a parent card (hairlines between, drawn by the parent);
 * the header always takes the section band's fill, never an opaque one, so the
 * page gradient keeps showing through. [state] is the file's own certainty: a
 * `Pending` card is the header alone (no counts, a muted dead chevron, no
 * body), a `Done` one the bare path (nothing will ever open behind it), a
 * `Failed` one wears the danger tint and the word `failed` instead of a
 * chevron.
 */
@Composable
fun DiffFileCard(
    file: Diff.File,
    expanded: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
    flush: Boolean = false,
    state: DiffCardState = DiffCardState.Ready,
) {
    val ready = state == DiffCardState.Ready
    val failed = state == DiffCardState.Failed
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Column(modifier = modifier.fillMaxWidth().then(if (flush) Modifier else Modifier.glassRow())) {
        val header: @Composable RowScope.() -> Unit = {
            if (flush) {
                // Web's flush row: a 12px-wide letter in the path's 12 type.
                DiffStatusLetter(file.status, danger = failed, fontSize = PathFontSize, modifier = Modifier.width(12.dp))
            } else {
                DiffStatusLetter(file.status, danger = failed)
            }
            Spacer(Modifier.width(8.dp))
            DiffPathLabel(file.path, failed = failed, modifier = Modifier.weight(1f))
            // Web's `gap-2` falls on both sides of its `ml-auto` spacer: 16
            // between a full label and the counts on a flush row.
            Spacer(Modifier.width(if (flush) 16.dp else 8.dp))
            when (state) {
                // A patch landed: the counts, and a chevron that turns.
                DiffCardState.Ready -> {
                    if (flush) {
                        DiffCounts(file.additions, file.deletions, fontSize = PathFontSize, gap = 6.dp)
                    } else {
                        DiffCounts(file.additions, file.deletions)
                    }
                    Spacer(Modifier.width(if (flush) 8.dp else 6.dp))
                    // EXP-706: ONE glyph that turns, instead of two that swap —
                    // the rotation reads as the card opening. Motion.standard()
                    // snaps under the OS's reduce-motion setting
                    // (ui/theme/Motion.kt).
                    val rotation by animateFloatAsState(
                        targetValue = if (expanded) 180f else 0f,
                        animationSpec = Motion.standard(),
                        label = "file-chevron",
                    )
                    Icon(
                        ExpIcons.uiChevronDown,
                        contentDescription = if (expanded) "Collapse" else "Expand",
                        modifier = Modifier.size(if (flush) 14.dp else 16.dp).rotate(rotation),
                        tint = muted,
                    )
                }
                // It ended carrying no patch: the path alone, in the ordinary
                // tint — nothing happened that the reader must read as wrong.
                DiffCardState.Done -> Unit
                // The call is still running: nothing to count and nothing to
                // open, so the chevron is there as a placeholder and dead.
                DiffCardState.Pending -> Icon(
                    ExpIcons.uiChevronDown,
                    contentDescription = null,
                    modifier = Modifier.size(if (flush) 14.dp else 16.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                )
                // It ended without one: say so, in the tint that means it.
                DiffCardState.Failed -> Text(
                    "failed",
                    style = MaterialTheme.typography.labelSmall,
                    color = DesignTokens.Semantic.Red,
                )
            }
        }
        if (flush) {
            // #EXP-1789: a flush row is web's / iOS's FLAT band — the section
            // fill edge to edge (no rounded tile), the parent's hairlines
            // between rows, `px-3 py-1.5` around 16-high 12 type (web
            // `text-xs`), so the edited-files card reads the same ×4. No
            // tracking: the theme's bodyLarge 0.5 sp would widen every mono
            // advance (and the path measurer reads this style too).
            CompositionLocalProvider(LocalTextStyle provides LocalTextStyle.current.copy(lineHeight = FlushLineHeight, letterSpacing = 0.sp)) {
                Row(
                    modifier = Modifier
                        .testTag("changes-file-row")
                        .fillMaxWidth()
                        .background(GlassTokens.SectionFill)
                        .then(if (ready) Modifier.clickable(onClick = onToggle) else Modifier)
                        .padding(horizontal = 12.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    content = header,
                )
            }
        } else {
            // The header is the section BAND everywhere a card stands alone.
            SectionBand(
                modifier = Modifier.testTag("changes-file-row"),
                onClick = if (ready) onToggle else null,
                contentPadding = PaddingValues(
                    horizontal = if (compact) 10.dp else 12.dp,
                    vertical = if (compact) 7.dp else 10.dp,
                ),
                content = header,
            )
        }
        if (expanded && ready) {
            if (file.hunks.isNotEmpty()) {
                PatchLines(
                    hunks = file.hunks,
                    compact = compact,
                    modifier = Modifier.padding(bottom = 8.dp),
                )
            } else {
                Text(
                    noHunksNote(file),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier.padding(horizontal = 12.dp).padding(bottom = 10.dp),
                )
            }
        }
    }
}
