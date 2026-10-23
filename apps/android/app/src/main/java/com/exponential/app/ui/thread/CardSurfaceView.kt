package com.exponential.app.ui.thread

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.host.MediaOptions
import at.exponential.ui.host.MediaRule
import at.exponential.ui.host.UrlPolicy
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.images.resolveAccountForUrl
import com.exponential.app.domain.CardSurface
import com.exponential.app.domain.PlanCard
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

/**
 * EXP-1280: the plan / question card's inline surface — an agent-authored
 * Exponential UI CORE surface, painted by the real renderer (`:ui-compose`,
 * the UniFFI core). The card host: NO transport (actions and input edits go
 * nowhere), `set` the only function (the core's built-in; host functions
 * are never registered), media = `/api/attachments/…` against the app's
 * instance WITH the account's bearer + https (its own host opted in past
 * VAPP-107's private-host rule, [cardAllowPrivate]). When the person answers, the
 * card reads [CardSurfaceHandle.snapshot]: the surface's data model, the
 * answer's `surfaceData`.
 */
class CardSurfaceHandle {
    internal var model: SurfaceModel? = null

    /** The surface's data model now (null = no surface / it failed). */
    fun snapshot(): JsonObject? = model?.let { m ->
        runCatching { Json.parseToJsonElement(m.data.json) as? JsonObject }.getOrNull()
    }
}

@EntryPoint
@InstallIn(SingletonComponent::class)
interface CardSurfaceEntryPoint {
    fun authRepository(): AuthRepository
}

/**
 * VAPP-107: private, loopback and link-local hosts are denied by default; a
 * self-hosted LAN instance (or the localhost dev stack) opts EXACTLY its own
 * API host back in, never the whole LAN (no `*`, no suffix). A public origin
 * needs nothing, and listing it changes nothing. Null = no origin.
 */
internal fun cardAllowPrivate(origin: String?): List<String>? = origin
    ?.let { runCatching { java.net.URI(it.trim()).host }.getOrNull() }
    ?.removePrefix("[")?.removeSuffix("]")
    ?.takeIf { it.isNotEmpty() }
    ?.let { listOf(it.lowercase()) }

/** The card host plugin (see [CardSurfaceHandle]). Thread 58cb32df: a
 *  [SurfaceCard] passes [onAction] (a press's event name + context) so a
 *  `reply` reaches the chat; a plan / question card passes none. */
private class CardSurfaceHost(
    base: String?,
    token: String?,
    private val onActionEvent: ((name: String, context: JsonObject?) -> Unit)? = null,
) : HostPlugin {
    private val origin = base?.trimEnd('/')

    private val ownHost: List<String>? = cardAllowPrivate(origin)

    override val mediaOptions: MediaOptions = MediaOptions(
        baseUrl = origin,
        rules = if (origin != null && token != null) {
            listOf(MediaRule("$origin/api/attachments/", mapOf("Authorization" to "Bearer $token")))
        } else {
            emptyList()
        },
        // https, plus the instance's own scheme (a local http dev stack).
        schemes = listOfNotNull("https", origin?.substringBefore("://")?.takeIf { it == "http" }),
        allowPrivate = ownHost,
    )

    override val urlPolicy: UrlPolicy = UrlPolicy(schemes = listOf("https"), baseUrl = origin, allowPrivate = ownHost)

    /** Never opened from a card: links stay inert (no transport). */
    override fun openUrl(url: String) {}

    override fun onAction(event: SurfaceActionEvent) {
        val forward = onActionEvent ?: return
        val context = runCatching { Json.parseToJsonElement(event.context.json) as? JsonObject }.getOrNull()
        forward(event.name, context)
    }
}

private fun cardHost(
    context: Context,
    onAction: ((name: String, context: JsonObject?) -> Unit)? = null,
): HostPlugin {
    val auth = runCatching {
        EntryPointAccessors.fromApplication(context.applicationContext, CardSurfaceEntryPoint::class.java).authRepository()
    }.getOrNull()
    val base = auth?.instanceUrl?.value
    val token = auth?.let { a -> base?.let { resolveAccountForUrl(a.accounts.value, a.activeAccountId.value, it)?.token } }
    return CardSurfaceHost(base, token, onAction)
}

/**
 * The [SurfaceModel] of a card's [surface] (the core catalog, the card host);
 * null when the core refuses it. [onAction] (thread 58cb32df) receives every
 * action event the surface raises.
 */
@Composable
internal fun rememberCardSurfaceModel(
    surface: CardSurface,
    onAction: ((name: String, context: JsonObject?) -> Unit)? = null,
): SurfaceModel? {
    val context = LocalContext.current
    val latest = androidx.compose.runtime.rememberUpdatedState(onAction)
    val forward: ((String, JsonObject?) -> Unit)? = if (onAction == null) null else { name, ctx -> latest.value?.invoke(name, ctx) }
    return remember(surface) {
        runCatching {
            SurfaceModel(
                id = "card-${surface.root.hashCode()}",
                options = SurfaceOptions(),
                host = cardHost(context, forward),
            ) { error("set by ExponentialSurface") }.also { m ->
                m.setNested(surface.root.toString())
                surface.data?.forEach { (key, value) ->
                    m.setData("/" + key.replace("~", "~0").replace("/", "~1"), JsonValue.parse(value.toString()))
                }
            }
        }.getOrNull()
    }
}

/**
 * The surface in its own frame (full card width, `plan-card-surface`) under
 * the [PlanCard.Copy.SURFACE] label. A surface the core refuses paints the
 * label over an empty frame.
 * [inert] = an answered card: taps never reach the surface.
 */
@Composable
fun CardSurfaceView(
    surface: CardSurface,
    handle: CardSurfaceHandle,
    inert: Boolean,
    modifier: Modifier = Modifier,
) {
    val model = rememberCardSurfaceModel(surface)
    handle.model = model
    // plan-card.json `card` (web `PlanCardSurface`): the "Preview" label
    // ABOVE the frame, `surfaceLabelGap` apart; the frame = the row fill
    // inside a 1dp stroke-card border, `surfaceRadius`, `surfacePadding`.
    val shape = RoundedCornerShape(PlanCard.Look.SURFACE_RADIUS.dp)
    Column(
        modifier = modifier.fillMaxWidth().testTag(PlanCard.TestIds.SURFACE),
        verticalArrangement = Arrangement.spacedBy(PlanCard.Look.SURFACE_LABEL_GAP.dp),
    ) {
        Text(
            PlanCard.Copy.SURFACE,
            style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .clip(shape)
                .background(GlassTokens.RowFill, shape)
                .border(PlanCard.Look.PICTURE_STROKE.dp, GlassTokens.StrokeCard, shape)
                .padding(PlanCard.Look.SURFACE_PADDING.dp)
                .then(if (inert) Modifier.blockTouches() else Modifier),
        ) {
            if (model != null) ExponentialSurface(model)
        }
    }
}

/** Swallows every pointer event before the surface sees it. */
private fun Modifier.blockTouches(): Modifier = pointerInput(Unit) {
    awaitPointerEventScope {
        while (true) {
            awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() }
        }
    }
}
