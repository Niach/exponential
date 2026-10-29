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
private val messageCircle = outline("message-circle", "M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719")
private val users = outline(
    "users",
    "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2",
    "M16 3.128a4 4 0 0 1 0 7.744",
    "M22 21v-2a4 4 0 0 0-3-3.87",
    "M9 3a4 4 0 1 0 0 8a4 4 0 1 0 0-8z",
)

// VAPP-162: the round-5 concepts (calls, pairing, push-to-talk) in Lucide geometry.
private fun rect(x: Float, y: Float, w: Float, h: Float, r: Float) =
    "M${x + r} ${y}h${w - 2 * r}a$r $r 0 0 1 $r ${r}v${h - 2 * r}a$r $r 0 0 1 -$r ${r}h-${w - 2 * r}a$r $r 0 0 1 -$r -${r}v-${h - 2 * r}a$r $r 0 0 1 $r -${r}z"
private fun circle(cx: Float, cy: Float, r: Float) = "M$cx ${cy - r}a$r $r 0 1 0 0 ${2 * r}a$r $r 0 1 0 0 -${2 * r}z"

private val mic = outline("mic", "M12 19v3", "M19 10v2a7 7 0 0 1-14 0v-2", rect(9f, 2f, 6f, 13f, 3f))
private val micOff = outline("mic-off", "M12 19v3", "M15 9.34V5a3 3 0 0 0-5.68-1.33", "M16.95 16.95A7 7 0 0 1 5 12v-2", "M18.89 13.23A7 7 0 0 0 19 12v-2", "m2 2 20 20", "M9 9v3a3 3 0 0 0 5.12 2.12")
private val phoneOff = outline(
    "phone-off",
    "M10.1 13.9a14 14 0 0 0 3.732 2.668 1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2 18 18 0 0 1-12.728-5.272",
    "M22 2 2 22",
    "M4.76 13.582A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 .244.473",
)
private val phoneIncoming = outline(
    "phone-incoming",
    "M16 2v6h6",
    "m22 2-6 6",
    "M13.832 16.568a1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 6.392 6.384",
)
private val volume2 = outline("volume-2", "M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z", "M16 9a5 5 0 0 1 0 6", "M19.364 18.364a9 9 0 0 0 0-12.728")
private val ear = outline("ear", "M6 8.5a6.5 6.5 0 1 1 13 0c0 6-6 6-6 10a3.5 3.5 0 1 1-7 0", "M15 8.5a2.5 2.5 0 0 0-5 0v1a2 2 0 1 1 0 4")
private val qrCode = outline(
    "qr-code",
    rect(3f, 3f, 5f, 5f, 1f), rect(16f, 3f, 5f, 5f, 1f), rect(3f, 16f, 5f, 5f, 1f),
    "M21 16h-3a2 2 0 0 0-2 2v3", "M21 21v.01", "M12 7v3a2 2 0 0 1-2 2H7", "M3 12h.01", "M12 3h.01", "M12 16v.01", "M16 12h1", "M21 12v.01", "M12 21v-1",
)
private val scanLine = outline("scan-line", "M3 7V5a2 2 0 0 1 2-2h2", "M17 3h2a2 2 0 0 1 2 2v2", "M21 17v2a2 2 0 0 1-2 2h-2", "M7 21H5a2 2 0 0 1-2-2v-2", "M7 12h10")
private val audioLines = outline("audio-lines", "M2 10v3", "M6 6v11", "M10 3v18", "M14 8v7", "M18 5v13", "M22 10v3")
private val hand = outline("hand", "M18 11V6a2 2 0 0 0-2-2a2 2 0 0 0-2 2", "M14 10V4a2 2 0 0 0-2-2a2 2 0 0 0-2 2v2", "M10 10.5V6a2 2 0 0 0-2-2a2 2 0 0 0-2 2v8", "M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15")
private val checkCheck = outline("check-check", "M18 6 7 17l-5-5", "m22 10-7.5 7.5L13 16")
private val globe = outline("globe", circle(12f, 12f, 10f), "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20", "M2 12h20")
private val mail = outline("mail", "m22 7-8.991 5.727a2 2 0 0 1-2.009 0L2 7", rect(2f, 4f, 20f, 16f, 2f))
private val pencil = outline("pencil", "M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z", "m15 5 4 4")
private val layers = outline("layers", "M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z", "M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12", "M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17")
private val fileText = outline("file-text", "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z", "M14 2v4a2 2 0 0 0 2 2h4", "M10 9H8", "M16 13H8", "M16 17H8")
private val bot = outline("bot", "M12 8V4H8", rect(4f, 8f, 16f, 12f, 2f), "M2 14h2", "M20 14h2", "M15 13v2", "M9 13v2")
private val repeat = outline("repeat", "m17 2 4 4-4 4", "M3 11v-1a4 4 0 0 1 4-4h14", "m7 22-4-4 4-4", "M21 13v1a4 4 0 0 1-4 4H3")

