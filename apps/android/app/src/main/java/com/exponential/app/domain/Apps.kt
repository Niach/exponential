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
        const val BUILTIN = "Built-in"
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
    }

    /** The BUILT-IN app every client ships (the Devices package). */
    object Builtin {
        const val SLUG = "devices"
        const val NAME = "Devices"
        const val ICON = "monitor"
        const val PACKAGE_ID = "exponential.devices"
        const val TEMPLATE_ID = "devices"
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
    }

    // Contract `apps` (packages/domain-contract/contract.json), locked by AppsTest.
    const val SLUG_MAX = 64
    const val NAME_MAX = 64
    const val DESCRIPTION_MAX = 2000
    const val NOTE_MAX = 500

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
        val builtin: Boolean
    }

    /** The list order: the built-in app first, then by name (case-insensitive). */
    fun <T : ListLike> sortApps(apps: List<T>): List<T> =
        apps.sortedWith(compareBy<T> { !it.builtin }.thenBy(String.CASE_INSENSITIVE_ORDER) { it.name })
}
