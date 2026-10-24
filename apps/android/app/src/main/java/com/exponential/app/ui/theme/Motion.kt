package com.exponential.app.ui.theme

import android.content.ContentResolver
import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext

// Motion (EXP-523) — the hand-written half of the shared motion system.
//
// DesignTokens.Motion (generated from packages/design-tokens/tokens.json, the
// same source the web CSS vars and the iOS/desktop tokens come from) carries
// the raw millisecond durations and cubic-bezier easings. This file owns the
// reduce-motion bridge, which Android had none of before: iOS honours
// `accessibilityReduceMotion` and web has `prefers-reduced-motion`, while every
// Compose animation here ran at full length regardless of the user's setting.

/**
 * True when the OS has animations switched off. Both Accessibility → "Remove
 * animations" and Developer Options → "Animator duration scale: off" write
 * ANIMATOR_DURATION_SCALE = 0, so it is the one signal to read.
 *
 * Provided once by [ExponentialTheme]. The default is false so previews and
 * unit tests animate normally.
 */
val LocalReduceMotion = staticCompositionLocalOf { false }

/**
 * The one reader of [Settings.Global.ANIMATOR_DURATION_SCALE]. OBSERVED rather
 * than read once: the user can flip it from Developer Options or the
 * accessibility shortcut without the process restarting, and a stale `true`
 * would leave the whole app permanently motionless. No permission required.
 */
@Composable
fun rememberReduceMotion(): Boolean {
    val resolver = LocalContext.current.contentResolver
    val state = remember(resolver) { mutableStateOf(animationsDisabled(resolver)) }
    DisposableEffect(resolver) {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) {
                state.value = animationsDisabled(resolver)
            }
        }
        resolver.registerContentObserver(
            Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE),
            false,
            observer,
        )
        onDispose { resolver.unregisterContentObserver(observer) }
    }
    return state.value
}

internal fun animationsDisabled(resolver: ContentResolver): Boolean =
    Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f

/**
 * The shared motion tokens, reduce-motion aware. Never write `tween(180)` —
 * reach for these. Every spec collapses to [snap] when the OS has animations
 * off, which is the Compose equivalent of iOS's nil `Animation`.
 *
 * Two flavours of each: a zero-arg `@Composable` one that reads
 * [LocalReduceMotion] itself, and an explicit one taking the flag, for the
 * lambdas that are NOT composable — `AnimatedContent`'s `transitionSpec` and
 * `NavHost`'s enter/exit transitions. In those, read `LocalReduceMotion.current`
 * in the enclosing composable and pass it down.
 */
object Motion {
    fun <T> fast(
        reduceMotion: Boolean,
        easing: Easing = DesignTokens.Motion.Ease.Standard,
    ): FiniteAnimationSpec<T> = spec(DesignTokens.Motion.Duration.Fast, easing, reduceMotion)

    fun <T> standard(
        reduceMotion: Boolean,
        easing: Easing = DesignTokens.Motion.Ease.Standard,
    ): FiniteAnimationSpec<T> = spec(DesignTokens.Motion.Duration.Standard, easing, reduceMotion)

    fun <T> slow(
        reduceMotion: Boolean,
        easing: Easing = DesignTokens.Motion.Ease.Standard,
    ): FiniteAnimationSpec<T> = spec(DesignTokens.Motion.Duration.Slow, easing, reduceMotion)

    // The composable forms take NO arguments on purpose: a zero-arg overload
    // beside a `(Boolean, Easing = …)` one can never be ambiguous, and nothing
    // yet needs a non-default curve from composable scope. If something does,
    // reach for the explicit form above rather than adding a default here.
    @Composable
    fun <T> fast(): FiniteAnimationSpec<T> = fast(LocalReduceMotion.current)

    @Composable
    fun <T> standard(): FiniteAnimationSpec<T> = standard(LocalReduceMotion.current)

    @Composable
    fun <T> slow(): FiniteAnimationSpec<T> = slow(LocalReduceMotion.current)

    private fun <T> spec(
        durationMs: Int,
        easing: Easing,
        reduceMotion: Boolean,
    ): FiniteAnimationSpec<T> =
        if (reduceMotion) snap() else tween(durationMs, easing = easing)
}