/** A filled 24-unit mark (icons.json `imported`/`custom`, `custom-icons.generated.ts`). */
private fun mark(name: String, d: String): ImageVector =
    ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f)
        .addPath(PathParser().parsePathString(d).toNodes(), fill = SolidColor(Color.Black))
        .build()

private val osApple = mark("os-apple", "M18.859 9.138c-0.105 0.081 -1.948 1.118 -1.948 3.428c0 2.672 2.345 3.618 2.415 3.639c-0.011 0.056 -0.373 1.294 -1.238 2.556c-0.77 1.107 -1.575 2.215 -2.798 2.215s-1.54 -0.71 -2.953 -0.71c-1.378 0 -1.867 0.735 -2.985 0.735s-1.902 -1.027 -2.798 -2.285c-1.041 -1.48 -1.881 -3.779 -1.881 -5.959c0 -3.498 2.275 -5.354 4.514 -5.354c1.188 0 2.183 0.78 2.929 0.78c0.71 0 1.821 -0.83 3.175 -0.83c0.513 0 2.355 0.046 3.568 1.786m-4.212 -3.27c0.559 -0.664 0.956 -1.586 0.956 -2.507c0 -0.127 -0.011 -0.257 -0.035 -0.362c-0.911 0.035 -1.993 0.608 -2.647 1.364c-0.513 0.584 -0.991 1.505 -0.991 2.44c0 0.141 0.025 0.281 0.035 0.327c0.056 0.011 0.151 0.025 0.246 0.025c0.816 -0.004 1.842 -0.548 2.436 -1.287")
private val osWindows = mark("os-windows", "M3 3h8.532v8.529H3Zm9.468 0H21v8.529H12.468ZM3 12.468h8.532V21H3Zm9.468 0H21V21H12.468")
private val osLinux = mark("os-linux", "M19.471 17.752c-0.12 -0.109 -0.299 -0.19 -0.418 -0.25c-0.179 -0.07 -0.299 -0.162 -0.418 -0.267c-0.179 -0.109 -0.239 -0.232 -0.299 -0.362l-0.06 -0.439c-0.06 -0.281 -0.12 -0.461 -0.179 -0.541s-0.12 -0.13 -0.239 -0.13c0.12 -0.387 0.179 -0.787 0.179 -1.199c0 -0.83 -0.179 -1.603 -0.478 -2.201c-0.239 -0.654 -0.654 -1.311 -1.192 -1.905c-0.239 -0.359 -0.478 -0.654 -0.654 -0.953c-0.179 -0.239 -0.299 -0.478 -0.418 -0.714c-0.12 -0.179 -0.179 -0.418 -0.239 -0.654c-0.06 -0.299 -0.06 -0.538 -0.12 -0.714c0 -0.179 -0.06 -0.418 -0.06 -0.773c-0.06 -0.418 -0.06 -0.714 -0.12 -1.012c-0.06 -0.478 -0.179 -0.833 -0.299 -1.192c-0.179 -0.299 -0.359 -0.598 -0.598 -0.773c-0.179 -0.179 -0.418 -0.299 -0.654 -0.418c-0.299 -0.12 -0.538 -0.179 -0.714 -0.179C12.316 3 12.137 3 11.961 3c-0.478 0 -0.893 0.06 -1.252 0.179s-0.598 0.359 -0.773 0.598c-0.204 0.239 -0.341 0.538 -0.418 0.833c-0.077 0.299 -0.113 0.598 -0.113 1.012c0 0.538 0.028 1.132 0.088 1.73c0.067 0.598 0.095 0.953 0.095 1.072c0 0.239 -0.06 0.538 -0.172 0.773s-0.316 0.538 -0.626 0.893L8.344 10.629c-0.359 0.418 -0.584 0.833 -0.679 1.252c-0.12 0.478 -0.225 0.833 -0.327 1.072c-0.095 0.239 -0.225 0.478 -0.401 0.798l-0.207 0.376c-0.214 0.401 -0.316 0.703 -0.316 0.918c-0.011 0.035 -0.011 0.095 -0.011 0.162c0 0.095 0.025 0.172 0.07 0.232c-0.042 0.049 -0.07 0.095 -0.095 0.137c-0.011 0.028 -0.025 0.067 -0.035 0.102c-0.011 0.025 -0.025 0.077 -0.053 0.148c-0.025 0.07 -0.06 0.148 -0.102 0.246c-0.127 0.07 -0.207 0.102 -0.264 0.102H5.236c-0.13 0 -0.257 0.077 -0.387 0.225s-0.19 0.359 -0.19 0.619c0 0.084 0.025 0.281 0.07 0.591c0.049 0.316 0.07 0.496 0.07 0.555s-0.011 0.12 -0.025 0.179c-0.011 0.067 -0.035 0.12 -0.06 0.172c-0.025 0.06 -0.049 0.113 -0.084 0.165c-0.025 0.06 -0.06 0.12 -0.102 0.186c-0.042 0.06 -0.084 0.127 -0.123 0.19c-0.042 0.109 -0.06 0.211 -0.06 0.306s0.042 0.179 0.123 0.253c0.088 0.074 0.207 0.137 0.359 0.183c0.155 0.056 0.299 0.095 0.422 0.123c0.137 0.028 0.292 0.056 0.461 0.081c0.172 0.028 0.281 0.049 0.327 0.06c0.482 0.109 1.037 0.274 1.663 0.489c0.619 0.225 1.079 0.341 1.371 0.341c0.464 0 0.798 -0.186 1.097 -0.541v0.018c0.418 -0.207 1.012 -0.313 1.789 -0.313c0.478 0 1.252 0.056 2.204 0.172c0.179 0.362 0.478 0.601 0.953 0.728l0.418 0.049c0.239 0 0.478 -0.053 0.654 -0.162c0.179 -0.102 0.418 -0.267 0.654 -0.499l0.598 -0.506c0.299 -0.253 0.598 -0.468 0.893 -0.643c0.359 -0.169 0.598 -0.288 0.714 -0.348c0.179 -0.053 0.299 -0.137 0.418 -0.257c0.12 -0.109 0.179 -0.257 0.179 -0.461c0.004 -0.134 -0.056 -0.257 -0.172 -0.376M15.364 11.438c0.299 0.06 0.538 0.239 0.773 0.598c0.239 0.359 0.359 0.773 0.359 1.371c0 0.429 -0.06 0.918 -0.239 1.473c0.06 0.011 0.179 0.025 0.239 0.028c0.06 0 0.06 0 0.12 0.011c0 0 0.06 0 0.12 0.007c0.359 0.049 0.654 0.162 0.833 0.341c0.179 0.172 0.239 0.327 0.239 0.461c0 0.053 0 0.077 -0.06 0.077c0 0 -0.06 -0.035 -0.06 -0.113c-0.06 -0.077 -0.12 -0.162 -0.179 -0.25c-0.06 -0.095 -0.179 -0.179 -0.418 -0.257c-0.179 -0.084 -0.478 -0.127 -0.773 -0.127c-0.179 0 -0.299 0.053 -0.478 0.155c-0.12 0.102 -0.179 0.204 -0.299 0.302c-0.06 0.102 -0.12 0.155 -0.12 0.155c-0.06 0 -0.06 -0.028 -0.06 -0.084c0.179 -0.345 0.418 -0.566 0.654 -0.661c0.239 -0.633 0.299 -1.125 0.299 -1.459c0 -0.418 -0.06 -0.773 -0.239 -1.192c-0.179 -0.359 -0.478 -0.598 -0.773 -0.714c0.004 -0.063 0.004 -0.063 0.063 -0.123M12.084 5.848q0.269 -0.179 0.538 -0.179c0.239 0 0.418 0.06 0.598 0.239c0.12 0.179 0.179 0.418 0.179 0.773c0 0.299 -0.06 0.538 -0.12 0.893c-0.12 -0.06 -0.179 -0.12 -0.299 -0.12c-0.06 0 -0.12 -0.06 -0.12 -0.06q0.359 -0.09 0.359 -0.538c0 -0.179 -0.06 -0.299 -0.12 -0.478c-0.12 -0.12 -0.239 -0.239 -0.418 -0.239q-0.179 0 -0.359 0.179c-0.12 0.179 -0.12 0.299 -0.12 0.538c0 0.12 0 0.239 0.06 0.299c-0.06 0 -0.12 -0.06 -0.179 -0.06c-0.12 -0.239 -0.179 -0.478 -0.179 -0.773c0 -0.176 0.06 -0.295 0.179 -0.475m0.657 0.657c0 -0.06 -0.06 -0.06 -0.06 -0.12c0.06 0 0.12 -0.06 0.12 -0.06c0.06 0 0.12 0.06 0.179 0.12s0.06 0.12 0.06 0.179s0 0.12 0.06 0.179h-0.12c-0.06 -0.179 -0.12 -0.299 -0.239 -0.299m-0.179 0.893c0.598 0.179 0.893 0.359 0.953 0.478c0.06 0 0.06 0.06 0.06 0.12c0 0.179 -0.12 0.299 -0.239 0.418l-0.538 0.359c-0.598 0.418 -1.072 0.654 -1.491 0.654c-0.299 0 -0.598 -0.12 -0.833 -0.359l-0.359 -0.359c-0.06 -0.06 -0.12 -0.179 -0.179 -0.299c0.359 0.299 0.773 0.478 1.192 0.478c0.478 0 1.072 -0.239 1.849 -0.773c0 0 0 -0.06 -0.06 -0.06v-0.12c-0.773 0.598 -1.371 0.833 -1.849 0.833c-0.359 0 -0.714 -0.179 -1.072 -0.478c-0.06 -0.12 -0.12 -0.179 -0.12 -0.179c0 -0.239 0.239 -0.478 0.598 -0.714c0.418 -0.239 0.714 -0.359 0.893 -0.359c0.243 0 0.657 0.12 1.195 0.359m-2.264 -0.893v-0.12c0.06 0 0.06 -0.06 0.06 -0.06c0.179 0 0.299 0.179 0.299 0.478h-0.12c0 -0.12 -0.06 -0.179 -0.06 -0.299Zm-0.359 -0.418c0.06 -0.179 0.239 -0.299 0.418 -0.299c0.239 0 0.359 0.12 0.478 0.299s0.12 0.359 0.12 0.538c0 0.239 0 0.359 -0.06 0.359v0.06h-0.06v-0.12c0 -0.179 -0.06 -0.359 -0.12 -0.538c-0.12 -0.12 -0.179 -0.179 -0.299 -0.179s-0.239 0.06 -0.299 0.179c-0.12 0.12 -0.12 0.239 -0.12 0.418c0 0.299 0.06 0.478 0.299 0.598c-0.06 0.06 -0.12 0.06 -0.179 0.12c-0.239 -0.239 -0.323 -0.478 -0.323 -0.714c0 -0.302 0.025 -0.482 0.144 -0.721m0.053 14.126c-0.285 0.359 -0.633 0.541 -1.048 0.541c-0.162 0 -0.601 -0.137 -1.322 -0.401s-1.361 -0.439 -1.912 -0.52v-0.004c-0.721 -0.095 -1.114 -0.257 -1.174 -0.489c0 -0.137 0.077 -0.327 0.225 -0.584c0.148 -0.246 0.221 -0.489 0.221 -0.735c0 -0.127 -0.025 -0.302 -0.07 -0.524c-0.049 -0.225 -0.07 -0.387 -0.07 -0.482c0 -0.207 0.077 -0.404 0.225 -0.577c0.113 -0.06 0.327 -0.095 0.643 -0.095h0.162c0.077 -0.007 0.13 -0.007 0.162 -0.007c0.137 -0.028 0.239 -0.06 0.299 -0.095c0.06 -0.042 0.095 -0.127 0.113 -0.25c0.011 -0.13 0.06 -0.257 0.144 -0.369s0.257 -0.197 0.506 -0.246c0.197 0.049 0.376 0.137 0.531 0.274c0.162 0.137 0.285 0.274 0.362 0.411c0.088 0.127 0.204 0.302 0.327 0.538c0.13 0.232 0.232 0.404 0.316 0.517l0.404 0.541c0.679 0.865 1.02 1.575 1.02 2.141c-0.004 0.158 -0.004 0.295 -0.063 0.411m1.311 -1.167c-0.714 0 -1.252 -0.165 -1.501 -0.506c-0.127 -0.232 -0.186 -0.352 -0.186 -0.359c0 -0.028 0.007 -0.042 0.018 -0.042c0.137 0 0.299 -0.07 0.418 -0.214c0.179 -0.137 0.239 -0.264 0.239 -0.387c0 -0.239 -0.359 -0.643 -1.03 -1.227l-0.633 -0.411c-0.499 -0.362 -0.816 -0.65 -0.949 -0.847c-0.12 -0.204 -0.186 -0.436 -0.186 -0.703c0 -0.362 0.13 -0.858 0.38 -1.396c0.25 -0.598 0.461 -1.012 0.633 -1.192l0.06 0.06c0.049 0 0.077 0 0.088 0.06c-0.207 0.239 -0.422 0.598 -0.636 1.132c-0.221 0.538 -0.327 0.988 -0.327 1.399c0 0.334 0.13 0.591 0.401 0.773c-0.025 -0.144 -0.035 -0.285 -0.035 -0.422c0 -0.686 0.246 -1.515 0.728 -2.408l0.446 -0.893c0.011 0 0.028 -0.12 0.053 -0.239c0.018 -0.179 0.109 -0.418 0.274 -0.833l0.376 -0.714c0.12 -0.239 0.179 -0.478 0.179 -0.654l0.359 0.299c0.179 0.239 0.478 0.299 0.833 0.299c0.418 0 0.893 -0.179 1.491 -0.598l0.538 -0.418c0.12 0.239 0.239 0.538 0.359 0.833c0.179 0.299 0.299 0.598 0.478 0.893c0 0.299 0.06 0.478 0.12 0.714c0.12 0.179 0.179 0.359 0.299 0.538l0.359 0.538c0.12 0.239 0.239 0.538 0.299 0.893c0.12 0.418 0.12 0.865 0.12 1.406c0 0.359 0 0.756 -0.06 1.216c-0.12 -0.088 -0.239 -0.13 -0.359 -0.13c-0.179 0 -0.299 0.088 -0.418 0.281c-0.12 0.19 -0.179 0.482 -0.179 0.868v0.221c0.06 0.077 0.06 0.264 0.06 0.559c-0.78 1.076 -1.793 1.61 -3.104 1.61m7.991 -0.503c-0.12 0.095 -0.239 0.165 -0.418 0.214c-0.12 0.053 -0.359 0.148 -0.598 0.292c-0.299 0.144 -0.538 0.309 -0.773 0.499l-1.012 0.875c-0.06 0.028 -0.06 0.06 -0.179 0.102c-0.06 0.049 -0.12 0.088 -0.179 0.13c-0.06 0.049 -0.179 0.088 -0.299 0.113q-0.179 0.053 -0.359 0.053c-0.478 0 -0.893 -0.225 -1.132 -0.675v-0.014l0.12 -0.661c0 -0.236 0 -0.446 0.06 -0.633c0 -0.186 0.06 -0.302 0.06 -0.359v-1.853c0 -0.626 0.12 -0.935 0.418 -0.935c0.12 0 0.179 0.053 0.299 0.155c0.06 0.25 0.12 0.471 0.239 0.661c0.179 0.257 0.418 0.387 0.893 0.387c0.12 0 0.299 -0.053 0.478 -0.155c0.12 -0.102 0.299 -0.207 0.359 -0.327c0.12 -0.12 0.239 -0.232 0.359 -0.334c0.06 -0.102 0.179 -0.155 0.239 -0.155s0.06 0.007 0.06 0.025c0.06 0.067 0.06 0.186 0.12 0.345l0.06 0.345c0.06 0.281 0.179 0.496 0.299 0.65c0.179 0.162 0.299 0.257 0.418 0.299s0.239 0.109 0.359 0.204s0.239 0.225 0.239 0.401c-0.007 0.148 -0.067 0.264 -0.127 0.352m-7.805 -11.204s-0.06 -0.06 -0.06 -0.12s0 -0.06 0.06 -0.06l0.12 0.06c0.06 0.06 0.12 0.12 0.12 0.179s-0.06 0.06 -0.06 0.06h-0.06c-0.06 0 -0.06 -0.06 -0.12 -0.12m-0.598 0.06c0 -0.06 0 -0.12 0.06 -0.179c0.12 0 0.179 -0.06 0.179 -0.06c0.06 0 0.06 0.06 0.06 0.06c0 0.06 -0.06 0.12 -0.179 0.239h-0.06c-0.06 0 -0.06 0 -0.06 -0.06")

