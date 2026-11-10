package com.exponential.app.ui.markdown

import androidx.compose.foundation.clickable
import com.exponential.app.ui.theme.ExpMono
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.isSpecified
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import at.exponential.ui.json.JsonValue
import at.exponential.ui.paint.MarkdownPainter
import at.exponential.ui.primitives.MarkdownAttachment
import at.exponential.ui.primitives.MarkdownBlockKind
import at.exponential.ui.primitives.MarkdownChip
import at.exponential.ui.primitives.MarkdownCodeHighlighter
import at.exponential.ui.primitives.MarkdownColors
import at.exponential.ui.primitives.MarkdownImageSizes
import at.exponential.ui.primitives.MarkdownLaidOut
import at.exponential.ui.primitives.MarkdownOptions
import at.exponential.ui.primitives.MarkdownSize
import at.exponential.ui.primitives.MarkdownStyles
import at.exponential.ui.primitives.MarkdownTextLink
import at.exponential.ui.primitives.MarkdownTextSpec
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ResolvedTextStyle
import at.exponential.ui.theme.ThemeHandle
import coil3.compose.AsyncImage
import coil3.compose.AsyncImagePainter
import com.exponential.app.domain.AppLink
import com.exponential.app.domain.ProseLinks
import com.exponential.app.domain.classifyAppLink
import com.exponential.app.ui.apps.APP_THEME_ID
import com.exponential.app.ui.components.entityConceptIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.media.AudioBlockView
import com.exponential.app.ui.markdown.media.VideoBlockView
import com.exponential.app.ui.theme.AppPrimitiveTokens
import com.exponential.app.ui.theme.resolvedStatusColor
import at.exponential.ui.primitives.MarkdownView as GenUiMarkdownView

private val MONO_FONTS = at.exponential.ui.primitives.MarkdownFontResolver { _, mono -> if (mono) ExpMono else null }

/**
 * Read-only markdown: the shared display path for issue descriptions,
 * comments, the agent transcript, plans and results. #VAPP-258: ONE
 * implementation with genUI's catalog `Markdown` (the Exponential UI
 * primitives' `MarkdownView` on the `Markdown` recipes of the app's theme,
 * [MarkdownPainter.styles]); this file only supplies the host hooks
 * (catalog/host.json `markdown`): `#IDENT` refs ([LocalIssueRefs]) and
 * `@email` mentions ([LocalMentions]) as chip segments, the transcript's
 * rewritten in-app links (prose-links.json) as entity chips, links through
 * the app-link rule ([LocalAppLinks], app-link.json), `/api/attachments/{id}`
 * images sized from the synced rows ([LocalAttachmentDims]) and pressable
 * through [LocalMarkdownImageClick], attachment links as players or a file
 * row. Editors keep their own renderer ([MarkdownEditor]).
 */
