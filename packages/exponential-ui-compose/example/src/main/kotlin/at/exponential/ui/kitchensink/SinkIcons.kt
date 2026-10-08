package at.exponential.ui.kitchensink

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ExitToApp
import androidx.compose.material.icons.automirrored.outlined.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.outlined.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.outlined.List
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.ArrowDropDown
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Create
import androidx.compose.material.icons.outlined.DateRange
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.Email
import androidx.compose.material.icons.outlined.Face
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.KeyboardArrowDown
import androidx.compose.material.icons.outlined.KeyboardArrowUp
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material.icons.outlined.Phone
import androidx.compose.material.icons.outlined.Place
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material.icons.outlined.Warning
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.dp

/**
 * A 24-unit outline glyph from SVG path data (Lucide geometry, 2-unit
 * stroke) for the concepts `material-icons-core` has no shape for.
 */
private fun outline(name: String, vararg paths: String): ImageVector {
    val b = ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f)
    for (d in paths) {
        b.addPath(
            PathParser().parsePathString(d).toNodes(),
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 2f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        )
    }
    return b.build()
}

private fun square(x: Int, y: Int) = "M${x + 1} ${y}h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-5a1 1 0 0 1-1-1v-5a1 1 0 0 1 1-1z"

private val grid = outline("grid", square(3, 3), square(14, 3), square(14, 14), square(3, 14))
private val folder = outline("folder", "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z")
private val archive = outline("archive", "M3 3h18a1 1 0 0 1 1 1v3a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z", "M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8", "M10 12h4")
private val copy = outline("copy", "M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2z", "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2")
private val minus = outline("minus", "M5 12h14")
private val upload = outline("upload", "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", "M17 8l-5-5-5 5", "M12 3v12")
private val file = outline("file", "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z", "M14 2v4a2 2 0 0 0 2 2h4")
private val clock = outline("clock", "M12 2a10 10 0 1 0 0 20a10 10 0 1 0 0-20z", "M12 6v6l4 2")
private val attach = outline("attach", "M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48")
private val stop = outline("stop", "M8 6h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z")
private val back = outline("back", "M19 12H5", "M12 19l-7-7 7-7")
private val menu = outline("menu", "M4 12h16", "M4 6h16", "M4 18h16")
private val star = outline("star", "M11.5 2.3a.53.53 0 0 1 .95 0l2.31 4.68a2.12 2.12 0 0 0 1.6 1.16l5.16.75a.53.53 0 0 1 .3.91l-3.74 3.64a2.12 2.12 0 0 0-.61 1.88l.88 5.14a.53.53 0 0 1-.77.56l-4.62-2.43a2.12 2.12 0 0 0-1.97 0L6.4 21.01a.53.53 0 0 1-.77-.56l.88-5.14a2.12 2.12 0 0 0-.61-1.88L2.16 9.8a.53.53 0 0 1 .3-.91l5.16-.75a2.12 2.12 0 0 0 1.6-1.16z")
private val filter = outline("filter", "M22 3H2l8 9.46V19l4 2v-8.54L22 3z")
private val undo = outline("undo", "M9 14 4 9l5-5", "M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11")
private val issues = outline("issues", "M12 2a10 10 0 1 0 0 20a10 10 0 1 0 0-20z", "M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8z")

/**
 * The catalog icon concepts the kitchen sink names, as Material outlined
 * glyphs (`material-icons-core`, the example's only icon dependency) plus
 * three Lucide outlines it lacks (grid, folder, archive). A
 * real host hands the renderer its own registry (the Exponential app's
 * Lucide set); the SDK ships none, so the example maps the concepts the
 * way the iOS example maps them to SF Symbols.
 */
val sinkIcons: Map<String, ImageVector> = mapOf(
    "nav-boards" to grid,
    "nav-inbox" to Icons.Outlined.Email,
    "nav-reviews" to Icons.Outlined.CheckCircle,
    "nav-search" to Icons.Outlined.Search,
    "ui-folder" to folder,
    "ui-archive" to archive,
    "ui-send" to Icons.AutoMirrored.Outlined.Send,
    "ui-issue" to Icons.Outlined.Refresh,
    "ui-pin" to Icons.Outlined.Place,
    "ui-success" to Icons.Outlined.CheckCircle,
    "ui-assignee" to Icons.Outlined.Person,
    "ui-device" to Icons.Outlined.Phone,
    "settings-statuses" to Icons.Outlined.Settings,
    "editor-bold" to Icons.Outlined.Create,
    "editor-italic" to Icons.Outlined.Edit,
    "ui-warning" to Icons.Outlined.Warning,
    "ui-info" to Icons.Outlined.Info,
    "ui-error" to Icons.Outlined.Warning,
    "ui-close" to Icons.Outlined.Close,
    "ui-plus" to Icons.Outlined.Add,
    "ui-more" to Icons.Outlined.MoreVert,
    "ui-settings" to Icons.Outlined.Settings,
    "ui-chevron-right" to Icons.AutoMirrored.Outlined.KeyboardArrowRight,
    "ui-chevron-left" to Icons.AutoMirrored.Outlined.KeyboardArrowLeft,
    "ui-chevron-down" to Icons.Outlined.KeyboardArrowDown,
    "ui-chevron-up" to Icons.Outlined.KeyboardArrowUp,
    "ui-selector" to Icons.Outlined.ArrowDropDown,
    "ui-check" to Icons.Outlined.Check,
    // The specimens' demo surface names the bare glyph (VAPP-93).
    "check" to Icons.Outlined.Check,
    "ui-search" to Icons.Outlined.Search,
    "ui-calendar" to Icons.Outlined.DateRange,
    "ui-image" to Icons.Outlined.Face,
    "ui-link" to Icons.Outlined.Share,
    "ui-external" to Icons.AutoMirrored.Outlined.ExitToApp,
    "ui-trash" to Icons.Outlined.Delete,
    "ui-edit" to Icons.Outlined.Edit,
    "ui-properties" to Icons.AutoMirrored.Outlined.List,
    // Round 1 built-in glyphs (core.catalog.json `builtinIcons`) and the specimens' concepts.
    "ui-copy" to copy,
    "ui-minus" to minus,
    "ui-add" to Icons.Outlined.Add,
    "upload" to upload,
    "ui-file" to file,
    "ui-clock" to clock,
    "calendar" to Icons.Outlined.DateRange,
    "search" to Icons.Outlined.Search,
    "ui-attach" to attach,
    "ui-stop" to stop,
    "ui-back" to back,
    "back" to back,
    "menu" to menu,
    "star" to star,
    "filter" to filter,
    "undo" to undo,
    "refresh" to Icons.Outlined.Refresh,
    "settings" to Icons.Outlined.Settings,
    "send" to Icons.AutoMirrored.Outlined.Send,
    "nav-issues" to issues,
    "nav-settings" to Icons.Outlined.Settings,
)

/**
 * The host's `icon(name, size)`: the concept's glyph at 80 % of the slot,
 * tinted with the `LocalContentColor` the painter provides; null (the
 * painter's placeholder circle) for a concept the map lacks.
 */
fun sinkIcon(name: String, size: Float): (@Composable () -> Unit)? {
    val vector = sinkIcons[name] ?: return null
    return {
        Box(Modifier.size(size.dp), contentAlignment = Alignment.Center) {
            Icon(vector, contentDescription = null, modifier = Modifier.size((size * 0.8f).dp))
        }
    }
}