/** Lucide's `progress-2-4`: the stroked ring (r 10) around a filled half wedge (r 6). */
private val progress24: ImageVector =
    ImageVector.Builder("progress-2-4", 24.dp, 24.dp, 24f, 24f)
        .addPath(
            PathParser().parsePathString("M12 2a10 10 0 1 0 0 20a10 10 0 1 0 0-20z").toNodes(),
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 2f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        )
        .addPath(PathParser().parsePathString("M12 12 L12 6 A6 6 0 0 1 12 18 Z").toNodes(), fill = SolidColor(Color.Black))
        .build()

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
    "pr-open" to Icons.Outlined.CheckCircle,
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
    // EXP-1324 Trace concepts (the kitchen sink's and the specimen's).
    "trace-chevron" to Icons.AutoMirrored.Outlined.KeyboardArrowRight,
    "trace-terminal" to Icons.AutoMirrored.Outlined.List,
    "trace-edit" to Icons.Outlined.Edit,
    "trace-reasoning" to Icons.Outlined.Info,
    "trace-think" to Icons.Outlined.Info,
    "trace-search" to Icons.Outlined.Search,
    "trace-run" to Icons.Outlined.Refresh,
    "trace-next" to Icons.AutoMirrored.Outlined.KeyboardArrowRight,
    "trace-out" to Icons.AutoMirrored.Outlined.ExitToApp,
    "trace-failed" to Icons.Outlined.Close,
    "trace-task-done" to Icons.Outlined.CheckCircle,
    // N-12: the DockCard specimen's plan glyph.
    "coding-plan" to Icons.AutoMirrored.Outlined.List,
    // VAPP-163: the bar's icon segments and the Icon specimen's non-Lucide marks.
    "message-circle" to messageCircle,
    "users" to users,
    "os-apple" to osApple,
    "os-windows" to osWindows,
    "os-linux" to osLinux,
    "progress-2-4" to progress24,
    // VAPP-162: the call, pairing and push-to-talk concepts (icons.json) and their bare Lucide names.
    "call-mute" to mic,
    "call-unmute" to micOff,
    "call-end" to phoneOff,
    "call-speaker" to volume2,
    "call-speaker-off" to ear,
    "call-incoming" to phoneIncoming,
    "ui-qr-code" to qrCode,
    "ui-scan" to scanLine,
    "ui-read" to checkCheck,
    "ui-hold" to hand,
    "voice-level" to audioLines,
    "mic" to mic,
    "mic-off" to micOff,
    "phone-off" to phoneOff,
    "phone-incoming" to phoneIncoming,
    "qr-code" to qrCode,
    "scan-line" to scanLine,
    "audio-lines" to audioLines,
    "hand" to hand,
    "check-check" to checkCheck,
    // The Plan / Trace specimens' step glyphs.
    "globe" to globe,
    "mail" to mail,
    "pencil" to pencil,
    "layers" to layers,
    "trace-read" to fileText,
    "trace-subagent" to bot,
    "trace-background" to repeat,
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
