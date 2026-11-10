package at.exponential.ui.kitchensink

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
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
 * stroke, round caps and joins: lucide-react's own SVG attributes).
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

// The Lucide glyphs, path data copied from lucide-react 0.544.0's
// `__iconNode` (the version icons.json names; `<circle>`/`<rect>`/`<line>`/
// `<polyline>` flattened to paths the way packages/icons/scripts/generate.ts
// does), so every concept paints the exact glyph web draws.
private val lArchive = outline("archive", "M3 3H21A1 1 0 0 1 22 4V7A1 1 0 0 1 21 8H3A1 1 0 0 1 2 7V4A1 1 0 0 1 3 3Z", "M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8", "M10 12h4")
private val lArrowLeft = outline("arrow-left", "m12 19-7-7 7-7", "M19 12H5")
private val lArrowRight = outline("arrow-right", "M5 12h14", "m12 5 7 7-7 7")
private val lAtSign = outline("at-sign", "M8 12A4 4 0 1 0 16 12A4 4 0 1 0 8 12Z", "M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8")
private val lAudioLines = outline("audio-lines", "M2 10v3", "M6 6v11", "M10 3v18", "M14 8v7", "M18 5v13", "M22 10v3")
private val lBell = outline("bell", "M10.268 21a2 2 0 0 0 3.464 0", "M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326")
private val lBold = outline("bold", "M6 12h9a4 4 0 0 1 0 8H7a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h7a4 4 0 0 1 0 8")
private val lBot = outline("bot", "M12 8V4H8", "M6 8H18A2 2 0 0 1 20 10V18A2 2 0 0 1 18 20H6A2 2 0 0 1 4 18V10A2 2 0 0 1 6 8Z", "M2 14h2", "M20 14h2", "M15 13v2", "M9 13v2")
private val lBrain = outline("brain", "M12 18V5", "M15 13a4.17 4.17 0 0 1-3-4 4.17 4.17 0 0 1-3 4", "M17.598 6.5A3 3 0 1 0 12 5a3 3 0 1 0-5.598 1.5", "M17.997 5.125a4 4 0 0 1 2.526 5.77", "M18 18a4 4 0 0 0 2-7.464", "M19.967 17.483A4 4 0 1 1 12 18a4 4 0 1 1-7.967-.517", "M6 18a4 4 0 0 1-2-7.464", "M6.003 5.125a4 4 0 0 0-2.526 5.77")
private val lBriefcase = outline("briefcase", "M16 20V4a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16", "M4 6H20A2 2 0 0 1 22 8V18A2 2 0 0 1 20 20H4A2 2 0 0 1 2 18V8A2 2 0 0 1 4 6Z")
private val lBug = outline("bug", "M12 20v-9", "M14 7a4 4 0 0 1 4 4v3a6 6 0 0 1-12 0v-3a4 4 0 0 1 4-4z", "M14.12 3.88 16 2", "M21 21a4 4 0 0 0-3.81-4", "M21 5a4 4 0 0 1-3.55 3.97", "M22 13h-4", "M3 21a4 4 0 0 1 3.81-4", "M3 5a4 4 0 0 0 3.55 3.97", "M6 13H2", "m8 2 1.88 1.88", "M9 7.13V6a3 3 0 1 1 6 0v1.13")
private val lBuilding2 = outline("building-2", "M6 22V4a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v18Z", "M6 12H4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h2", "M18 9h2a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-2", "M10 6h4", "M10 10h4", "M10 14h4", "M10 18h4")
private val lCalendar = outline("calendar", "M8 2v4", "M16 2v4", "M5 4H19A2 2 0 0 1 21 6V20A2 2 0 0 1 19 22H5A2 2 0 0 1 3 20V6A2 2 0 0 1 5 4Z", "M3 10h18")
private val lCheck = outline("check", "M20 6 9 17l-5-5")
private val lCheckCheck = outline("check-check", "M18 6 7 17l-5-5", "m22 10-7.5 7.5L13 16")
private val lChevronDown = outline("chevron-down", "m6 9 6 6 6-6")
private val lChevronLeft = outline("chevron-left", "m15 18-6-6 6-6")
private val lChevronRight = outline("chevron-right", "m9 18 6-6-6-6")
private val lChevronUp = outline("chevron-up", "m18 15-6-6-6 6")
private val lChevronsUpDown = outline("chevrons-up-down", "m7 15 5 5 5-5", "m7 9 5-5 5 5")
private val lChromium = outline("chromium", "M10.88 21.94 15.46 14", "M21.17 8H12", "M3.95 6.06 8.54 14", "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z", "M8 12A4 4 0 1 0 16 12A4 4 0 1 0 8 12Z")
private val lCircle = outline("circle", "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z")
private val lCircleCheck = outline("circle-check", "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z", "m9 12 2 2 4-4")
private val lCircleCheckBig = outline("circle-check-big", "M21.801 10A10 10 0 1 1 17 3.335", "m9 11 3 3L22 4")
private val lCircleDashed = outline("circle-dashed", "M10.1 2.182a10 10 0 0 1 3.8 0", "M13.9 21.818a10 10 0 0 1-3.8 0", "M17.609 3.721a10 10 0 0 1 2.69 2.7", "M2.182 13.9a10 10 0 0 1 0-3.8", "M20.279 17.609a10 10 0 0 1-2.7 2.69", "M21.818 10.1a10 10 0 0 1 0 3.8", "M3.721 6.391a10 10 0 0 1 2.7-2.69", "M6.391 20.279a10 10 0 0 1-2.69-2.7")
private val lCircleDot = outline("circle-dot", "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z", "M11 12A1 1 0 1 0 13 12A1 1 0 1 0 11 12Z")
private val lCircleStop = outline("circle-stop", "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z", "M10 9H14A1 1 0 0 1 15 10V14A1 1 0 0 1 14 15H10A1 1 0 0 1 9 14V10A1 1 0 0 1 10 9Z")
private val lCircleUser = outline("circle-user", "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z", "M9 10A3 3 0 1 0 15 10A3 3 0 1 0 9 10Z", "M7 20.662V19a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v1.662")
private val lCircleX = outline("circle-x", "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z", "m15 9-6 6", "m9 9 6 6")
private val lClipboardList = outline("clipboard-list", "M9 2H15A1 1 0 0 1 16 3V5A1 1 0 0 1 15 6H9A1 1 0 0 1 8 5V3A1 1 0 0 1 9 2Z", "M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2", "M12 11h4", "M12 16h4", "M8 11h.01", "M8 16h.01")
private val lClock = outline("clock", "M12 6v6l4 2", "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z")
private val lCopy = outline("copy", "M10 8H20A2 2 0 0 1 22 10V20A2 2 0 0 1 20 22H10A2 2 0 0 1 8 20V10A2 2 0 0 1 10 8Z", "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2")
private val lCreditCard = outline("credit-card", "M4 5H20A2 2 0 0 1 22 7V17A2 2 0 0 1 20 19H4A2 2 0 0 1 2 17V7A2 2 0 0 1 4 5Z", "M2 10L22 10")
private val lEar = outline("ear", "M6 8.5a6.5 6.5 0 1 1 13 0c0 6-6 6-6 10a3.5 3.5 0 1 1-7 0", "M15 8.5a2.5 2.5 0 0 0-5 0v1a2 2 0 1 1 0 4")
private val lEllipsis = outline("ellipsis", "M11 12A1 1 0 1 0 13 12A1 1 0 1 0 11 12Z", "M18 12A1 1 0 1 0 20 12A1 1 0 1 0 18 12Z", "M4 12A1 1 0 1 0 6 12A1 1 0 1 0 4 12Z")
private val lExternalLink = outline("external-link", "M15 3h6v6", "M10 14 21 3", "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6")
private val lFile = outline("file", "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z", "M14 2v4a2 2 0 0 0 2 2h4")
private val lFileCode = outline("file-code", "M10 12.5 8 15l2 2.5", "m14 12.5 2 2.5-2 2.5", "M14 2v4a2 2 0 0 0 2 2h4", "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z")
private val lFilePlay = outline("file-play", "M14 2v4a2 2 0 0 0 2 2h4", "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z", "M15.033 13.44a.647.647 0 0 1 0 1.12l-4.065 2.352a.645.645 0 0 1-.968-.56v-4.704a.645.645 0 0 1 .967-.56z")
private val lFileSpreadsheet = outline("file-spreadsheet", "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z", "M14 2v4a2 2 0 0 0 2 2h4", "M8 13h2", "M14 13h2", "M8 17h2", "M14 17h2")
private val lFileText = outline("file-text", "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z", "M14 2v4a2 2 0 0 0 2 2h4", "M10 9H8", "M16 13H8", "M16 17H8")
private val lFiles = outline("files", "M15 2a2 2 0 0 1 1.414.586l4 4A2 2 0 0 1 21 8v7a2 2 0 0 1-2 2h-8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z", "M15 2v4a2 2 0 0 0 2 2h4", "M5 7a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h8a2 2 0 0 0 1.732-1")
private val lFolder = outline("folder", "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z")
private val lFolderGit2 = outline("folder-git-2", "M9 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H20a2 2 0 0 1 2 2v5", "M11 12A2 2 0 1 0 15 12A2 2 0 1 0 11 12Z", "M18 19c-2.8 0-5-2.2-5-5v8", "M18 19A2 2 0 1 0 22 19A2 2 0 1 0 18 19Z")
private val lFolderInput = outline("folder-input", "M2 9V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H20a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-1", "M2 13h10", "m9 16 3-3-3-3")
private val lFolderKanban = outline("folder-kanban", "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z", "M8 10v4", "M12 10v2", "M16 10v6")
private val lFolderOpen = outline("folder-open", "m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2")
private val lFunnel = outline("funnel", "M10 20a1 1 0 0 0 .553.895l2 1A1 1 0 0 0 14 21v-7a2 2 0 0 1 .517-1.341L21.74 4.67A1 1 0 0 0 21 3H3a1 1 0 0 0-.742 1.67l7.225 7.989A2 2 0 0 1 10 14z")
private val lGitBranch = outline("git-branch", "M6 3L6 15", "M15 6A3 3 0 1 0 21 6A3 3 0 1 0 15 6Z", "M3 18A3 3 0 1 0 9 18A3 3 0 1 0 3 18Z", "M18 9a9 9 0 0 1-9 9")
private val lGitPullRequest = outline("git-pull-request", "M15 18A3 3 0 1 0 21 18A3 3 0 1 0 15 18Z", "M3 6A3 3 0 1 0 9 6A3 3 0 1 0 3 6Z", "M13 6h3a2 2 0 0 1 2 2v7", "M6 9L6 21")
private val lGithub = outline("github", "M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4", "M9 18c-4.51 2-5-2-7-2")
private val lGitlab = outline("gitlab", "m22 13.29-3.33-10a.42.42 0 0 0-.14-.18.38.38 0 0 0-.22-.11.39.39 0 0 0-.23.07.42.42 0 0 0-.14.18l-2.26 6.67H8.32L6.1 3.26a.42.42 0 0 0-.1-.18.38.38 0 0 0-.26-.08.39.39 0 0 0-.23.07.42.42 0 0 0-.14.18L2 13.29a.74.74 0 0 0 .27.83L12 21l9.69-6.88a.71.71 0 0 0 .31-.83Z")
private val lGlobe = outline("globe", "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z", "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20", "M2 12h20")
private val lHand = outline("hand", "M18 11V6a2 2 0 0 0-2-2a2 2 0 0 0-2 2", "M14 10V4a2 2 0 0 0-2-2a2 2 0 0 0-2 2v2", "M10 10.5V6a2 2 0 0 0-2-2a2 2 0 0 0-2 2v8", "M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15")
private val lHash = outline("hash", "M4 9L20 9", "M4 15L20 15", "M10 3L8 21", "M16 3L14 21")
private val lHeart = outline("heart", "M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5")
private val lImage = outline("image", "M5 3H19A2 2 0 0 1 21 5V19A2 2 0 0 1 19 21H5A2 2 0 0 1 3 19V5A2 2 0 0 1 5 3Z", "M7 9A2 2 0 1 0 11 9A2 2 0 1 0 7 9Z", "m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21")
private val lInbox = outline("inbox", "M22 12L16 12L14 15L10 15L8 12L2 12", "M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z")
private val lInfo = outline("info", "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z", "M12 16v-4", "M12 8h.01")
private val lItalic = outline("italic", "M19 4L10 4", "M14 20L5 20", "M15 4L9 20")
private val lLayers = outline("layers", "M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z", "M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12", "M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17")
private val lLightbulb = outline("lightbulb", "M15 14c.2-1 .7-1.7 1.5-2.5 1-.9 1.5-2.2 1.5-3.5A6 6 0 0 0 6 8c0 1 .2 2.2 1.5 3.5.7.7 1.3 1.5 1.5 2.5", "M9 18h6", "M10 22h4")
private val lLink = outline("link", "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71", "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71")
private val lList = outline("list", "M3 5h.01", "M3 12h.01", "M3 19h.01", "M8 5h13", "M8 12h13", "M8 19h13")
private val lListChecks = outline("list-checks", "M13 5h8", "M13 12h8", "M13 19h8", "m3 17 2 2 4-4", "m3 7 2 2 4-4")
private val lListMusic = outline("list-music", "M16 5H3", "M11 12H3", "M11 19H3", "M21 16V5", "M15 16A3 3 0 1 0 21 16A3 3 0 1 0 15 16Z")
private val lLoader = outline("loader", "M12 2v4", "m16.2 7.8 2.9-2.9", "M18 12h4", "m16.2 16.2 2.9 2.9", "M12 18v4", "m4.9 19.1 2.9-2.9", "M2 12h4", "m4.9 4.9 2.9 2.9")
private val lLogOut = outline("log-out", "m16 17 5-5-5-5", "M21 12H9", "M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4")
private val lMail = outline("mail", "m22 7-8.991 5.727a2 2 0 0 1-2.009 0L2 7", "M4 4H20A2 2 0 0 1 22 6V18A2 2 0 0 1 20 20H4A2 2 0 0 1 2 18V6A2 2 0 0 1 4 4Z")
private val lMegaphone = outline("megaphone", "M11 6a13 13 0 0 0 8.4-2.8A1 1 0 0 1 21 4v12a1 1 0 0 1-1.6.8A13 13 0 0 0 11 14H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z", "M6 14a12 12 0 0 0 2.4 7.2 2 2 0 0 0 3.2-2.4A8 8 0 0 1 10 14", "M8 6v8")
private val lMenu = outline("menu", "M4 5h16", "M4 12h16", "M4 19h16")
private val lMessageCircle = outline("message-circle", "M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719")
private val lMessageSquare = outline("message-square", "M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z")
private val lMic = outline("mic", "M12 19v3", "M19 10v2a7 7 0 0 1-14 0v-2", "M12 2H12A3 3 0 0 1 15 5V12A3 3 0 0 1 12 15H12A3 3 0 0 1 9 12V5A3 3 0 0 1 12 2Z")
private val lMicOff = outline("mic-off", "M12 19v3", "M15 9.34V5a3 3 0 0 0-5.68-1.33", "M16.95 16.95A7 7 0 0 1 5 12v-2", "M18.89 13.23A7 7 0 0 0 19 12v-2", "m2 2 20 20", "M9 9v3a3 3 0 0 0 5.12 2.12")
private val lMinus = outline("minus", "M5 12h14")
private val lMonitor = outline("monitor", "M4 3H20A2 2 0 0 1 22 5V15A2 2 0 0 1 20 17H4A2 2 0 0 1 2 15V5A2 2 0 0 1 4 3Z", "M8 21L16 21", "M12 17L12 21")
private val lMusic = outline("music", "M9 18V5l12-2v13", "M3 18A3 3 0 1 0 9 18A3 3 0 1 0 3 18Z", "M15 16A3 3 0 1 0 21 16A3 3 0 1 0 15 16Z")
private val lPackage = outline("package", "M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z", "M12 22V12", "M3.29 7L12 12L20.71 7", "m7.5 4.27 9 5.15")
private val lPaperclip = outline("paperclip", "m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551")
private val lPenTool = outline("pen-tool", "M15.707 21.293a1 1 0 0 1-1.414 0l-1.586-1.586a1 1 0 0 1 0-1.414l5.586-5.586a1 1 0 0 1 1.414 0l1.586 1.586a1 1 0 0 1 0 1.414z", "m18 13-1.375-6.874a1 1 0 0 0-.746-.776L3.235 2.028a1 1 0 0 0-1.207 1.207L5.35 15.879a1 1 0 0 0 .776.746L13 18", "m2.3 2.3 7.286 7.286", "M9 11A2 2 0 1 0 13 11A2 2 0 1 0 9 11Z")
private val lPencil = outline("pencil", "M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z", "m15 5 4 4")
private val lPhone = outline("phone", "M13.832 16.568a1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 6.392 6.384")
private val lPhoneIncoming = outline("phone-incoming", "M16 2v6h6", "m22 2-6 6", "M13.832 16.568a1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 6.392 6.384")
private val lPhoneOff = outline("phone-off", "M10.1 13.9a14 14 0 0 0 3.732 2.668 1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2 18 18 0 0 1-12.728-5.272", "M22 2 2 22", "M4.76 13.582A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 .244.473")
private val lPin = outline("pin", "M12 17v5", "M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z")
private val lPlus = outline("plus", "M5 12h14", "M12 5v14")
private val lQrCode = outline("qr-code", "M4 3H7A1 1 0 0 1 8 4V7A1 1 0 0 1 7 8H4A1 1 0 0 1 3 7V4A1 1 0 0 1 4 3Z", "M17 3H20A1 1 0 0 1 21 4V7A1 1 0 0 1 20 8H17A1 1 0 0 1 16 7V4A1 1 0 0 1 17 3Z", "M4 16H7A1 1 0 0 1 8 17V20A1 1 0 0 1 7 21H4A1 1 0 0 1 3 20V17A1 1 0 0 1 4 16Z", "M21 16h-3a2 2 0 0 0-2 2v3", "M21 21v.01", "M12 7v3a2 2 0 0 1-2 2H7", "M3 12h.01", "M12 3h.01", "M12 16v.01", "M16 12h1", "M21 12v.01", "M12 21v-1")
private val lRefreshCw = outline("refresh-cw", "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8", "M21 3v5h-5", "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16", "M8 16H3v5")
private val lRepeat = outline("repeat", "m17 2 4 4-4 4", "M3 11v-1a4 4 0 0 1 4-4h14", "m7 22-4-4 4-4", "M21 13v1a4 4 0 0 1-4 4H3")
private val lReply = outline("reply", "M20 18v-2a4 4 0 0 0-4-4H4", "m9 17-5-5 5-5")
private val lRotateCcw = outline("rotate-ccw", "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8", "M3 3v5h5")
private val lScan = outline("scan", "M3 7V5a2 2 0 0 1 2-2h2", "M17 3h2a2 2 0 0 1 2 2v2", "M21 17v2a2 2 0 0 1-2 2h-2", "M7 21H5a2 2 0 0 1-2-2v-2")
private val lScanLine = outline("scan-line", "M3 7V5a2 2 0 0 1 2-2h2", "M17 3h2a2 2 0 0 1 2 2v2", "M21 17v2a2 2 0 0 1-2 2h-2", "M7 21H5a2 2 0 0 1-2-2v-2", "M7 12h10")
private val lSearch = outline("search", "m21 21-4.34-4.34", "M3 11A8 8 0 1 0 19 11A8 8 0 1 0 3 11Z")
private val lSearchX = outline("search-x", "m13.5 8.5-5 5", "m8.5 8.5 5 5", "M3 11A8 8 0 1 0 19 11A8 8 0 1 0 3 11Z", "m21 21-4.3-4.3")
private val lSend = outline("send", "M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z", "m21.854 2.147-10.94 10.939")
private val lSettings = outline("settings", "M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915", "M9 12A3 3 0 1 0 15 12A3 3 0 1 0 9 12Z")
private val lShieldCheck = outline("shield-check", "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z", "m9 12 2 2 4-4")
private val lSignalHigh = outline("signal-high", "M2 20h.01", "M7 20v-4", "M12 20v-8", "M17 20V8")
private val lSkipBack = outline("skip-back", "M17.971 4.285A2 2 0 0 1 21 6v12a2 2 0 0 1-3.029 1.715l-9.997-5.998a2 2 0 0 1-.003-3.432z", "M3 20V4")
private val lSkipForward = outline("skip-forward", "M21 4v16", "M6.029 4.285A2 2 0 0 0 3 6v12a2 2 0 0 0 3.029 1.715l9.997-5.998a2 2 0 0 0 .003-3.432z")
private val lSlidersHorizontal = outline("sliders-horizontal", "M10 5H3", "M12 19H3", "M14 3v4", "M16 17v4", "M21 12h-9", "M21 19h-5", "M21 5h-7", "M8 10v4", "M8 12H3")
private val lSprout = outline("sprout", "M14 9.536V7a4 4 0 0 1 4-4h1.5a.5.5 0 0 1 .5.5V5a4 4 0 0 1-4 4 4 4 0 0 0-4 4c0 2 1 3 1 5a5 5 0 0 1-1 3", "M4 9a5 5 0 0 1 8 4 5 5 0 0 1-8-4", "M5 21h14")
private val lSquare = outline("square", "M5 3H19A2 2 0 0 1 21 5V19A2 2 0 0 1 19 21H5A2 2 0 0 1 3 19V5A2 2 0 0 1 5 3Z")
private val lSquareKanban = outline("square-kanban", "M5 3H19A2 2 0 0 1 21 5V19A2 2 0 0 1 19 21H5A2 2 0 0 1 3 19V5A2 2 0 0 1 5 3Z", "M8 7v7", "M12 7v4", "M16 7v9")
private val lSquareTerminal = outline("square-terminal", "m7 11 2-2-2-2", "M11 13h4", "M5 3H19A2 2 0 0 1 21 5V19A2 2 0 0 1 19 21H5A2 2 0 0 1 3 19V5A2 2 0 0 1 5 3Z")
private val lTag = outline("tag", "M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z", "M7 7.5A0.5 0.5 0 1 0 8 7.5A0.5 0.5 0 1 0 7 7.5Z")
private val lStar = outline("star", "M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z")
private val lTerminal = outline("terminal", "M12 19h8", "m4 17 6-6-6-6")
private val lTrash2 = outline("trash-2", "M10 11v6", "M14 11v6", "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", "M3 6h18", "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2")
private val lTriangleAlert = outline("triangle-alert", "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3", "M12 9v4", "M12 17h.01")
private val lTruck = outline("truck", "M14 18V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v11a1 1 0 0 0 1 1h2", "M15 18H9", "M19 18h2a1 1 0 0 0 1-1v-3.65a1 1 0 0 0-.22-.624l-3.48-4.35A1 1 0 0 0 17.52 8H14", "M15 18A2 2 0 1 0 19 18A2 2 0 1 0 15 18Z", "M5 18A2 2 0 1 0 9 18A2 2 0 1 0 5 18Z")
private val lUndo = outline("undo", "M3 7v6h6", "M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13")
private val lUndo2 = outline("undo-2", "M9 14 4 9l5-5", "M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11")
private val lUpload = outline("upload", "M12 3v12", "m17 8-5-5-5 5", "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4")
private val lUser = outline("user", "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2", "M8 7A4 4 0 1 0 16 7A4 4 0 1 0 8 7Z")
private val lUserPlus = outline("user-plus", "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2", "M5 7A4 4 0 1 0 13 7A4 4 0 1 0 5 7Z", "M19 8L19 14", "M22 11L16 11")
private val lUsers = outline("users", "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2", "M16 3.128a4 4 0 0 1 0 7.744", "M22 21v-2a4 4 0 0 0-3-3.87", "M5 7A4 4 0 1 0 13 7A4 4 0 1 0 5 7Z")
private val lVolume2 = outline("volume-2", "M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z", "M16 9a5 5 0 0 1 0 6", "M19.364 18.364a9 9 0 0 0 0-12.728")
private val lWifi = outline("wifi", "M12 20h.01", "M2 8.82a15 15 0 0 1 20 0", "M5 12.859a10 10 0 0 1 14 0", "M8.5 16.429a5 5 0 0 1 7 0")
private val lX = outline("x", "M18 6 6 18", "m6 6 12 12")