@Composable
fun MarkdownView(
    markdown: String,
    modifier: Modifier = Modifier,
    /** `breaks: hard`: every newline inside a paragraph breaks the line —
     *  agent output (EXP-440) is written for a terminal. */
    softBreaksAsNewlines: Boolean = false,
    /** `density: chat` (the transcript rhythm); default = an agent surface
     *  (hard breaks or a transcript body style in force), else `document`. */
    chat: Boolean = softBreaksAsNewlines || LocalMarkdownBodyStyle.current != MdStyle.body,
    /** `streaming: true`: [markdown] is a growing prefix (a reply being written). */
    streaming: Boolean = false,
    /** `imageMaxHeight`: a document's screenshots keep their width (none, at
     *  most the screen's height); a chat caps them at [CHAT_IMAGE_MAX_HEIGHT]. */
    imageMaxHeight: Float? = if (chat) CHAT_IMAGE_MAX_HEIGHT else null,
    /** The laid-out document (a fold at whole lines, [MarkdownLaidOut.foldHeight]). */
    onLayout: ((MarkdownLaidOut) -> Unit)? = null,
) {
    if (markdown.isBlank()) return
    val issueRefs = LocalIssueRefs.current
    val mentions = LocalMentions.current
    val appLinks = LocalAppLinks.current
    val bare = LocalIssueRefBare.current
    val autolink = LocalMarkdownAutolink.current
    val dims = LocalAttachmentDims.current
    val opener by rememberUpdatedState(LocalAttachmentLinkOpener.current)
    val onImageClick = LocalMarkdownImageClick.current
    val uri = LocalUriHandler.current
    val shown = rememberChippedProse(markdown)
    val source = remember(shown, autolink) { AppMarkdown.prepare(shown, autolink) }
    val look = AppMarkdown.look(chat, LocalDensity.current.fontScale)
    val policy = remember(appLinks) { { href: String -> AppMarkdown.police(href, appLinks) } }
    val segments = remember(source, issueRefs, mentions, bare) {
        { piece: String -> AppMarkdown.segments(piece, source, issueRefs, mentions, bare) }
    }
    val imageSize = remember(dims) { { src: String -> AppMarkdown.imageSize(src, dims) } }
    val attachments = remember(dims, source) { { href: String -> AppMarkdown.attachment(href, dims, source) } }
    val onLink = remember(issueRefs, appLinks, uri) {
        { href: String -> AppMarkdown.open(href, issueRefs, appLinks, opener) { runCatching { uri.openUri(it) } } }
    }
    GenUiMarkdownView(
        text = source.text,
        modifier = modifier,
        styles = look.styles,
        colors = look.colors,
        codeBlockRadius = look.codeRadius.dp,
        monoFamily = null,
        // #EXP-1629: code spans and blocks in the bundled JetBrains Mono.
        fonts = MONO_FONTS,
        onLink = onLink,
        linkPolicy = policy,
        linkSegments = segments,
        imageSize = imageSize,
        attachments = attachments,
        onImagePress = onImageClick,
        codeHighlighter = look.highlighter,
        chipIcon = { name, size, color -> ChipIcon(name, size, color) },
        attachmentView = { a, w, h -> AttachmentBlock(a, w, h, dims, look, onLink) },
        image = { src, alt ->
            // #VAPP-258: the box's corners are the theme's (`Markdown/image`);
            // a picture the synced rows cannot size reports its decoded size
            // (genUI's media cache), so its box takes the real aspect.
            AsyncImage(
                model = src,
                contentDescription = alt,
                contentScale = ContentScale.Fit,
                alignment = Alignment.TopStart,
                onState = { state ->
                    val size = (state as? AsyncImagePainter.State.Success)?.painter?.intrinsicSize
                    if (size != null && size.isSpecified) MarkdownImageSizes.remember(src, size.width, size.height)
                },
                modifier = Modifier.fillMaxSize(),
            )
        },
        options = MarkdownOptions(hardBreaks = softBreaksAsNewlines, chat = chat, selectable = true, streaming = streaming, imageMaxHeight = imageMaxHeight),
        onLayout = onLayout,
    )
}

/** A chat's block image height cap (dp); documents have none. */
const val CHAT_IMAGE_MAX_HEIGHT = 240f

/** #EXP-1493: a host that opens a picture on a press (the plan page's
 *  lightbox) provides it; null = the picture is inert. */
val LocalMarkdownImageClick = compositionLocalOf<((url: String) -> Unit)?> { null }

/**
 * How a screen opens an attachment link the renderer shows as a file row
 * (EXP-824), typically download-to-cache + hand to the system. Null (the
 * default) leaves the link to the platform URI handler.
 */
val LocalAttachmentLinkOpener = compositionLocalOf<((String) -> Unit)?> { null }

