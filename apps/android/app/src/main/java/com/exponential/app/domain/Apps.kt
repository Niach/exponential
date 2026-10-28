package com.exponential.app.domain

import java.text.Normalizer

// VAPP-110: Exponential apps, the Android mirror of the pure reference
// `packages/domain-contract/src/apps.ts` (fixtures/apps.json is the spec;
// `AppsTest` replays its `slugifyCases`, `versionLabelCases` and
// `sortAppsCases` and locks the copy below to the fixture byte for byte).
object Apps {
    object Copy {
        const val NAV = "Apps"
        const val TITLE = "Apps"
        const val NEW_APP = "New app"
        const val VERSION = "v{n}"
        const val NO_VERSION = "No version yet"
        const val VERSIONS = "Versions"
        const val HEAD = "Current"
        const val NO_NOTE = "No note"
        const val BY_AGENT = "the editor session"
        const val VIEWING = "Viewing v{n}"
        const val RESTORE = "Restore"
        const val RESTORED = "Restored v{n} as v{m}"
        const val RESTORED_NOTE = "Restored v{n}"
        const val EMPTY = "No apps yet"
        const val CHAT_EMPTY = "Describe the app you want. The editor session builds it and the preview follows."
        const val PREVIEW_EMPTY = "No version yet"
        const val COMPOSER_PLACEHOLDER = "Ask for a change…"
        const val NO_DEVICE = "Start a device to edit this app"
        const val NAME_PLACEHOLDER = "Name"
        const val DESCRIPTION_PLACEHOLDER = "What is it for? (optional)"
        const val CREATE = "Create app"
        const val RENAME = "Save"
        const val DELETE_TITLE = "Delete {name}?"
        const val DELETE_BODY = "Its versions go with it. This cannot be undone."
        const val DELETE = "Delete"
        const val MENU_RENAME = "Rename"
        const val MENU_DELETE = "Delete app"
        const val TAB_CHAT = "Chat"
        const val TAB_PREVIEW = "Preview"
        const val TAB_VERSIONS = "Versions"
        // VAPP-122: the app's records count (Settings → Apps → the app).
        const val RECORDS = "{n} records"
        const val RECORDS_ONE = "1 record"
        const val RECORDS_OF = "{n} of {max} records"
        const val RECORDS_NONE = "No records"
    }

    object Preview {
        const val RADIUS = 12
        const val PADDING = 16
        const val SURFACE_ID = "app"
    }

    object TestIds {
        const val LIST = "apps-list"
        const val ROW = "apps-row"
        const val NEW_APP = "apps-new"
        const val EDITOR = "app-editor"
        const val CHAT = "app-editor-chat"
        const val PREVIEW = "app-editor-preview"
        const val VERSIONS = "app-editor-versions"
        const val VERSION_ROW = "app-version-row"
        const val TABS = "app-editor-tabs"
        const val COMPOSER = "app-editor-composer"
        const val VIEWING = "app-editor-viewing"
        // VAPP-129: the floating layout (web and the IDE).
        const val CHAT_CARD = "app-editor-chat-card"
        const val DOCK_TO_SIDE = "app-editor-dock-side"
        const val FLOAT = "app-editor-float"
    }

    // Contract `apps` (packages/domain-contract/contract.json), locked by AppsTest.
    const val SLUG_MAX = 64
    const val NAME_MAX = 64
    const val DESCRIPTION_MAX = 2000
    const val NOTE_MAX = 500
    // VAPP-122 records: a kind's name, one record's `data` JSON, kinds per manifest.
    const val KIND_PATTERN = "^[a-z][a-z0-9_]{0,31}$"
    const val RECORD_MAX_BYTES = 16384
    const val RECORD_KINDS_MAX = 16

    /** `v{n}`, or the "no version" copy while an app has no head. */
    fun versionLabel(headVersion: Int): String =
        if (headVersion > 0) Copy.VERSION.replace("{n}", headVersion.toString()) else Copy.NO_VERSION

    fun viewing(number: Int): String = Copy.VIEWING.replace("{n}", number.toString())

    fun restored(restored: Int, version: Int): String =
        Copy.RESTORED.replace("{n}", restored.toString()).replace("{m}", version.toString())

    fun deleteTitle(name: String): String = Copy.DELETE_TITLE.replace("{name}", name)

    private val COMBINING = Regex("[\\u0300-\\u036f]")
    private val NON_SLUG = Regex("[^a-z0-9]+")
    private val EDGE_DASHES = Regex("^-+|-+$")
    private val TRAILING_DASHES = Regex("-+$")

    /** A name → its slug: lowercase ASCII letters, digits and single dashes,
     *  diacritics stripped, cut at the cap, `app` when nothing is left; a taken
     *  slug gets `-2`, `-3`, … */
    fun slugify(name: String, taken: Collection<String> = emptyList()): String {
        val base = Normalizer.normalize(name, Normalizer.Form.NFKD)
            .replace(COMBINING, "")
            .lowercase()
            .replace(NON_SLUG, "-")
            .replace(EDGE_DASHES, "")
            .take(SLUG_MAX)
            .replace(TRAILING_DASHES, "")
            .ifEmpty { "app" }
        if (base !in taken) return base
        var n = 2
        while (true) {
            val suffix = "-$n"
            val candidate = base.take(SLUG_MAX - suffix.length).replace(TRAILING_DASHES, "") + suffix
            if (candidate !in taken) return candidate
            n++
        }
    }

    /** What [sortApps] orders by. */
    interface ListLike {
        val slug: String
        val name: String
    }

    /** The list order: by name (case-insensitive). Exponential ships no built-in apps. */
    fun <T : ListLike> sortApps(apps: List<T>): List<T> =
        apps.sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.name })

    /** The fixture's `mediaPolicy` (`AppsTest` locks the copy). */
    object MediaPolicy {
        val DATA_MIME_PREFIXES = listOf("image/", "audio/", "video/")
        val DENIED_DATA_MIMES = listOf("image/svg+xml")
        const val ATTACHMENT_PATH = "/api/attachments/"
    }

    private val ATTACHMENT_PATH_RE = Regex(
        "${Regex.escape(MediaPolicy.ATTACHMENT_PATH)}[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
        RegexOption.IGNORE_CASE,
    )

    /** VAPP-157: the app hosts' media rule (fixture `mediaPolicy`), a plain
     *  string rule so the four hosts agree byte for byte: a `data:` image,
     *  audio or video (never svg), or the instance's own
     *  `/api/attachments/<uuid>` (relative or on [origin] exactly) with
     *  nothing after the id. */
    fun appMediaAllowed(src: String, origin: String): Boolean {
        if (src.isEmpty() || src.trim() != src) return false
        val lower = src.lowercase()
        if (lower.startsWith("data:")) {
            val comma = lower.indexOf(',')
            if (comma < 0) return false
            val mime = lower.substring(5, comma).split(';')[0]
            return MediaPolicy.DATA_MIME_PREFIXES.any { mime.startsWith(it) && mime.length > it.length } &&
                mime !in MediaPolicy.DENIED_DATA_MIMES
        }
        val path = if (src.startsWith("/") && !src.startsWith("//")) {
            src
        } else {
            val base = origin.trimEnd('/').lowercase()
            if (base.isEmpty() || !lower.startsWith("$base/")) return false
            src.substring(base.length)
        }
        return ATTACHMENT_PATH_RE.matches(path)
    }
}
