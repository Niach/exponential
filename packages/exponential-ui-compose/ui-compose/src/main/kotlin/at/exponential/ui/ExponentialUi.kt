package at.exponential.ui

import android.content.Context
import androidx.compose.ui.text.font.FontFamily
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.extension.ExtensionRegistry
import at.exponential.ui.ffi.basicCatalogId
import at.exponential.ui.ffi.builtinThemeIds
import at.exponential.ui.ffi.coreCatalogId
import at.exponential.ui.ffi.defaultThemeId
import at.exponential.ui.ffi.extensionErrors
import at.exponential.ui.ffi.version
import at.exponential.ui.measure.INTER_FAMILY
import at.exponential.ui.measure.InterFont

/** The SDK's entry points. */
object ExponentialUi {
    private val fonts = HashMap<String, FontFamily>()

    /** The application context `HostPlugin.openUrl`'s default opens links with (optional). */
    @Volatile
    var appContext: Context? = null

    /**
     * Register an extension catalog (its JSON definition) and the painters of
     * its natives. Every `SurfaceModel` created afterwards knows it;
     * `SurfaceModel.register` adds one to a live surface. Throws
     * [IllegalArgumentException] when the definition is invalid.
     */
    fun register(extensionJson: String, painters: Map<String, ExtensionPainter>) {
        val errors = try {
            extensionErrors(extensionJson)
        } catch (e: Exception) {
            throw IllegalArgumentException("extension: ${e.message}", e)
        }
        require(errors.isEmpty()) { "extension: ${errors.joinToString("; ")}" }
        ExtensionRegistry.shared.register(extensionJson)
        for ((kind, painter) in painters) ExtensionRegistry.shared.register(kind, painter)
    }

    /** Register the Compose family a theme names (`Inter`); a missing family falls back to the default font. */
    fun registerFont(family: String, font: FontFamily) {
        synchronized(fonts) { fonts[family] = font }
    }

    /** The family registered under `name`; `Inter` unregistered = the bundled faces (#EXP-2215). */
    fun fontFamily(name: String): FontFamily? = synchronized(fonts) { fonts[name] } ?: if (name == INTER_FAMILY) InterFont.bundled else null

    /**
     * #EXP-2215: the bundled Inter (Regular, Medium, SemiBold, Bold; SIL OFL
     * 1.1): the face genUI's `exponential` theme draws in, for the host's own
     * typography too, so an app never bundles a second copy.
     */
    val inter: FontFamily get() = InterFont.bundled

    /** The built-in theme ids. */
    val builtinThemes: List<String> get() = builtinThemeIds()

    /** The default built-in theme id. */
    val defaultTheme: String get() = defaultThemeId()

    /** The core catalog id. */
    val coreCatalog: String get() = coreCatalogId()

    /** The A2UI basic catalog id. */
    val basicCatalog: String get() = basicCatalogId()

    /** The core's version. */
    val coreVersion: String get() = version()
}
