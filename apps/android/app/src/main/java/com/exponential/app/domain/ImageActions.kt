package com.exponential.app.domain

import at.exponential.ui.primitives.ImageSource

/**
 * #EXP-1926 / #EXP-1948 (fixtures/image-actions.json, TS
 * `src/image-actions.ts`): "Copy image" + Share on every content picture,
 * and which picture sources load at all. `ImageActionsTest` replays
 * `menuCases`, `pressCases`, `encodingCases`, `shareNameCases` and
 * `sourceCases`.
 */
object ImageActions {
    object Copy {
        const val COPY_IMAGE = DomainContract.imageActionsCopyImage
        const val SHARE = DomainContract.imageActionsShare
        const val COPIED = DomainContract.imageActionsCopied
        const val COPY_FAILED = DomainContract.imageActionsCopyFailed
        const val SHARE_FAILED = DomainContract.imageActionsShareFailed
        const val LINK_HOST = DomainContract.imageActionsLinkHost
    }

    object Icons {
        const val COPY_IMAGE = "ui-copy"
        const val SHARE = "ui-share"
    }

    object TestIds {
        const val COPY_IMAGE = "imageCopy"
        const val SHARE = "imageShare"
    }

    enum class Input { Pointer, Touch }
    enum class Item { CopyImage, Share }
    enum class PressMenu { Image, Owner, None }
    enum class Encoding { Passthrough, Reencode }

    /** The menu's items in order: pointer = Copy image; touch = Copy image then Share (only when a file can be shared). */
    fun imageMenu(input: Input, canShareFile: Boolean): List<Item> = when (input) {
        Input.Pointer -> listOf(Item.CopyImage)
        Input.Touch -> if (canShareFile) listOf(Item.CopyImage, Item.Share) else listOf(Item.CopyImage)
    }

    /** Which menu a press opens: the image's own on a rendered picture, else the element's around it, else none. Never merged. */
    fun pressMenu(onImage: Boolean, imageReady: Boolean, ownerMenu: String?): PressMenu = when {
        onImage && imageReady -> PressMenu.Image
        ownerMenu != null -> PressMenu.Owner
        else -> PressMenu.None
    }

    private val PNG_SIGNATURE = byteArrayOf(0x89.toByte(), 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a)

    /** PNG bytes go as they are; anything else is decoded and re-encoded to PNG (an animation = its first frame). */
    fun copyEncoding(head: ByteArray): Encoding =
        if (head.size >= PNG_SIGNATURE.size && PNG_SIGNATURE.indices.all { head[it] == PNG_SIGNATURE[it] }) Encoding.Passthrough else Encoding.Reencode

    /** The shared PNG's file name: the last extension becomes `.png`, path separators `-`, an empty stem (or no name) = `image.png`. */
    fun shareFileName(name: String?): String {
        val clean = (name ?: "").trim().replace('/', '-').replace('\\', '-')
        val dot = clean.lastIndexOf('.')
        val stem = (if (dot > 0) clean.substring(0, dot) else clean).trim()
        return "${stem.ifEmpty { "image" }}.png"
    }

    private const val ATTACHMENT_PATH = "/api/attachments/"
    private val ATTACHMENT_RE = Regex(
        "^${Regex.escape(ATTACHMENT_PATH)}[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}(?:\\?[^#]*)?(?:#.*)?$",
    )
    private val LOCAL_SCHEMES = listOf("blob:", "app-file:")
    private val LINK_SCHEMES = listOf("https:", "http:")
    private const val EDGE_SPACE = " \t\n\u000c\r"

    private fun trimAscii(s: String): String = s.trim { it in EDGE_SPACE }

    private fun isAttachmentPath(path: String): Boolean = ATTACHMENT_RE.matches(path)

    private fun isLoadableData(lower: String): Boolean {
        val comma = lower.indexOf(',')
        if (comma < 0) return false
        val mime = lower.substring(5, comma).split(';')[0]
        return mime.startsWith("image/") && mime.length > "image/".length && mime != "image/svg+xml"
    }

    /** The label host of a `scheme://` or `//` url: lowercased, userinfo and port dropped, IPv6 brackets kept; empty = none. */
    private fun linkHost(afterSlashes: String): String {
        val end = afterSlashes.indexOfFirst { it == '/' || it == '?' || it == '#' || it == '\\' }
        var authority = if (end < 0) afterSlashes else afterSlashes.substring(0, end)
        val at = authority.lastIndexOf('@')
        if (at >= 0) authority = authority.substring(at + 1)
        val close = authority.lastIndexOf(']')
        val colon = authority.lastIndexOf(':')
        if (colon > close) authority = authority.substring(0, colon)
        return authority.lowercase()
    }

    /** What a picture's src renders as: [ImageSource.Load] (ours, data or local), a [ImageSource.Link] (another website: never fetched) or plain [ImageSource.Text] (the alt, maybe empty). */
    fun imageSource(src: String, alt: String, origin: String): ImageSource {
        val altText = trimAscii(alt)
        val plain = ImageSource.Text(altText)
        val s = trimAscii(src)
        if (s.isEmpty() || s.any { it.code <= 0x20 || it.code == 0x7f }) return plain
        val lower = s.lowercase()
        if (lower.startsWith("data:")) return if (isLoadableData(lower)) ImageSource.Load else plain
        if (LOCAL_SCHEMES.any { lower.startsWith(it) }) return ImageSource.Load
        if (s.startsWith("/") && !s.startsWith("//")) return if (isAttachmentPath(s)) ImageSource.Load else plain
        val base = origin.trimEnd('/').lowercase()
        if (base.isNotEmpty() && lower.startsWith("$base/") && isAttachmentPath(s.substring(base.length))) return ImageSource.Load
        val href: String
        val rest: String
        if (s.startsWith("//")) {
            href = "https:$s"
            rest = s.substring(2)
        } else {
            val scheme = LINK_SCHEMES.firstOrNull { lower.startsWith("$it//") } ?: return plain
            href = s
            rest = s.substring(scheme.length + 2)
        }
        val host = linkHost(rest)
        if (host.isEmpty()) return plain
        return ImageSource.Link(href, altText.ifEmpty { Copy.LINK_HOST.replace("{host}", host) })
    }

    /** True when the picture may be fetched at all. */
    fun imageSourceAllowed(src: String, origin: String): Boolean = imageSource(src, "", origin) == ImageSource.Load

    private val UUID_RE = Regex("[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")

    /** The attachment id of an `/api/attachments/<uuid>` src (relative or on [origin]); null = not ours. */
    fun attachmentId(src: String, origin: String): String? {
        val s = trimAscii(src)
        val path = when {
            s.startsWith("/") && !s.startsWith("//") -> s
            origin.isNotEmpty() && s.lowercase().startsWith("${origin.trimEnd('/').lowercase()}/") -> s.substring(origin.trimEnd('/').length)
            else -> return null
        }
        if (!isAttachmentPath(path)) return null
        return UUID_RE.find(path.substring(ATTACHMENT_PATH.length))?.value
    }
}