/** A filled 24-unit mark (icons.json `imported`/`custom`, `custom-icons.generated.ts`). */
private fun mark(name: String, d: String): ImageVector =
    ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f)
        .addPath(PathParser().parsePathString(d).toNodes(), fill = SolidColor(Color.Black))
        .build()

private val osApple = mark("os-apple", "M18.859 9.138c-0.105 0.081 -1.948 1.118 -1.948 3.428c0 2.672 2.345 3.618 2.415 3.639c-0.011 0.056 -0.373 1.294 -1.238 2.556c-0.77 1.107 -1.575 2.215 -2.798 2.215s-1.54 -0.71 -2.953 -0.71c-1.378 0 -1.867 0.735 -2.985 0.735s-1.902 -1.027 -2.798 -2.285c-1.041 -1.48 -1.881 -3.779 -1.881 -5.959c0 -3.498 2.275 -5.354 4.514 -5.354c1.188 0 2.183 0.78 2.929 0.78c0.71 0 1.821 -0.83 3.175 -0.83c0.513 0 2.355 0.046 3.568 1.786m-4.212 -3.27c0.559 -0.664 0.956 -1.586 0.956 -2.507c0 -0.127 -0.011 -0.257 -0.035 -0.362c-0.911 0.035 -1.993 0.608 -2.647 1.364c-0.513 0.584 -0.991 1.505 -0.991 2.44c0 0.141 0.025 0.281 0.035 0.327c0.056 0.011 0.151 0.025 0.246 0.025c0.816 -0.004 1.842 -0.548 2.436 -1.287")
private val osWindows = mark("os-windows", "M3 3h8.532v8.529H3Zm9.468 0H21v8.529H12.468ZM3 12.468h8.532V21H3Zm9.468 0H21V21H12.468")
private val osLinux = mark("os-linux", "M19.471 17.752c-0.12 -0.109 -0.299 -0.19 -0.418 -0.25c-0.179 -0.07 -0.299 -0.162 -0.418 -0.267c-0.179 -0.109 -0.239 -0.232 -0.299 -0.362l-0.06 -0.439c-0.06 -0.281 -0.12 -0.461 -0.179 -0.541s-0.12 -0.13 -0.239 -0.13c0.12 -0.387 0.179 -0.787 0.179 -1.199c0 -0.83 -0.179 -1.603 -0.478 -2.201c-0.239 -0.654 -0.654 -1.311 -1.192 -1.905c-0.239 -0.359 -0.478 -0.654 -0.654 -0.953c-0.179 -0.239 -0.299 -0.478 -0.418 -0.714c-0.12 -0.179 -0.179 -0.418 -0.239 -0.654c-0.06 -0.299 -0.06 -0.538 -0.12 -0.714c0 -0.179 -0.06 -0.418 -0.06 -0.773c-0.06 -0.418 -0.06 -0.714 -0.12 -1.012c-0.06 -0.478 -0.179 -0.833 -0.299 -1.192c-0.179 -0.299 -0.359 -0.598 -0.598 -0.773c-0.179 -0.179 -0.418 -0.299 -0.654 -0.418c-0.299 -0.12 -0.538 -0.179 -0.714 -0.179C12.316 3 12.137 3 11.961 3c-0.478 0 -0.893 0.06 -1.252 0.179s-0.598 0.359 -0.773 0.598c-0.204 0.239 -0.341 0.538 -0.418 0.833c-0.077 0.299 -0.113 0.598 -0.113 1.012c0 0.538 0.028 1.132 0.088 1.73c0.067 0.598 0.095 0.953 0.095 1.072c0 0.239 -0.06 0.538 -0.172 0.773s-0.316 0.538 -0.626 0.893L8.344 10.629c-0.359 0.418 -0.584 0.833 -0.679 1.252c-0.12 0.478 -0.225 0.833 -0.327 1.072c-0.095 0.239 -0.225 0.478 -0.401 0.798l-0.207 0.376c-0.214 0.401 -0.316 0.703 -0.316 0.918c-0.011 0.035 -0.011 0.095 -0.011 0.162c0 0.095 0.025 0.172 0.07 0.232c-0.042 0.049 -0.07 0.095 -0.095 0.137c-0.011 0.028 -0.025 0.067 -0.035 0.102c-0.011 0.025 -0.025 0.077 -0.053 0.148c-0.025 0.07 -0.06 0.148 -0.102 0.246c-0.127 0.07 -0.207 0.102 -0.264 0.102H5.236c-0.13 0 -0.257 0.077 -0.387 0.225s-0.19 0.359 -0.19 0.619c0 0.084 0.025 0.281 0.07 0.591c0.049 0.316 0.07 0.496 0.07 0.555s-0.011 0.12 -0.025 0.179c-0.011 0.067 -0.035 0.12 -0.06 0.172c-0.025 0.06 -0.049 0.113 -0.084 0.165c-0.025 0.06 -0.06 0.12 -0.102 0.186c-0.042 0.06 -0.084 0.127 -0.123 0.19c-0.042 0.109 -0.06 0.211 -0.06 0.306s0.042 0.179 0.123 0.253c0.088 0.074 0.207 0.137 0.359 0.183c0.155 0.056 0.299 0.095 0.422 0.123c0.137 0.028 0.292 0.056 0.461 0.081c0.172 0.028 0.281 0.049 0.327 0.06c0.482 0.109 1.037 0.274 1.663 0.489c0.619 0.225 1.079 0.341 1.371 0.341c0.464 0 0.798 -0.186 1.097 -0.541v0.018c0.418 -0.207 1.012 -0.313 1.789 -0.313c0.478 0 1.252 0.056 2.204 0.172c0.179 0.362 0.478 0.601 0.953 0.728l0.418 0.049c0.239 0 0.478 -0.053 0.654 -0.162c0.179 -0.102 0.418 -0.267 0.654 -0.499l0.598 -0.506c0.299 -0.253 0.598 -0.468 0.893 -0.643c0.359 -0.169 0.598 -0.288 0.714 -0.348c0.179 -0.053 0.299 -0.137 0.418 -0.257c0.12 -0.109 0.179 -0.257 0.179 -0.461c0.004 -0.134 -0.056 -0.257 -0.172 -0.376M15.364 11.438c0.299 0.06 0.538 0.239 0.773 0.598c0.239 0.359 0.359 0.773 0.359 1.371c0 0.429 -0.06 0.918 -0.239 1.473c0.06 0.011 0.179 0.025 0.239 0.028c0.06 0 0.06 0 0.12 0.011c0 0 0.06 0 0.12 0.007c0.359 0.049 0.654 0.162 0.833 0.341c0.179 0.172 0.239 0.327 0.239 0.461c0 0.053 0 0.077 -0.06 0.077c0 0 -0.06 -0.035 -0.06 -0.113c-0.06 -0.077 -0.12 -0.162 -0.179 -0.25c-0.06 -0.095 -0.179 -0.179 -0.418 -0.257c-0.179 -0.084 -0.478 -0.127 -0.773 -0.127c-0.179 0 -0.299 0.053 -0.478 0.155c-0.12 0.102 -0.179 0.204 -0.299 0.302c-0.06 0.102 -0.12 0.155 -0.12 0.155c-0.06 0 -0.06 -0.028 -0.06 -0.084c0.179 -0.345 0.418 -0.566 0.654 -0.661c0.239 -0.633 0.299 -1.125 0.299 -1.459c0 -0.418 -0.06 -0.773 -0.239 -1.192c-0.179 -0.359 -0.478 -0.598 -0.773 -0.714c0.004 -0.063 0.004 -0.063 0.063 -0.123M12.084 5.848q0.269 -0.179 0.538 -0.179c0.239 0 0.418 0.06 0.598 0.239c0.12 0.179 0.179 0.418 0.179 0.773c0 0.299 -0.06 0.538 -0.12 0.893c-0.12 -0.06 -0.179 -0.12 -0.299 -0.12c-0.06 0 -0.12 -0.06 -0.12 -0.06q0.359 -0.09 0.359 -0.538c0 -0.179 -0.06 -0.299 -0.12 -0.478c-0.12 -0.12 -0.239 -0.239 -0.418 -0.239q-0.179 0 -0.359 0.179c-0.12 0.179 -0.12 0.299 -0.12 0.538c0 0.12 0 0.239 0.06 0.299c-0.06 0 -0.12 -0.06 -0.179 -0.06c-0.12 -0.239 -0.179 -0.478 -0.179 -0.773c0 -0.176 0.06 -0.295 0.179 -0.475m0.657 0.657c0 -0.06 -0.06 -0.06 -0.06 -0.12c0.06 0 0.12 -0.06 0.12 -0.06c0.06 0 0.12 0.06 0.179 0.12s0.06 0.12 0.06 0.179s0 0.12 0.06 0.179h-0.12c-0.06 -0.179 -0.12 -0.299 -0.239 -0.299m-0.179 0.893c0.598 0.179 0.893 0.359 0.953 0.478c0.06 0 0.06 0.06 0.06 0.12c0 0.179 -0.12 0.299 -0.239 0.418l-0.538 0.359c-0.598 0.418 -1.072 0.654 -1.491 0.654c-0.299 0 -0.598 -0.12 -0.833 -0.359l-0.359 -0.359c-0.06 -0.06 -0.12 -0.179 -0.179 -0.299c0.359 0.299 0.773 0.478 1.192 0.478c0.478 0 1.072 -0.239 1.849 -0.773c0 0 0 -0.06 -0.06 -0.06v-0.12c-0.773 0.598 -1.371 0.833 -1.849 0.833c-0.359 0 -0.714 -0.179 -1.072 -0.478c-0.06 -0.12 -0.12 -0.179 -0.12 -0.179c0 -0.239 0.239 -0.478 0.598 -0.714c0.418 -0.239 0.714 -0.359 0.893 -0.359c0.243 0 0.657 0.12 1.195 0.359m-2.264 -0.893v-0.12c0.06 0 0.06 -0.06 0.06 -0.06c0.179 0 0.299 0.179 0.299 0.478h-0.12c0 -0.12 -0.06 -0.179 -0.06 -0.299Zm-0.359 -0.418c0.06 -0.179 0.239 -0.299 0.418 -0.299c0.239 0 0.359 0.12 0.478 0.299s0.12 0.359 0.12 0.538c0 0.239 0 0.359 -0.06 0.359v0.06h-0.06v-0.12c0 -0.179 -0.06 -0.359 -0.12 -0.538c-0.12 -0.12 -0.179 -0.179 -0.299 -0.179s-0.239 0.06 -0.299 0.179c-0.12 0.12 -0.12 0.239 -0.12 0.418c0 0.299 0.06 0.478 0.299 0.598c-0.06 0.06 -0.12 0.06 -0.179 0.12c-0.239 -0.239 -0.323 -0.478 -0.323 -0.714c0 -0.302 0.025 -0.482 0.144 -0.721m0.053 14.126c-0.285 0.359 -0.633 0.541 -1.048 0.541c-0.162 0 -0.601 -0.137 -1.322 -0.401s-1.361 -0.439 -1.912 -0.52v-0.004c-0.721 -0.095 -1.114 -0.257 -1.174 -0.489c0 -0.137 0.077 -0.327 0.225 -0.584c0.148 -0.246 0.221 -0.489 0.221 -0.735c0 -0.127 -0.025 -0.302 -0.07 -0.524c-0.049 -0.225 -0.07 -0.387 -0.07 -0.482c0 -0.207 0.077 -0.404 0.225 -0.577c0.113 -0.06 0.327 -0.095 0.643 -0.095h0.162c0.077 -0.007 0.13 -0.007 0.162 -0.007c0.137 -0.028 0.239 -0.06 0.299 -0.095c0.06 -0.042 0.095 -0.127 0.113 -0.25c0.011 -0.13 0.06 -0.257 0.144 -0.369s0.257 -0.197 0.506 -0.246c0.197 0.049 0.376 0.137 0.531 0.274c0.162 0.137 0.285 0.274 0.362 0.411c0.088 0.127 0.204 0.302 0.327 0.538c0.13 0.232 0.232 0.404 0.316 0.517l0.404 0.541c0.679 0.865 1.02 1.575 1.02 2.141c-0.004 0.158 -0.004 0.295 -0.063 0.411m1.311 -1.167c-0.714 0 -1.252 -0.165 -1.501 -0.506c-0.127 -0.232 -0.186 -0.352 -0.186 -0.359c0 -0.028 0.007 -0.042 0.018 -0.042c0.137 0 0.299 -0.07 0.418 -0.214c0.179 -0.137 0.239 -0.264 0.239 -0.387c0 -0.239 -0.359 -0.643 -1.03 -1.227l-0.633 -0.411c-0.499 -0.362 -0.816 -0.65 -0.949 -0.847c-0.12 -0.204 -0.186 -0.436 -0.186 -0.703c0 -0.362 0.13 -0.858 0.38 -1.396c0.25 -0.598 0.461 -1.012 0.633 -1.192l0.06 0.06c0.049 0 0.077 0 0.088 0.06c-0.207 0.239 -0.422 0.598 -0.636 1.132c-0.221 0.538 -0.327 0.988 -0.327 1.399c0 0.334 0.13 0.591 0.401 0.773c-0.025 -0.144 -0.035 -0.285 -0.035 -0.422c0 -0.686 0.246 -1.515 0.728 -2.408l0.446 -0.893c0.011 0 0.028 -0.12 0.053 -0.239c0.018 -0.179 0.109 -0.418 0.274 -0.833l0.376 -0.714c0.12 -0.239 0.179 -0.478 0.179 -0.654l0.359 0.299c0.179 0.239 0.478 0.299 0.833 0.299c0.418 0 0.893 -0.179 1.491 -0.598l0.538 -0.418c0.12 0.239 0.239 0.538 0.359 0.833c0.179 0.299 0.299 0.598 0.478 0.893c0 0.299 0.06 0.478 0.12 0.714c0.12 0.179 0.179 0.359 0.299 0.538l0.359 0.538c0.12 0.239 0.239 0.538 0.299 0.893c0.12 0.418 0.12 0.865 0.12 1.406c0 0.359 0 0.756 -0.06 1.216c-0.12 -0.088 -0.239 -0.13 -0.359 -0.13c-0.179 0 -0.299 0.088 -0.418 0.281c-0.12 0.19 -0.179 0.482 -0.179 0.868v0.221c0.06 0.077 0.06 0.264 0.06 0.559c-0.78 1.076 -1.793 1.61 -3.104 1.61m7.991 -0.503c-0.12 0.095 -0.239 0.165 -0.418 0.214c-0.12 0.053 -0.359 0.148 -0.598 0.292c-0.299 0.144 -0.538 0.309 -0.773 0.499l-1.012 0.875c-0.06 0.028 -0.06 0.06 -0.179 0.102c-0.06 0.049 -0.12 0.088 -0.179 0.13c-0.06 0.049 -0.179 0.088 -0.299 0.113q-0.179 0.053 -0.359 0.053c-0.478 0 -0.893 -0.225 -1.132 -0.675v-0.014l0.12 -0.661c0 -0.236 0 -0.446 0.06 -0.633c0 -0.186 0.06 -0.302 0.06 -0.359v-1.853c0 -0.626 0.12 -0.935 0.418 -0.935c0.12 0 0.179 0.053 0.299 0.155c0.06 0.25 0.12 0.471 0.239 0.661c0.179 0.257 0.418 0.387 0.893 0.387c0.12 0 0.299 -0.053 0.478 -0.155c0.12 -0.102 0.299 -0.207 0.359 -0.327c0.12 -0.12 0.239 -0.232 0.359 -0.334c0.06 -0.102 0.179 -0.155 0.239 -0.155s0.06 0.007 0.06 0.025c0.06 0.067 0.06 0.186 0.12 0.345l0.06 0.345c0.06 0.281 0.179 0.496 0.299 0.65c0.179 0.162 0.299 0.257 0.418 0.299s0.239 0.109 0.359 0.204s0.239 0.225 0.239 0.401c-0.007 0.148 -0.067 0.264 -0.127 0.352m-7.805 -11.204s-0.06 -0.06 -0.06 -0.12s0 -0.06 0.06 -0.06l0.12 0.06c0.06 0.06 0.12 0.12 0.12 0.179s-0.06 0.06 -0.06 0.06h-0.06c-0.06 0 -0.06 -0.06 -0.12 -0.12m-0.598 0.06c0 -0.06 0 -0.12 0.06 -0.179c0.12 0 0.179 -0.06 0.179 -0.06c0.06 0 0.06 0.06 0.06 0.06c0 0.06 -0.06 0.12 -0.179 0.239h-0.06c-0.06 0 -0.06 0 -0.06 -0.06")
private val brandExponential = mark("brand-exponential", "M2.172 13.845C4.9 13.894 7.707 12.624 9.109 10.214C10.607 7.77 11.029 4.849 11.172 2.034C8.351 2.269 5.762 3.687 4.046 5.939C2.331 8.19 1.649 11.063 2.172 13.845ZM2.472 15.037C5.561 15.022 8.654 13.454 10.202 10.729C11.767 8.11 12.233 5.008 12.375 2.007C13.577 2.052 14.761 2.314 15.871 2.779C15.526 6.188 14.654 9.626 12.766 12.523C10.867 15.524 7.672 17.602 4.191 18.246C3.427 17.291 2.844 16.203 2.472 15.037ZM5.129 19.266C8.677 18.442 11.862 16.205 13.808 13.122C15.695 10.206 16.624 6.773 17.017 3.349C18.221 4.048 19.267 4.99 20.086 6.116C19.383 9.263 18.182 12.326 16.344 14.987C14.438 17.77 11.785 20.041 8.714 21.445C7.379 20.98 6.157 20.237 5.129 19.266ZM10.498 21.887C13.231 20.391 15.579 18.223 17.339 15.657C19.028 13.211 20.207 10.438 20.966 7.571C22.616 10.912 22.272 14.894 20.072 17.903C17.872 20.911 14.182 22.446 10.498 21.887Z")
private val brandClaude = mark("brand-claude", "M4.704 15.96l4.728 -2.64l0.072 -0.24l-0.072 -0.12h-0.24l-0.792 -0.048l-2.688 -0.072L3.36 12.72l-2.28 -0.12l-0.576 -0.12L0 11.76l0.048 -0.36l0.48 -0.312l0.696 0.048l1.512 0.12l2.28 0.144l1.656 0.096L9.12 11.784h0.384l0.048 -0.168l-0.12 -0.096l-0.096 -0.096L6.96 9.84l-2.544 -1.68l-1.344 -0.984l-0.72 -0.48l-0.36 -0.48l-0.144 -1.008l0.648 -0.72l0.888 0.072l0.216 0.048l0.888 0.696l1.92 1.464L8.88 8.64l0.36 0.288l0.144 -0.096l0.024 -0.072l-0.168 -0.264L7.92 6l-1.44 -2.496l-0.648 -1.032l-0.168 -0.624c-0.072 -0.24 -0.096 -0.48 -0.096 -0.72l0.72 -1.008L6.72 0l1.008 0.144L8.112 0.48l0.624 1.44l0.984 2.232L11.28 7.176l0.48 0.912l0.24 0.816l0.072 0.24h0.168v-0.12l0.12 -1.728l0.24 -2.088l0.24 -2.688l0.072 -0.768l0.384 -0.912l0.72 -0.48L14.64 0.624l0.48 0.696l-0.072 0.432l-0.264 1.848L14.16 6.504l-0.36 1.968h0.216l0.24 -0.264l0.984 -1.296l1.656 -2.064l0.72 -0.84L18.48 3.12l0.552 -0.432h1.032l0.744 1.128l-0.336 1.176l-1.056 1.344l-0.888 1.128l-1.272 1.704l-0.768 1.368l0.072 0.096h0.168l2.88 -0.624l1.536 -0.264l1.824 -0.312l0.84 0.384l0.096 0.384l-0.336 0.816l-1.968 0.48l-2.304 0.48l-3.432 0.792l-0.048 0.024l0.048 0.072l1.536 0.144l0.672 0.048h1.632l3.024 0.24l0.792 0.48l0.456 0.648l-0.072 0.48l-1.224 0.624l-1.632 -0.384l-3.84 -0.912l-1.296 -0.312h-0.192v0.096l1.104 1.08l1.992 1.8L21.36 19.224l0.12 0.576l-0.312 0.48l-0.336 -0.048l-2.208 -1.68l-0.864 -0.72l-1.92 -1.632h-0.12v0.168l0.432 0.648l2.352 3.528l0.12 1.08l-0.168 0.336l-0.624 0.24l-0.648 -0.144l-1.392 -1.92l-1.44 -2.16l-1.128 -1.968l-0.12 0.096l-0.696 7.248l-0.312 0.36l-0.72 0.288l-0.6 -0.48l-0.336 -0.72l0.336 -1.488l0.384 -1.92l0.312 -1.536l0.288 -1.896l0.168 -0.624v-0.048H11.76L10.32 17.28l-2.16 2.952l-1.728 1.824l-0.408 0.168l-0.72 -0.36l0.072 -0.672L5.76 20.64l2.4 -3.072l1.44 -1.896l0.96 -1.104l-0.024 -0.12h-0.072L4.128 18.576l-1.128 0.144l-0.48 -0.48l0.048 -0.72l0.24 -0.24l1.92 -1.32Z")
private val brandCodex = mark("brand-codex", "M9.064 3.344a4.578 4.578 0 0 1 2.285 -0.312c1 0.115 1.891 0.54 2.673 1.275c0.01 0.01 0.024 0.017 0.037 0.021a0.09 0.09 0 0 0 0.043 0a4.55 4.55 0 0 1 3.046 0.275l0.047 0.022l0.116 0.057a4.581 4.581 0 0 1 2.188 2.399c0.209 0.51 0.313 1.041 0.315 1.595a4.24 4.24 0 0 1 -0.134 1.223a0.123 0.123 0 0 0 0.03 0.115c0.594 0.607 0.988 1.33 1.183 2.17c0.289 1.425 -0.007 2.71 -0.887 3.854l-0.136 0.166a4.548 4.548 0 0 1 -2.201 1.388a0.123 0.123 0 0 0 -0.081 0.076c-0.191 0.551 -0.383 1.023 -0.74 1.494c-0.9 1.187 -2.222 1.846 -3.711 1.838c-1.187 -0.006 -2.239 -0.44 -3.157 -1.302a0.107 0.107 0 0 0 -0.105 -0.024c-0.388 0.125 -0.78 0.143 -1.204 0.138a4.441 4.441 0 0 1 -1.945 -0.466a4.544 4.544 0 0 1 -1.61 -1.335c-0.152 -0.202 -0.303 -0.392 -0.414 -0.617a5.81 5.81 0 0 1 -0.37 -0.961a4.582 4.582 0 0 1 -0.014 -2.298a0.124 0.124 0 0 0 0.006 -0.056a0.085 0.085 0 0 0 -0.027 -0.048a4.467 4.467 0 0 1 -1.034 -1.651a3.896 3.896 0 0 1 -0.251 -1.192a5.189 5.189 0 0 1 0.141 -1.6c0.337 -1.112 0.982 -1.985 1.933 -2.618c0.212 -0.141 0.413 -0.251 0.601 -0.33c0.215 -0.089 0.43 -0.164 0.646 -0.227a0.098 0.098 0 0 0 0.065 -0.066a4.51 4.51 0 0 1 0.829 -1.615a4.535 4.535 0 0 1 1.837 -1.388Zm3.482 10.565a0.637 0.637 0 0 0 0 1.272h3.636a0.637 0.637 0 1 0 0 -1.272h-3.636ZM8.462 9.23a0.637 0.637 0 0 0 -1.106 0.631l1.272 2.224l-1.266 2.136a0.636 0.636 0 1 0 1.095 0.649l1.454 -2.455a0.636 0.636 0 0 0 0.005 -0.64L8.462 9.23Z")

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
 * The catalog icon names the kitchen sink, the specimens and the eval
 * surfaces draw, resolved the way the web harness resolves them
 * (`packages/exponential-ui-react/harness/icons.ts`): an icons.json
 * `semantic` concept to its Lucide name, else the bare Lucide name, plus the
 * `custom`/`imported` marks. A name neither resolves (web's placeholder)
 * stays out, so the painter draws its placeholder too. A real host hands
 * the renderer its own registry (the Exponential app's generated Lucide
 * set); the SDK ships none.
 */
val sinkIcons: Map<String, ImageVector> = mapOf(
    "action-chat" to lMessageCircle,
    "archive" to lArchive,
    "arrow-left" to lArrowLeft,
    "arrow-right" to lArrowRight,
    "at-sign" to lAtSign,
    "audio-lines" to lAudioLines,
    "bell" to lBell,
    "briefcase" to lBriefcase,
    "bug" to lBug,
    "building-2" to lBuilding2,
    "calendar" to lCalendar,
    "call-end" to lPhoneOff,
    "call-incoming" to lPhoneIncoming,
    "call-mute" to lMic,
    "call-speaker" to lVolume2,
    "call-speaker-off" to lEar,
    "call-unmute" to lMicOff,
    "check" to lCheck,
    "check-check" to lCheckCheck,
    "check-circle" to lCircleCheckBig,
    "chevron-left" to lChevronLeft,
    "chevron-right" to lChevronRight,
    "chevrons-up-down" to lChevronsUpDown,
    "chrome" to lChromium,
    "circle" to lCircle,
    "circle-check" to lCircleCheck,
    "circle-dashed" to lCircleDashed,
    "circle-dot" to lCircleDot,
    "circle-x" to lCircleX,
    "clock" to lClock,
    "coding-plan" to lClipboardList,
    "copy" to lCopy,
    "credit-card" to lCreditCard,
    "editor-bold" to lBold,
    "editor-italic" to lItalic,
    "external-link" to lExternalLink,
    "file" to lFile,
    "file-code" to lFileCode,
    "file-spreadsheet" to lFileSpreadsheet,
    "file-text" to lFileText,
    "file-video" to lFilePlay,
    "files" to lFiles,
    "filter" to lFunnel,
    "folder" to lFolder,
    "folder-git-2" to lFolderGit2,
    "folder-input" to lFolderInput,
    "git-branch" to lGitBranch,
    "git-pull-request" to lGitPullRequest,
    "github" to lGithub,
    "gitlab" to lGitlab,
    "globe" to lGlobe,
    "hand" to lHand,
    "hash" to lHash,
    "heart" to lHeart,
    "image" to lImage,
    "info" to lInfo,
    "kanban-square" to lSquareKanban,
    "layers" to lLayers,
    "lightbulb" to lLightbulb,
    "list-music" to lListMusic,
    "loader" to lLoader,
    "log-out" to lLogOut,
    "mail" to lMail,
    "media-volume-up" to lVolume2,
    "menu" to lMenu,
    "message-circle" to lMessageCircle,
    "message-square" to lMessageSquare,
    "mic" to lMic,
    "mic-off" to lMicOff,
    "monitor" to lMonitor,
    "music" to lMusic,
    "nav-boards" to lFolderKanban,
    "nav-changelog" to lMegaphone,
    "nav-inbox" to lInbox,
    "nav-issues" to lList,
    "nav-search" to lSearch,
    "nav-settings" to lSettings,
    "package" to lPackage,
    "pen-tool" to lPenTool,
    "pencil" to lPencil,
    "phone" to lPhone,
    "phone-incoming" to lPhoneIncoming,
    "phone-off" to lPhoneOff,
    "plus" to lPlus,
    "pr-open" to lGitPullRequest,
    "qr-code" to lQrCode,
    "refresh-cw" to lRefreshCw,
    "reply" to lReply,
    "rotate-ccw" to lRotateCcw,
    "scan" to lScan,
    "scan-line" to lScanLine,
    "search" to lSearch,
    "search-x" to lSearchX,
    "send" to lSend,
    "settings" to lSettings,
    "settings-issues" to lListChecks,
    "settings-labels" to lTag,
    "settings-statuses" to lCircleDot,
    "settings-tools" to lSquareTerminal,
    "shield-check" to lShieldCheck,
    "signal-high" to lSignalHigh,
    "skip-back" to lSkipBack,
    "skip-forward" to lSkipForward,
    "sprout" to lSprout,
    "square" to lSquare,
    "square-kanban" to lSquareKanban,
    "star" to lStar,
    "status-backlog" to lCircleDashed,
    "status-done" to lCircleCheck,
    "terminal" to lTerminal,
    "trace-background" to lRepeat,
    "trace-chevron" to lChevronRight,
    "trace-edit" to lPencil,
    "trace-failed" to lCircleX,
    "trace-read" to lFileText,
    "trace-reasoning" to lBrain,
    "trace-search" to lSearch,
    "trace-subagent" to lBot,
    "trace-task-done" to lCircleCheck,
    "trace-terminal" to lTerminal,
    "trash-2" to lTrash2,
    "truck" to lTruck,
    "ui-add" to lPlus,
    "ui-archive" to lArchive,
    "ui-arrow-right" to lArrowRight,
    "ui-assignee" to lCircleUser,
    "ui-attach" to lPaperclip,
    "ui-back" to lArrowLeft,
    "ui-check" to lCheck,
    "ui-chevron-down" to lChevronDown,
    "ui-chevron-left" to lChevronLeft,
    "ui-chevron-right" to lChevronRight,
    "ui-chevron-up" to lChevronUp,
    "ui-clock" to lClock,
    "ui-close" to lX,
    "ui-copy" to lCopy,
    "ui-device" to lMonitor,
    "ui-edit" to lPencil,
    "ui-error" to lCircleX,
    "ui-external-link" to lExternalLink,
    "ui-file" to lFile,
    "ui-folder" to lFolder,
    "ui-folder-open" to lFolderOpen,
    "ui-hold" to lHand,
    "ui-icon-placeholder" to lCircleDashed,
    "ui-info" to lInfo,
    "ui-issue" to lFileText,
    "ui-link" to lLink,
    "ui-minus" to lMinus,
    "ui-more" to lEllipsis,
    "ui-pin" to lPin,
    "ui-properties" to lSlidersHorizontal,
    "ui-qr-code" to lQrCode,
    "ui-read" to lCheckCheck,
    "ui-scan" to lScanLine,
    "ui-search" to lSearch,
    "ui-selector" to lChevronsUpDown,
    "ui-send" to lSend,
    "ui-stop" to lCircleStop,
    "ui-success" to lCircleCheck,
    "ui-undo" to lUndo2,
    "ui-warning" to lTriangleAlert,
    "undo" to lUndo,
    "undo-2" to lUndo2,
    "upload" to lUpload,
    "user" to lUser,
    "user-plus" to lUserPlus,
    "users" to lUsers,
    "voice-level" to lAudioLines,
    "wifi" to lWifi,
    "x" to lX,
    // icons.json `custom` / `imported` marks.
    "status-in-progress" to progress24,
    "progress-2-4" to progress24,
    "os-apple" to osApple,
    "os-windows" to osWindows,
    "os-linux" to osLinux,
    "brand-exponential" to brandExponential,
    // #EXP-1764: the agent marks (Claude in its own orange: the painter's `iconFills`).
    "brand-claude" to brandClaude,
    "brand-codex" to brandCodex,
)

/**
 * The host's `icon(name, size)`: the concept's 24-unit glyph filling the
 * slot (web sizes the Lucide `<svg>` to the slot; the glyph's own margin
 * is in its viewBox), tinted with the `LocalContentColor` the painter
 * provides; null (the painter's placeholder circle) for a concept the map
 * lacks.
 */
fun sinkIcon(name: String, size: Float): (@Composable () -> Unit)? {
    val vector = sinkIcons[name] ?: return null
    return {
        Box(Modifier.size(size.dp), contentAlignment = Alignment.Center) {
            Icon(vector, contentDescription = null, modifier = Modifier.size(size.dp))
        }
    }
}
