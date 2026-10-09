package at.exponential.ui.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import at.exponential.ui.ffi.Theme
import at.exponential.ui.ffi.UiException
import at.exponential.ui.ffi.defaultThemeId
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.json
import at.exponential.ui.primitives.PrimitiveTokens
import at.exponential.ui.primitives.parseHexColor

/** `light` | `dark`. */
enum class Mode(val wire: String) {
    Light("light"), Dark("dark");

    companion object {
        /** The mode named `wire` (null when unknown). */
        fun of(wire: String): Mode? = entries.firstOrNull { it.wire == wire }
    }
}

/**
 * The theme the painter queries: the facade's [Theme] object plus caches
 * (part looks keyed on their query, colours per mode). Painters never read
 * recipes themselves: [part] resolves a sub-part the core does not
 * synthesize exactly like the core resolves a synthetic one.
 */
/** One reason a theme was refused (`path` into the theme file). */
data class ThemeIssue(val path: String, val message: String)

class ThemeHandle(val theme: Theme) {
    /** The theme id. */
    val id: String = theme.id()
    private val parts = HashMap<String, PartStyle>()
    private val colors = HashMap<String, Color?>()
    private val rawColors = HashMap<String, String?>()
    private val lock = Any()

    companion object {
        /** A built-in theme by id (null when unknown). */
        fun builtin(id: String): ThemeHandle? = runCatching { Theme.builtin(id) }.getOrNull()?.let(::ThemeHandle)

        /** Load a theme file over the built-ins (`parents` = theme JSON documents it may extend). Throws on an invalid theme. */
        fun load(json: String, parents: List<String> = emptyList()): ThemeHandle =
            ThemeHandle(Theme.load(json, if (parents.isEmpty()) null else "[${parents.joinToString(",")}]"))

        /**
         * Round 4 (VAPP-103): a theme file that NEVER fails the host. An
         * unusable one (a bad `$schema`, an unknown token, a missing value…)
         * falls back to the default built-in and [onIssues] gets why, as the
         * React surface's `onThemeIssues` and the TS host's `onIssue` do.
         */
        fun loadOrDefault(json: String, parents: List<String> = emptyList(), onIssues: ((List<ThemeIssue>) -> Unit)? = null): ThemeHandle =
            try {
                load(json, parents)
            } catch (e: UiException.Theme) {
                onIssues?.invoke(
                    (JsonValue.parse(e.issuesJson).array ?: emptyList()).map { ThemeIssue(it["path"]?.string ?: "", it["message"]?.string ?: "") },
                )
                builtin(defaultThemeId())!!
            } catch (e: UiException) {
                onIssues?.invoke(listOf(ThemeIssue("theme", e.message ?: "invalid")))
                builtin(defaultThemeId())!!
            }
    }

    /** `owner/part` resolved for the OWNER's props and `states` in `mode`. */
    fun part(owner: String, part: String, props: Props, states: List<String> = emptyList(), mode: Mode): PartStyle {
        val propsJson = props.json
        val key = "$owner/$part|${mode.wire}|${states.sorted().joinToString(",")}|$propsJson"
        synchronized(lock) { parts[key] }?.let { return it }
        val resolved = try {
            val p = theme.resolvePart(owner, part, propsJson, states, mode.wire)
            PartStyle(PaintStyle(p.visual), JsonValue.parse(p.styleJson).obj ?: emptyMap())
        } catch (_: Exception) {
            PartStyle.EMPTY
        }
        synchronized(lock) { parts[key] = resolved }
        return resolved
    }

    /** A colour token for the mode (`foreground`, `primary`, `chart1`…). */
    fun color(name: String, mode: Mode): Color? {
        val key = "$name|${mode.wire}"
        synchronized(lock) { if (colors.containsKey(key)) return colors[key] }
        val hex = theme.color(name, mode.wire)
        val c = hex?.let(::parseHexColor)
        synchronized(lock) {
            colors[key] = c
            rawColors[key] = hex
        }
        return c
    }

    /** A colour token as the theme's hex string. */
    fun colorHex(name: String, mode: Mode): String? {
        color(name, mode)
        return synchronized(lock) { rawColors["$name|${mode.wire}"] }
    }

    /** A spacing token (dp; 0 when missing). */
    fun spacing(name: String): Float = (theme.spacing(name) ?: 0.0).toFloat()

    /** A radius token (dp; 0 when missing). */
    fun radius(name: String): Float = (theme.radius(name) ?: 0.0).toFloat()

    /** A control size token (dp), else `fallback`. */
    fun control(name: String, fallback: Float): Float = theme.control(name)?.toFloat() ?: fallback

    /** A font family NAME by kind (`sans`, `mono`). */
    fun fontFamily(kind: String): String? = theme.fontFamily(kind)

    /** The sans family NAME. */
    val sansFamily: String? get() = fontFamily("sans")

    /** The mono family NAME. */
    val monoFamily: String? get() = fontFamily("mono")

    /** The surface's default text colour. */
    fun ink(mode: Mode): Color =
        color("foreground", mode) ?: if (mode == Mode.Dark) Color(0xFFFAFAFA) else Color(0xFF0A0A0A)

    /** The tokens the generic primitives paint with under this theme. */
    fun primitiveTokens(mode: Mode): PrimitiveTokens {
        val d = PrimitiveTokens()
        return PrimitiveTokens(
            foreground = ink(mode),
            mutedForeground = color("mutedForeground", mode) ?: d.mutedForeground,
            background = color("background", mode) ?: Color.Transparent,
            card = color("card", mode) ?: Color.Transparent,
            muted = color("muted", mode) ?: d.muted,
            border = color("border", mode) ?: d.border,
            input = color("input", mode) ?: d.input,
            primary = color("primary", mode) ?: d.primary,
            primaryForeground = color("primaryForeground", mode) ?: d.primaryForeground,
            accent = color("accent", mode) ?: d.accent,
            destructive = color("destructive", mode) ?: d.destructive,
            success = color("success", mode) ?: d.success,
            warning = color("warning", mode) ?: d.warning,
            info = color("info", mode) ?: d.info,
            ring = color("ring", mode) ?: d.ring,
            hairline = control("hairline", 1f).dp,
            pillHeight = control("pill", 24f).dp,
            inputHeight = control("input", 36f).dp,
            rowHeight = control("row", 32f).dp,
            radiusSm = radius("sm").dp,
            radiusMd = radius("md").dp,
            radiusLg = radius("lg").dp,
            sansFamily = sansFamily,
            monoFamily = monoFamily,
        )
    }
}

/** Spacing fallbacks in geometry mode (no theme), the core's table. */
object GeometrySpacing {
    /** The dp value of a spacing token name. */
    fun value(name: String): Float = when (name) {
        "xxs" -> 2f
        "xs" -> 4f
        "sm" -> 8f
        "md" -> 12f
        "lg" -> 16f
        "xl" -> 24f
        else -> 0f
    }
}
