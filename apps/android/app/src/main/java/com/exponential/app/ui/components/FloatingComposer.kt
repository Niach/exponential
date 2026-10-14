package com.exponential.app.ui.components

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.ime
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest

/**
 * What [FloatingComposer] hands its expanded card: the modifier the card's
 * text field must wear (it reports focus, which drives collapse-on-blur) and
 * the guard a menu opened from INSIDE the card (a picker that portals out
 * and takes focus) sets while it is up, so the card never collapses under it.
 */
class FloatingComposerScope internal constructor(
    val fieldModifier: Modifier,
    val onMenuOpenChange: (id: String, open: Boolean) -> Unit,
)

/**
 * EXP-1316 (1e) THE phone reply box ×3, extracted from the session view's
 * steer composer: collapsed it is the floating bar's [BarCapsule] with the
 * [placeholder] (plus the host's [left]/[right]/[afterCentre] circles); a tap
 * expands it into THE composer card ([expandedContent], a [GlassComposer]).
 * The host overlays it on its content inside a [FloatingBarEdge], so it
 * floats over the tail and never pushes the content.
 *
 * The card collapses again (EXP-790/931) once its field HAD focus and lost it
 * with the keyboard down and nothing drafted ([hasDraft]), or while the agent
 * works ([working]) with nobody typing in it — each after a ~200ms quiet
 * period and only while resumed (a picker backgrounds the activity). A draft
 * that arrives from outside (restored, a menu pick, an attachment) opens it.
 */
@Composable
fun FloatingComposer(
    expanded: Boolean,
    onExpandedChange: (Boolean) -> Unit,
    placeholder: String,
    /** Text typed or a file pending — the card never collapses over one. */
    hasDraft: Boolean,
    modifier: Modifier = Modifier,
    /** The agent is mid-turn: an idle, empty card stands down. */
    working: Boolean = false,
    capsuleTestTag: String = "floating-composer-capsule",
    left: (@Composable () -> Unit)? = null,
    right: (@Composable () -> Unit)? = null,
    afterCentre: (@Composable () -> Unit)? = null,
    expandedContent: @Composable FloatingComposerScope.() -> Unit,
) {
    LaunchedEffect(hasDraft) {
        if (!expanded && hasDraft) onExpandedChange(true)
    }
    var fieldFocused by remember { mutableStateOf(false) }
    var innerMenus by remember { mutableStateOf(emptySet<String>()) }
    val innerMenuState = rememberUpdatedState(innerMenus.isNotEmpty())
    val imeVisible = WindowInsets.ime.getBottom(LocalDensity.current) > 0
    val imeVisibleState = rememberUpdatedState(imeVisible)
    val draftState = rememberUpdatedState(hasDraft)
    val workingState = rememberUpdatedState(working)
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    // Collapse-on-blur (the issue comment bar's rule).
    LaunchedEffect(expanded) {
        if (!expanded) return@LaunchedEffect
        var hadFocus = false
        snapshotFlow { fieldFocused to imeVisibleState.value }.collectLatest { (focused, ime) ->
            if (focused) {
                hadFocus = true
                return@collectLatest
            }
            if (!hadFocus || ime) return@collectLatest
            delay(200)
            if (!draftState.value && !innerMenuState.value &&
                lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)
            ) {
                onExpandedChange(false)
            }
        }
    }
    // EXP-931: while the agent works, an empty card nobody types in stands
    // down so the bar (and its circles) come back.
    LaunchedEffect(expanded) {
        if (!expanded) return@LaunchedEffect
        var hadFocus = false
        snapshotFlow {
            val idle = workingState.value && !fieldFocused && !imeVisibleState.value &&
                !draftState.value && !innerMenuState.value
            fieldFocused to idle
        }.collectLatest { (focused, idle) ->
            if (focused) hadFocus = true
            if (!hadFocus || !idle) return@collectLatest
            delay(200)
            if (lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) onExpandedChange(false)
        }
    }
    val reduceMotion = LocalReduceMotion.current
    AnimatedContent(
        targetState = expanded,
        transitionSpec = {
            val spec = Motion.slow<Float>(reduceMotion)
            (fadeIn(spec) togetherWith fadeOut(spec)).using(SizeTransform(clip = false))
        },
        label = "floating-composer",
        modifier = modifier.fillMaxWidth(),
    ) { isExpanded ->
        if (isExpanded) {
            val scope = remember {
                FloatingComposerScope(
                    fieldModifier = Modifier.onFocusChanged { fieldFocused = it.isFocused },
                    onMenuOpenChange = { id, open -> innerMenus = if (open) innerMenus + id else innerMenus - id },
                )
            }
            scope.expandedContent()
        } else {
            FloatingBottomBar(left = left, right = right, afterCentre = afterCentre) {
                BarCapsule(
                    label = placeholder,
                    onClick = { onExpandedChange(true) },
                    modifier = Modifier.testTag(capsuleTestTag),
                )
            }
        }
    }
}