/** A chip's icon: a concept / registry name, or a `status|<glyph>|<argb>` issue status. */
@Composable
private fun ChipIcon(name: String, size: Float, color: Color) {
    val status = AppMarkdown.statusIcon(name)
    val vector = if (status != null) ExpIcons.byName(status.first) else entityConceptIcon(name) ?: ExpIcons.byName(name)
    if (vector == null) return
    Icon(vector, contentDescription = null, tint = status?.second ?: color, modifier = Modifier.size(size.dp))
}

/** catalog/host.json `attachment`: the app's players for video / audio, a file row otherwise. */
@Composable
private fun AttachmentBlock(
    a: MarkdownBlockKind.Attachment,
    w: Float,
    h: Float,
    dims: AttachmentDims,
    look: AppMarkdown.Look,
    onLink: (String) -> Unit,
) {
    Box(Modifier.size(w.dp, h.dp)) {
        when (a.type) {
            "video" -> VideoBlockView(url = a.href, label = a.name, info = dims.infoOf(a.href))
            "audio" -> AudioBlockView(url = a.href, label = a.name, info = dims.infoOf(a.href))
            else -> Row(
                modifier = Modifier
                    .fillMaxSize()
                    .clip(RoundedCornerShape(look.codeRadius.dp))
                    .clickable { onLink(a.href) }
                    .padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Icon(ExpIcons.uiFile, contentDescription = null, tint = look.colors.muted, modifier = Modifier.size(16.dp))
                Text(
                    a.name,
                    style = TextStyle(color = look.colors.link, fontSize = look.styles.body.size.sp),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** The pure half of [MarkdownView]: the look and the host hooks. */
object AppMarkdown {
    /** The resolved look of one density. */
    class Look(
        val styles: MarkdownStyles,
        val colors: MarkdownColors,
        val codeRadius: Float,
        val highlighter: MarkdownCodeHighlighter?,
    )

    private val MODE = Mode.Dark

    /** The app's theme (the catalog's `exponential`); null where the core cannot load (JVM tests). */
    private val theme: ThemeHandle? by lazy { ThemeHandle.builtin(APP_THEME_ID) }

    private val looks = HashMap<Pair<Boolean, Float>, Look>()

    /** The look of `density: chat` or `document` at the system [fontScale] (the type scales, the painter stays at fontScale 1). */
    fun look(chat: Boolean, fontScale: Float = 1f): Look = synchronized(looks) {
        looks.getOrPut(chat to fontScale) { resolve(chat, fontScale) }
    }

    private fun resolve(chat: Boolean, fontScale: Float): Look {
        val t = theme
        val fallback = MarkdownColors.from(AppPrimitiveTokens)
        if (t == null) {
            val body = MarkdownTextSpec(size = 14f * fontScale, lineHeight = (if (chat) 22f else 20f) * fontScale, weight = 400, family = null)
            return Look(MarkdownStyles(body), fallback, AppPrimitiveTokens.radiusMd.value, null)
        }
        val props = MarkdownPainter.props(emptyMap(), chat)
        val root = t.part("Markdown", "root", emptyMap(), mode = MODE)
        val body = ResolvedTextStyle(
            fontSize = (root.px("fontSize") ?: 14f) * fontScale,
            fontWeight = root.props["fontWeight"]?.number?.toInt() ?: 400,
            lineHeight = (root.px("lineHeight") ?: 20f) * fontScale,
            fontFamily = root.fontFamily,
        )
        val styles = MarkdownPainter.styles(t, MODE, body, props, fontScale)
        fun part(owner: String, name: String, p: Map<String, JsonValue> = props, states: List<String> = emptyList()) =
            t.part(owner, name, p, states, MODE)
        val ink = part("Markdown", "root").color ?: t.color("foreground", MODE) ?: fallback.ink
        val muted = t.color("mutedForeground", MODE) ?: fallback.muted
        val border = t.color("border", MODE) ?: fallback.border
        val taskOff = part("Checkbox", "box", mapOf("checked" to JsonValue.Bool(false)))
        val taskOn = part("Checkbox", "box", mapOf("checked" to JsonValue.Bool(true)), listOf("checked"))
        val chipRoot = part("Chip", "root", MarkdownPainter.CHIP_PROPS)
        val colors = MarkdownColors(
            ink = ink,
            muted = muted,
            link = part("Markdown", "link").color ?: t.color("primary", MODE) ?: fallback.link,
            border = border,
            codeBackground = part("Markdown", "code").style.background,
            codeBlockBackground = part("Markdown", "codeBlock").style.background ?: t.color("muted", MODE) ?: fallback.codeBlockBackground,
            quoteBorder = part("Markdown", "quote").style.borderColor ?: border,
            taskFill = taskOff.style.background ?: Color.Transparent,
            taskBorder = taskOff.style.borderColor ?: border,
            taskChecked = taskOn.style.background ?: t.color("primary", MODE) ?: ink,
            taskCheck = part("Checkbox", "check", mapOf("checked" to JsonValue.Bool(true)), listOf("checked")).color
                ?: t.color("primaryForeground", MODE) ?: Color.White,
            chipBackground = chipRoot.style.background ?: Color.Transparent,
            chipBorder = chipRoot.style.borderColor ?: border,
            chipBorderWidth = chipRoot.style.borderWidth,
            chipInk = part("Chip", "label", MarkdownPainter.CHIP_PROPS).color ?: ink,
            imageBorder = part("Markdown", "image").style.borderColor ?: Color.Unspecified,
            imageBorderWidth = part("Markdown", "image").style.borderWidth,
        )
        val highlighter = MarkdownPainter.codeHighlighter(t, MODE)
        return Look(styles, colors, t.radius("md").takeIf { it > 0f } ?: 6f, highlighter)
    }

    /** An entity chip spliced out of the source (prose-links.json). */
    data class Entity(val label: String, val href: String, val kind: ProseLinks.ChipKind)

    /**
     * The text handed to the painter: every rewritten in-app link
     * (`[title](url#exp-chip-kind)`) replaced by a private-use token the text
     * links answer as its entity chip (a markdown link's label never asks the
     * host), and with [autolink] every bare `http(s)://` url outside code
     * written as a link. [labels] = each link's label by destination (an
     * attachment's name).
     */
    class Source(val text: String, val entities: List<Entity>, val labels: Map<String, String>)

    private const val TOKEN_OPEN = ''
    private const val TOKEN_CLOSE = ''
    private val LINK = Regex("""(?<!!)\[((?:\\.|[^\\\]])*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)""")
    private val TOKEN = Regex("$TOKEN_OPEN(\\d+)$TOKEN_CLOSE")
    private const val URL_STOPS = " \t<>()[]`\"'"
    private const val URL_TRAILING = ".,;:!?*_~"

    fun prepare(markdown: String, autolink: Boolean): Source {
        val labels = HashMap<String, String>()
        val entities = ArrayList<Entity>()
        val hasChips = markdown.contains("#exp-chip-")
        val bareUrls = autolink && (markdown.contains("http://", true) || markdown.contains("https://", true))
        for (m in LINK.findAll(markdown)) labels.putIfAbsent(m.groupValues[2], unescape(m.groupValues[1]))
        if (!hasChips && !bareUrls) return Source(markdown, entities, labels)
        var fence: String? = null
        val text = markdown.split("\n").joinToString("\n") { line ->
            val mark = fenceMark(line)
            when {
                mark != null -> {
                    if (fence == null) fence = mark else if (fence == mark) fence = null
                    line
                }
                fence != null -> line
                else -> rewriteLine(line, hasChips, bareUrls, entities)
            }
        }
        return Source(text, entities, labels)
    }

    private fun fenceMark(line: String): String? {
        val rest = line.trimStart(' ')
        if (line.length - rest.length > 3) return null
        return if (rest.startsWith("```") || rest.startsWith("~~~")) rest.substring(0, 3) else null
    }

    /** One line outside a fence: its code spans kept, chip links tokenized, bare urls linked. */
    private fun rewriteLine(line: String, chips: Boolean, urls: Boolean, entities: MutableList<Entity>): String {
        val out = StringBuilder(line.length)
        var at = 0
        while (at < line.length) {
            val ch = line[at]
            if (ch == '`') {
                var run = at
                while (run < line.length && line[run] == '`') run++
                val close = line.indexOf(line.substring(at, run), run)
                val end = if (close == -1) run else close + (run - at)
                out.append(line, at, end)
                at = end
                continue
            }
            if (ch == '[' && (at == 0 || line[at - 1] != '!')) {
                val m = LINK.matchAt(line, at)
                if (m != null) {
                    val kind = if (chips) ProseLinks.chipKind(m.groupValues[2]) else null
                    if (kind != null) {
                        out.append(TOKEN_OPEN).append(entities.size).append(TOKEN_CLOSE)
                        entities.add(Entity(unescape(m.groupValues[1]), m.groupValues[2], kind))
                    } else {
                        out.append(m.value)
                    }
                    at = m.range.last + 1
                    continue
                }
            }
            if (urls && (line.startsWith("http://", at, true) || line.startsWith("https://", at, true))) {
                val prev = line.getOrNull(at - 1)
                val glued = prev != null && (prev.isLetterOrDigit() || prev == '_' || prev == '[' || prev == '<' || prev == '(')
                if (!glued) {
                    var end = at
                    while (end < line.length && line[end] !in URL_STOPS) end++
                    while (end > at && line[end - 1] in URL_TRAILING) end--
                    val url = line.substring(at, end)
                    if (url.length > "https://".length) {
                        out.append('[').append(url.replace("[", "\\[").replace("]", "\\]")).append("](").append(url).append(')')
                        at = end
                        continue
                    }
                }
            }
            out.append(ch)
            at++
        }
        return out.toString()
    }

    private fun unescape(label: String): String {
        if (!label.contains('\\')) return label
        val out = StringBuilder(label.length)
        var i = 0
        while (i < label.length) {
            val c = label[i]
            if (c == '\\' && i + 1 < label.length) {
                out.append(label[i + 1])
                i += 2
            } else {
                out.append(c)
                i++
            }
        }
        return out.toString()
    }

    private const val ISSUE_SCHEME = "exp-issue:"
    private const val MENTION_SCHEME = "exp-mention:"
    private const val STATUS_ICON = "status|"

    /**
     * catalog/host.json `textLinks` with chips over one text piece: the
     * entity tokens, the `#IDENT` refs that resolve (an issue chip: the
     * identifier + title, the status glyph) and the known `@email`
     * mentions (the member's name); null = nothing to link.
     */
    fun segments(
        piece: String,
        source: Source,
        issueRefs: IssueRefHandler?,
        mentions: MentionResolver?,
        bare: Boolean,
    ): List<MarkdownTextLink>? {
        class Hit(val start: Int, val end: Int, val link: MarkdownTextLink)
        val hits = ArrayList<Hit>()
        if (source.entities.isNotEmpty() && piece.contains(TOKEN_OPEN)) {
            for (m in TOKEN.findAll(piece)) {
                val e = source.entities.getOrNull(m.groupValues[1].toInt()) ?: continue
                hits.add(Hit(m.range.first, m.range.last + 1, MarkdownTextLink(e.label, e.href, MarkdownChip("entity", e.label, e.kind.icon))))
            }
        }
        if (issueRefs != null) {
            for (match in IssueRefs.findAll(piece, bare)) {
                val target = issueRefs.resolve(match.identifier) ?: continue
                val title = chipTitle(target.title)
                val label = if (title.isEmpty()) target.identifier else "${target.identifier} $title"
                val icon = target.resolvedStatus?.let { "$STATUS_ICON${it.iconName}|${resolvedStatusColor(it).toArgb()}" }
                hits.add(
                    Hit(
                        match.start,
                        match.end,
                        MarkdownTextLink(piece.substring(match.start, match.end), ISSUE_SCHEME + target.identifier, MarkdownChip("issue", label, icon)),
                    ),
                )
            }
        }
        if (mentions != null) {
            for (match in Mentions.findAll(piece)) {
                val member = mentions.resolve(match.email) ?: continue
                hits.add(
                    Hit(
                        match.start,
                        match.end,
                        MarkdownTextLink(piece.substring(match.start, match.end), MENTION_SCHEME + match.email, MarkdownChip("mention", member.name.trim().ifEmpty { match.email })),
                    ),
                )
            }
        }
        if (hits.isEmpty()) return null
        hits.sortBy { it.start }
        val out = ArrayList<MarkdownTextLink>()
        var at = 0
        for (hit in hits) {
            if (hit.start < at) continue
            if (hit.start > at) out.add(MarkdownTextLink(piece.substring(at, hit.start)))
            out.add(hit.link)
            at = hit.end
        }
        if (at < piece.length) out.add(MarkdownTextLink(piece.substring(at)))
        return out
    }

    /** `status|<glyph>|<argb>` → the glyph and its colour; null = another icon. */
    fun statusIcon(name: String): Pair<String, Color>? {
        if (!name.startsWith(STATUS_ICON)) return null
        val parts = name.split('|')
        if (parts.size != 3) return null
        return parts[1] to Color(parts[2].toIntOrNull() ?: return null)
    }

    /**
     * The URL policy: `~/` links mean nothing without an app-link handler,
     * an ignored app link (app-link.json `ignore`) is text; attachments and
     * everything else stay as written.
     */
    fun police(href: String, appLinks: AppLinkHandler?): String? {
        if (attachmentIdFromUrl(href) != null) return href
        if (appLinks == null) return if (href.trim().startsWith("~")) null else href
        return if (classifyAppLink(href, appLinks.origin, appLinks.team) == AppLink.Ignore) null else href
    }

    /** A pressed link or chip: an issue chip opens the issue, a mention is inert, an attachment opens through [opener], the rest through the app-link rule. */
    fun open(
        href: String,
        issueRefs: IssueRefHandler?,
        appLinks: AppLinkHandler?,
        opener: ((String) -> Unit)?,
        system: (String) -> Unit,
    ) {
        when {
            href.startsWith(ISSUE_SCHEME) -> {
                val refs = issueRefs?.takeIf { it.canOpen } ?: return
                refs.resolve(href.removePrefix(ISSUE_SCHEME))?.let(refs.onOpen)
            }
            href.startsWith(MENTION_SCHEME) -> Unit
            attachmentIdFromUrl(href) != null && opener != null -> opener(href)
            href.startsWith("mailto:", ignoreCase = true) -> system(href)
            appLinks != null -> appLinks.openHref(href)
            else -> system(href)
        }
    }

    /** catalog/host.json `imageSize`: an attachment's probed size. */
    fun imageSize(src: String, dims: AttachmentDims): MarkdownSize? {
        val info = dims.infoOf(src) ?: return null
        val w = info.width ?: return null
        val h = info.height ?: return null
        return MarkdownSize(w.toFloat(), h.toFloat()).takeIf { it.valid }
    }

    /** catalog/host.json `attachment`: a synced attachment row's kind, named by its link's label. */
    fun attachment(href: String, dims: AttachmentDims, source: Source): MarkdownAttachment? {
        val info = dims.infoOf(href) ?: return null
        val type = when {
            info.isVideo -> "video"
            info.isAudio -> "audio"
            else -> "file"
        }
        val name = source.labels[href]?.takeIf { it.isNotBlank() } ?: href.substringAfterLast('/')
        return MarkdownAttachment(type, name)
    }
}
