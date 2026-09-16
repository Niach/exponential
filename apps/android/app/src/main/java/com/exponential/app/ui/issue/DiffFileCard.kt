package com.exponential.app.ui.issue

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.api.PullFile
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.glassSectionBand

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

/**
 * The file list's filter (web `FileDiffNav`): a case-insensitive substring of
 * the WHOLE path, so `values/str` finds `res/values/strings.xml`. A blank
 * needle keeps every file.
 */
fun filterDiffFiles(files: List<Diff.File>, query: String): List<Diff.File> {
    val needle = query.trim().lowercase()
    if (needle.isEmpty()) return files
    return files.filter { it.path.lowercase().contains(needle) }
}

/**
 * Shorten [value] to at most [max] characters by replacing its MIDDLE with an
 * ellipsis — mirrors web `middleTruncate`. On a phone a trailing ellipsis eats
 * the only part of a path that identifies the file, so the DIRECTORY gives way
 * instead and both of its ends stay readable.
 */
fun middleTruncatePath(value: String, max: Int): String {
    if (max <= 0 || max < 3) return value
    if (value.length <= max) return value
    val keep = max - 1
    val head = keep / 2
    val tail = keep - head
    return value.take(head) + "…" + value.takeLast(tail)
}

/** How much directory a phone card keeps (web `MOBILE_DIR_CHARS`). */
private const val DIR_CHARS = 22

private val PathFontSize = 12.sp
private val CountFontSize = 11.sp

/**
 * `apps/web/src/` dimmed, `file.kt` at full weight — ONE monospace run, so the
 * path never breaks into two competing labels.
 */
@Composable
private fun diffPathText(path: String, dirChars: Int = DIR_CHARS): AnnotatedString {
    val dim = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    return remember(path, dim, dirChars) {
        val dir = diffPathDir(path)
        buildAnnotatedString {
            if (dir.isNotEmpty()) {
                withStyle(SpanStyle(color = dim)) { append(middleTruncatePath(dir, dirChars)) }
            }
            append(diffPathBase(path))
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
) {
    Text(
        diffStatusLetter(status),
        color = when {
            danger -> DesignTokens.Semantic.Red
            status == Diff.Status.ADDED -> DesignTokens.Diff.AddFg
            status == Diff.Status.REMOVED -> DesignTokens.Diff.DelFg
            else -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
        },
        fontFamily = FontFamily.Monospace,
        fontSize = CountFontSize,
        fontWeight = FontWeight.Bold,
        modifier = modifier,
    )
}

/** `+12 −2` — the deletion count is U+2212, never a hyphen ([Diff.deletionsLabel]). */
@Composable
fun DiffCounts(additions: Int, deletions: Int, modifier: Modifier = Modifier) {
    Row(modifier = modifier, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            Diff.additionsLabel(additions),
            color = DesignTokens.Diff.AddFg,
            fontFamily = FontFamily.Monospace,
            fontSize = CountFontSize,
        )
        Text(
            Diff.deletionsLabel(deletions),
            color = DesignTokens.Diff.DelFg,
            fontFamily = FontFamily.Monospace,
            fontSize = CountFontSize,
        )
    }
}

/**
 * EXP-916: what the card KNOWS about its file. A `Ready` card is the ordinary
 * one; the two others exist only inside an edited-files card, where a call may
 * name a file before (or without) ever publishing a patch for it.
 */
enum class DiffCardState { Ready, Pending, Failed }

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
 * body), a `Failed` one wears the danger tint and the word `failed` instead of
 * a chevron.
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
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .testTag("changes-file-row")
                // The header is the section BAND everywhere — a flush card
                // drops the outer edge, never the band under its header.
                .glassSectionBand()
                .then(if (ready) Modifier.clickable(onClick = onToggle) else Modifier)
                .padding(
                    horizontal = if (compact) 10.dp else 12.dp,
                    vertical = if (compact) 7.dp else 10.dp,
                ),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            DiffStatusLetter(file.status, danger = failed)
            Spacer(Modifier.width(8.dp))
            Text(
                diffPathText(file.path),
                fontFamily = FontFamily.Monospace,
                fontSize = PathFontSize,
                color = if (failed) DesignTokens.Semantic.Red else MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            when (state) {
                // A patch landed: the counts, and a chevron that turns.
                DiffCardState.Ready -> {
                    DiffCounts(file.additions, file.deletions)
                    Spacer(Modifier.width(6.dp))
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
                        modifier = Modifier.size(16.dp).rotate(rotation),
                        tint = muted,
                    )
                }
                // The call is still running: nothing to count and nothing to
                // open, so the chevron is there as a placeholder and dead.
                DiffCardState.Pending -> Icon(
                    ExpIcons.uiChevronDown,
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
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
