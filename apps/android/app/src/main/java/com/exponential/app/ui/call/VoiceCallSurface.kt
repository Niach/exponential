package com.exponential.app.ui.call

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import android.Manifest
import android.os.SystemClock
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.produceState
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import com.exponential.app.ui.components.CallControls
import com.exponential.app.ui.theme.GlassTokens
import kotlinx.coroutines.delay
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.withInfiniteAnimationFrameNanos
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.Text
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import at.exponential.ui.ffi.voiceWaveJson
import at.exponential.ui.primitives.VoiceWaveRing
import at.exponential.ui.primitives.VoiceWaveView
import com.exponential.app.data.api.VoiceApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.call.VoiceCallController
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.Voice
import com.exponential.app.domain.VoiceCall
import com.exponential.app.domain.parseDeviceDoctor
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.thread.AgentGlyph
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject

// NEW-69 (voice V5, `fixtures/voice-call.json`): the phone's call. THE
// telephone in the agent chat's header ([AgentCallButton]) and, S-3, THE
// PHONE-CALL screen ([VoiceCallScreen], styleguide entry `voice-call`):
// full screen, top to bottom the agent's avatar large with its activity
// ring (ringing a soft halo pulse, listening the success ring glowing with
// the level, speaking VoiceWave, thinking dim, muted none), the name, the
// status or the ticking timer, the caption, and at the bottom the round
// icon-only buttons ([CallControls], entry `call-controls`), shown while a
// call runs ([VoiceCallHost]).

/** The VoiceWave edge at `xl` (the size enum: sm 32 · md 56 · lg 160 · xl 200). */
private const val WAVE_XL = 200f

/** The brand mark inside the avatar. */
private const val MARK_SIZE = 56f

/** The avatar's disc. */
private val AVATAR_SIZE = 112.dp

/**
 * THE phone-call screen: [view] from [VoiceCall.callView]; [timer] = the
 * ticking `callTimer` text while [VoiceCall.View.status] is null; [level] /
 * [bands] move the ring (bands may be empty). [still] = the specimen's
 * frame zero.
 */
@Composable
fun VoiceCallScreen(
    agent: String,
    view: VoiceCall.View,
    /** The agent's mark (null icon = the Exponential mark: the Assistant). */
    agentIcon: String? = null,
    agentColor: String? = null,
    timer: String?,
    level: Float,
    bands: List<Double>,
    onMute: () -> Unit,
    onSpeaker: () -> Unit,
    onEnd: () -> Unit,
    modifier: Modifier = Modifier,
    still: Boolean = false,
) {
    Column(
        modifier = modifier
            .padding(horizontal = 24.dp, vertical = 32.dp)
            .testTag(VoiceCall.TestIds.SURFACE),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Spacer(Modifier.weight(0.6f))
        CompositionLocalProvider(LocalCallMark provides CallMark(agentIcon, agentColor)) {
            VoiceCallAvatar(activity = view.activity, level = level, bands = bands, still = still)
        }
        Spacer(Modifier.height(20.dp))
        Text(
            agent,
            style = MaterialTheme.typography.headlineSmall.copy(fontWeight = FontWeight.SemiBold),
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.height(4.dp))
        val status = view.status
        Text(
            status ?: timer.orEmpty(),
            style = MaterialTheme.typography.bodyMedium,
            color = DesignTokens.Palette.MutedForeground,
            textAlign = TextAlign.Center,
            modifier = if (status == null) Modifier.testTag(VoiceCall.TestIds.TIMER) else Modifier,
        )
        Spacer(Modifier.height(32.dp))
        Text(
            view.caption,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
            maxLines = 4,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.fillMaxWidth().widthIn(max = 360.dp),
        )
        Spacer(Modifier.weight(1f))
        CallControls(view = view, onMute = onMute, onSpeaker = onSpeaker, onEnd = onEnd)
    }
}

/**
 * The agent's avatar (the brand mark on its disc) inside its activity ring;
 * the state word is the avatar's accessibility value.
 */
