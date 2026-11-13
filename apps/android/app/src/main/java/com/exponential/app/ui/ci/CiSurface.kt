package com.exponential.app.ui.ci

import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import com.exponential.app.ui.icons.ExpIcons
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

// #EXP-1804 (web `components/ci/ci-surface.tsx`): a genUI tree the app
// rebuilds from live data (the CI run page, the CI runner section), painted by
// the core catalog like every other surface ×4. [tree] replaces the painted
// tree whenever it changes; [data] seeds the data model (bound fields);
// [onAction] gets every event (name, context, the component's payload).

/** The surface's data model, for a host writing back into it (a ChipInput's
 *  bound `values`). */
class CiSurfaceHandle {
    internal var model: SurfaceModel? = null

    fun setData(path: String, value: JsonElement) {
        model?.setData(path, JsonValue.parse(value.toString()))
    }
}

/** THE icon registry the painter reads: a concept (`ci-failed`) or a name. */
private class CiSurfaceHost(private val onActionEvent: (SurfaceActionEvent) -> Unit) : HostPlugin {
    override fun icon(name: String, size: Float): (@Composable () -> Unit)? {
        val glyph = ExpIcons.byConcept(name) ?: ExpIcons.byName(name) ?: return null
        return { Icon(glyph, contentDescription = null, modifier = Modifier.size(size.dp)) }
    }

    override fun onAction(event: SurfaceActionEvent) = onActionEvent(event)
}

internal fun JsonValue?.toJsonObject(): JsonObject? =
    this?.let { runCatching { Json.parseToJsonElement(it.json) as? JsonObject }.getOrNull() }

@Composable
fun CiSurface(
    surfaceId: String,
    tree: JsonObject,
    modifier: Modifier = Modifier,
    data: Map<String, JsonElement> = emptyMap(),
    handle: CiSurfaceHandle? = null,
    onAction: (name: String, context: JsonObject?, payload: JsonObject?) -> Unit = { _, _, _ -> },
) {
    val latest = rememberUpdatedState(onAction)
    val model = remember(surfaceId) {
        runCatching {
            SurfaceModel(
                id = surfaceId,
                options = SurfaceOptions(),
                host = CiSurfaceHost { event -> latest.value(event.name, event.context.toJsonObject(), event.payload.toJsonObject()) },
            ) { error("set by ExponentialSurface") }.also { m ->
                m.setNested(tree.toString())
                for ((key, value) in data) m.setData("/$key", JsonValue.parse(value.toString()))
            }
        }.getOrNull()
    } ?: return
    handle?.model = model
    // A new tree (live data moved) replaces the painted one; the first is
    // the one the model was built with.
    val painted = remember(model) { arrayOf(tree) }
    SideEffect {
        if (painted[0] != tree) {
            painted[0] = tree
            model.setNested(tree.toString())
        }
    }
    ExponentialSurface(model, modifier)
}