// EXP-1243 (EXP-1259 D17): the motion rules (animations.dev), pinned in
// `packages/design-tokens` and applied ×4. Enters ease OUT (`decelerate`) at
// the standard duration, exits are faster (`accelerate`, the fast duration)
// and opacity-only; a popover or menu scales from `PopoverFrom` (never from 0)
// at its trigger's origin; a primary button presses to `Press`. Nothing
// animates for keyboard-initiated or high-frequency changes (typeahead, tab
// switches, list navigation). Reduce motion = opacity only.

/** A popover / menu ENTER: fade + scale from [DesignTokens.Motion.Scale.PopoverFrom]
 *  at [origin] (the trigger's side), decelerating; opacity only under reduce
 *  motion. */
fun popoverEnter(reduceMotion: Boolean, origin: TransformOrigin = TransformOrigin.Center): EnterTransition {
    val fade = fadeIn(tween(DesignTokens.Motion.Duration.Standard, easing = DesignTokens.Motion.Ease.Decelerate))
    if (reduceMotion) return fade
    return fade + scaleIn(
        tween(DesignTokens.Motion.Duration.Standard, easing = DesignTokens.Motion.Ease.Decelerate),
        initialScale = DesignTokens.Motion.Scale.PopoverFrom,
        transformOrigin = origin,
    )
}

/** A popover / menu EXIT: opacity only, faster than its enter. */
fun popoverExit(): ExitTransition =
    fadeOut(tween(DesignTokens.Motion.Duration.Fast, easing = DesignTokens.Motion.Ease.Accelerate))

/** A primary button's press: scale to [DesignTokens.Motion.Scale.Press] while
 *  [pressed]; none under reduce motion. */
@Composable
fun Modifier.pressScale(pressed: Boolean): Modifier {
    val reduce = LocalReduceMotion.current
    val target = if (pressed && !reduce) DesignTokens.Motion.Scale.Press else 1f
    val scale by animateFloatAsState(
        target,
        Motion.fast(reduce, if (pressed) DesignTokens.Motion.Ease.Decelerate else DesignTokens.Motion.Ease.Standard),
        label = "press-scale",
    )
    return if (scale == 1f) this else graphicsLayer { scaleX = scale; scaleY = scale }
}

/** A motion token by its fixture NAME (`fast` | `standard` | `slow`), in ms
 *  (thread 58cb32df: `chats.json` `surfaceCard.enter` names its tokens). */
fun motionDurationMs(name: String): Int = when (name) {
    "fast" -> DesignTokens.Motion.Duration.Fast
    "slow" -> DesignTokens.Motion.Duration.Slow
    else -> DesignTokens.Motion.Duration.Standard
}

/** An easing token by its fixture NAME (`standard` | `decelerate` | `accelerate`). */
fun motionEasing(name: String): Easing = when (name) {
    "decelerate" -> DesignTokens.Motion.Ease.Decelerate
    "accelerate" -> DesignTokens.Motion.Ease.Accelerate
    else -> DesignTokens.Motion.Ease.Standard
}

/** The px a phone chat bubble rises by when it arrives live (chats.json:
 *  "an opacity + 6px rise at `standard` / `decelerate`"). */
const val BUBBLE_RISE_PX = 6f

/**
 * Thread 58cb32df (chats.json TRANSCRIPT): a NEW bubble that arrives while the
 * chat is open rises in once — opacity + [BUBBLE_RISE_PX] at `standard` /
 * `decelerate`; reduce motion = the opacity alone. [animate] false (history,
 * a reopen, a scroll back) draws it in place.
 */
@Composable
fun Modifier.liveArrival(animate: Boolean): Modifier {
    val reduce = LocalReduceMotion.current
    // Decided once per item: a later recomposition never re-plays it.
    val progress = androidx.compose.runtime.remember { androidx.compose.animation.core.Animatable(if (animate) 0f else 1f) }
    androidx.compose.runtime.LaunchedEffect(Unit) {
        if (progress.value >= 1f) return@LaunchedEffect
        progress.animateTo(
            1f,
            tween(DesignTokens.Motion.Duration.Standard, easing = DesignTokens.Motion.Ease.Decelerate),
        )
    }
    return graphicsLayer {
        val p = progress.value
        alpha = p
        if (!reduce) translationY = (1f - p) * BUBBLE_RISE_PX * density
    }
}