@Composable
private fun VoiceCallAvatar(activity: VoiceCall.Activity, level: Float, bands: List<Double>, still: Boolean) {
    val reduced = LocalReduceMotion.current
    Box(
        modifier = Modifier
            .size(WAVE_XL.dp)
            .semantics { stateDescription = activity.label },
        contentAlignment = Alignment.Center,
    ) {
        when (activity) {
            VoiceCall.Activity.Speaking -> VoiceCallWave(activity = activity, level = level, bands = bands, still = still) { AvatarDisc() }
            VoiceCall.Activity.Ringing -> {
                RingingHalo(animates = !still && !reduced)
                AvatarDisc()
            }
            VoiceCall.Activity.Listening -> {
                val glow by animateFloatAsState(level.coerceIn(0f, 1f), label = "call-level")
                val green = DesignTokens.Semantic.Green
                Canvas(Modifier.size(AVATAR_SIZE + 32.dp)) {
                    val r = AVATAR_SIZE.toPx() / 2
                    drawCircle(green.copy(alpha = 0.10f + 0.25f * glow), radius = r + (8 + 12 * glow).dp.toPx())
                    drawCircle(green, radius = r + 3.dp.toPx(), style = Stroke(width = 3.dp.toPx()))
                }
                AvatarDisc()
            }
            VoiceCall.Activity.Thinking -> AvatarDisc(Modifier.alpha(0.5f))
            VoiceCall.Activity.Muted -> AvatarDisc()
        }
    }
}

@Composable
private fun AvatarDisc(modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .size(AVATAR_SIZE)
            .background(DesignTokens.Palette.Card, CircleShape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        val mark = LocalCallMark.current
        if (mark.icon == null) {
            ExponentialMark(size = MARK_SIZE.dp)
        } else {
            AgentGlyph(icon = mark.icon, color = mark.color, size = MARK_SIZE.dp)
        }
    }
}

/** S-3: the callee's mark, the same disc ×4 (iOS `VoiceCallCallee`). */
private data class CallMark(val icon: String?, val color: String?)

private val LocalCallMark = staticCompositionLocalOf { CallMark(null, null) }

/** Ringing: a soft halo pulsing out of the disc (still under reduced motion). */
@Composable
private fun RingingHalo(animates: Boolean) {
    val pulse = if (animates) {
        val transition = rememberInfiniteTransition(label = "call-ring")
        transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(durationMillis = 1600, easing = LinearEasing), RepeatMode.Restart),
            label = "call-ring-pulse",
        ).value
    } else {
        0.5f
    }
    val tint = DesignTokens.Palette.Foreground
    Canvas(Modifier.size(WAVE_XL.dp)) {
        val r = AVATAR_SIZE.toPx() / 2
        val reach = size.minDimension / 2 - r
        drawCircle(tint.copy(alpha = 0.06f), radius = r + reach * 0.45f)
        drawCircle(tint.copy(alpha = 0.16f * (1f - pulse)), radius = r + reach * pulse)
    }
}

/**
 * Exponential UI's VoiceWave at `xl` with the brand mark as its child: the
 * rings and the child's scale come from the core (`voiceWaveJson`), one
 * frame per display frame while the activity moves (still under reduced
 * motion); idle and muted draw quiet (35%, the muted colour).
 */
@Composable
private fun VoiceCallWave(activity: VoiceCall.Activity, level: Float, bands: List<Double>, still: Boolean, child: @Composable () -> Unit) {
    val reduced = LocalReduceMotion.current
    val animates = !still && !reduced && activity != VoiceCall.Activity.Muted
    val elapsed = remember { mutableFloatStateOf(0f) }
    if (animates) {
        LaunchedEffect(activity) {
            var start = -1L
            while (true) {
                val now = withInfiniteAnimationFrameNanos { it }
                if (start < 0) start = now
                elapsed.floatValue = (now - start) / 1e9f
            }
        }
    }
    val quiet = activity == VoiceCall.Activity.Muted
    val outer = if (quiet) DesignTokens.Palette.MutedForeground else DesignTokens.Semantic.Blue
    val inner = if (quiet) DesignTokens.Palette.MutedForeground else MaterialTheme.colorScheme.primary
    val frame = voiceWaveFrame(activity.wire, level.toDouble(), bands, if (animates) elapsed.floatValue.toDouble() else 0.0, reduced || still)
    val rings = frame?.rings.orEmpty().map { r ->
        VoiceWaveRing(r.points, if (r.part == "outer") outer else inner, r.fillAlpha, r.strokeAlpha, r.strokeWidth)
    }
    VoiceWaveView(
        rings = rings,
        size = WAVE_XL.dp,
        // S-3: the avatar disc keeps its size (the phone-call layout); the rings move around it.
        childScale = 1f,
        modifier = Modifier.alpha(if (quiet) 0.35f else 1f),
    ) {
        child()
    }
}

private class WaveRing(val part: String, val points: FloatArray, val fillAlpha: Float, val strokeAlpha: Float, val strokeWidth: Float)

private class WaveFrame(val rings: List<WaveRing>, val childScale: Float)

/** One core frame (`voiceWaveJson`); null when the core is unavailable or refuses. */
private fun voiceWaveFrame(activity: String, level: Double, bands: List<Double>, elapsed: Double, reducedMotion: Boolean): WaveFrame? =
    runCatching {
        val input = buildJsonObject {
            put("size", WAVE_XL)
            put("activity", activity)
            put("level", level)
            putJsonArray("bands") { bands.forEach { add(JsonPrimitive(it)) } }
            put("elapsed", elapsed)
            put("reducedMotion", reducedMotion)
            putJsonObject("child") {
                put("width", AVATAR_SIZE.value)
                put("height", AVATAR_SIZE.value)
            }
        }
        val out = Json.parseToJsonElement(voiceWaveJson(input.toString())).jsonObject
        fun JsonObject.num(key: String): Float = (this[key] as? JsonPrimitive)?.doubleOrNull?.toFloat() ?: 0f
        val rings = out.getValue("rings").jsonArray.map { el ->
            val ring = el.jsonObject
            val pts = ring["points"]?.jsonArray ?: JsonArray(emptyList())
            val flat = FloatArray(pts.size * 2)
            pts.forEachIndexed { i, p ->
                val xy = p.jsonArray
                flat[2 * i] = xy.getOrNull(0)?.jsonPrimitive?.doubleOrNull?.toFloat() ?: 0f
                flat[2 * i + 1] = xy.getOrNull(1)?.jsonPrimitive?.doubleOrNull?.toFloat() ?: 0f
            }
            WaveRing(
                part = (ring["part"] as? JsonPrimitive)?.content ?: "outer",
                points = flat,
                fillAlpha = ring.num("fillAlpha"),
                strokeAlpha = ring.num("strokeAlpha"),
                strokeWidth = (ring["strokeWidth"] as? JsonPrimitive)?.doubleOrNull?.toFloat() ?: 1f,
            )
        }
        val childScale = (out["child"] as? JsonObject)?.num("scale")
            ?: if (out.num("childSize") > 0f) minOf(1f, out.num("childSize") / AVATAR_SIZE.value) else 0f
        WaveFrame(rings, childScale)
    }.getOrNull()

@HiltViewModel
class VoiceCallHostViewModel @Inject constructor(val controller: VoiceCallController) : ViewModel()

/**
 * Mounted once above the nav graph: the call surface while a call runs
 * (a full-screen dialog; back does not hang up, End call does) and the
 * failure's toast when it ends.
 */
@Composable
fun VoiceCallHost(viewModel: VoiceCallHostViewModel = hiltViewModel()) {
    val controller = viewModel.controller
    val ui by controller.ui.collectAsStateWithLifecycle()
    val level by controller.level.collectAsStateWithLifecycle()
    val toaster = LocalToaster.current
    LaunchedEffect(ui.toast) {
        val toast = ui.toast ?: return@LaunchedEffect
        toaster.error(toast)
        controller.consumeToast()
    }
    val view = ui.view ?: return
    Dialog(
        onDismissRequest = {},
        properties = DialogProperties(
            dismissOnBackPress = false,
            dismissOnClickOutside = false,
            usePlatformDefaultWidth = false,
            decorFitsSystemWindows = false,
        ),
    ) {
        // The screen never dims on its own while a call runs.
        val window = LocalView.current
        DisposableEffect(window) {
            window.keepScreenOn = true
            onDispose { window.keepScreenOn = false }
        }
        // The timer ticks once a second from the answer on.
        val answeredAt = ui.answeredAt
        val now by produceState(SystemClock.elapsedRealtime(), answeredAt) {
            while (answeredAt != null) {
                value = SystemClock.elapsedRealtime()
                delay(1_000L - (value - answeredAt) % 1_000L)
            }
        }
        AppBackground {
            VoiceCallScreen(
                agent = ui.agent,
                agentIcon = ui.agentIcon,
                agentColor = ui.agentColor,
                view = view,
                timer = answeredAt?.let { VoiceCall.callTimer(now - it) },
                level = level,
                bands = emptyList(),
                onMute = controller::toggleMute,
                onSpeaker = controller::toggleSpeaker,
                onEnd = controller::end,
                modifier = Modifier.fillMaxSize().systemBarsPadding(),
            )
        }
    }
}

/**
 * The telephone's availability for THIS member on a phone: `voice.get`
 * (fetched whenever the chat appears), the synced devices row of the voice
 * device (its name, `last_seen_at` freshness, its doctor's `speech` row).
 */
@HiltViewModel
class AgentCallViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val voiceApi: VoiceApi,
    private val controller: VoiceCallController,
) : ViewModel() {

    data class Target(val availability: VoiceCall.Availability, val device: String?)

    private val settings = MutableStateFlow<Voice.Settings?>(null)

    private val devices = accountDatabaseFlow(auth, holder).scopedQuery(emptyList()) { it.deviceDao().observeAll() }

    val target: StateFlow<Target> = combine(settings, devices, auth.userId, DeviceLiveness.ticker()) { s, rows, me, now ->
        val id = s?.voiceDeviceId
        val row = id?.let { rows.firstOrNull { r -> r.deviceId == it && (me == null || r.userId == me) } }
        val device = row?.let { it.label.ifBlank { it.deviceId } }
        Target(
            availability = VoiceCall.callAvailability(
                VoiceCall.AvailabilityInput(
                    platform = VoiceCall.Platform.Phone,
                    device = device,
                    online = row != null && DeviceLiveness.isOnline(row.lastSeenAt, now),
                    here = false,
                    speech = VoiceCall.callSpeech(parseDeviceDoctor(row?.doctor)),
                    busy = false,
                    meeting = s?.meetingStartedAt != null,
                ),
            ),
            device = device,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), Target(VoiceCall.callAvailability(OFF), null))

    fun refresh() {
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            runCatching { voiceApi.get(accountId) }.onSuccess { settings.value = it }
        }
    }

    fun hasMicPermission(): Boolean = controller.hasMicPermission()

    fun start(agentId: String, agentName: String, agentIcon: String?, agentColor: String?) {
        controller.start(agentId, agentName, target.value.device.orEmpty(), settings.value?.language, agentIcon, agentColor)
    }

    fun permissionDenied() {
        controller.permissionDenied(target.value.device.orEmpty(), settings.value?.language)
    }

    private companion object {
        val OFF = VoiceCall.AvailabilityInput(VoiceCall.Platform.Phone, null, false, false, null, false)
    }
}

/**
 * THE telephone in the agent chat's header, left of `…`: the header's icon
 * button (44 / 18 like the menu). Disabled = dimmed, its reason on long
 * press (the app's tooltip). A tap asks for the microphone once, then calls.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentCallButton(
    agentId: String?,
    agentName: String,
    agentIcon: String? = null,
    agentColor: String? = null,
    viewModel: AgentCallViewModel = hiltViewModel(),
) {
    LaunchedEffect(Unit) { viewModel.refresh() }
    val target by viewModel.target.collectAsStateWithLifecycle()
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (agentId == null) return@rememberLauncherForActivityResult
        if (granted) viewModel.start(agentId, agentName, agentIcon, agentColor) else viewModel.permissionDenied()
    }
    val enabled = target.availability.enabled && agentId != null
    val button: @Composable () -> Unit = {
        CircleIconButton(
            ExpIcons.voiceCall,
            VoiceCall.Copy.call(agentName),
            onClick = {
                if (agentId == null) return@CircleIconButton
                if (viewModel.hasMicPermission()) viewModel.start(agentId, agentName, agentIcon, agentColor) else permission.launch(Manifest.permission.RECORD_AUDIO)
            },
            enabled = enabled,
            size = 44.dp,
            glyphSize = 18.dp,
            modifier = Modifier.testTag(VoiceCall.TestIds.CALL),
        )
    }
    val reason = target.availability.reason
    if (!enabled && reason != null) {
        TooltipBox(
            positionProvider = TooltipDefaults.rememberPlainTooltipPositionProvider(),
            tooltip = { PlainTooltip { Text(reason) } },
            state = rememberTooltipState(),
        ) { button() }
    } else {
        button()
    }
}

/** The phone-call screen for one `surfaceSpecimens` row (the styleguide's static frame). */
@Composable
fun VoiceCallSpecimen(specimen: VoiceCall.SurfaceSpecimen, modifier: Modifier = Modifier) {
    VoiceCallScreen(
        agent = specimen.agent,
        view = specimen.view,
        timer = specimen.timer,
        level = specimen.level.toFloat(),
        bands = specimen.bands,
        onMute = {},
        onSpeaker = {},
        onEnd = {},
        modifier = modifier,
        still = true,
    )
}
