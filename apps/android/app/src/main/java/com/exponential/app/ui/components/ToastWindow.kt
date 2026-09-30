package com.exponential.app.ui.components

import android.app.Activity
import android.app.Dialog
import android.content.Context
import android.content.ContextWrapper
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.os.Build
import android.view.Gravity
import android.view.View
import android.view.ViewTreeObserver
import android.view.inspector.WindowInspector
import android.view.Window
import android.view.WindowManager
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionContext
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCompositionContext
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import androidx.core.view.OneShotPreDrawListener
import androidx.core.view.WindowCompat
import androidx.lifecycle.findViewTreeLifecycleOwner
import androidx.lifecycle.findViewTreeViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.findViewTreeSavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import kotlinx.coroutines.delay

/**
 * EXP-1031: the toast stack's OWN window, so a toast always draws above a
 * bottom sheet (`ModalBottomSheet` = a dialog window) or an `AlertDialog` —
 * the iOS overlay window's twin. Android has no z-order control between an
 * app's windows beyond "the newest dialog wins", so every [Toaster.show]
 * (a [Toaster.generation] bump) — and every sheet/dialog opened over a
 * showing toast (the main window losing focus) — re-creates this window on
 * top; the stack's
 * state lives in [Toaster], so nothing replays.
 *
 * A plain [Dialog] (not compose `Dialog`): its flags must be set BEFORE
 * `show()` — a focusable window, even for a frame, would steal focus (and
 * the keyboard) from the sheet underneath. NOT_FOCUSABLE keeps back + keys
 * with the sheet; NOT_TOUCH_MODAL + a window sized to the stack alone lets
 * every touch beside it reach the app. The bottom offset (navigation bar or
 * keyboard, + [Toaster.bottomInset], + the gap) is the window's `y`.
 */
@Composable
fun ToastWindow(toaster: Toaster) {
    val view = LocalView.current
    val parent = rememberCompositionContext()
    val holder = remember(view) { ToastWindowHolder(view) }
    val density = LocalDensity.current

    val available = LocalConfiguration.current.screenWidthDp.dp - ToastDefaults.HorizontalInset * 2
    val widthPx = with(density) { ToastDefaults.cardWidth(available).roundToPx() }
    val navBottom = WindowInsets.navigationBars.getBottom(density)
    val imeBottom = WindowInsets.ime.getBottom(density)
    // The keyboard replaces the bar + banner inset: the stack sits on it.
    val yPx = with(density) {
        if (imeBottom > navBottom) {
            imeBottom + ToastDefaults.BottomGap.roundToPx()
        } else {
            navBottom + (toaster.bottomInset + ToastDefaults.BottomGap).roundToPx()
        }
    }

    val visible = toaster.rendered.isNotEmpty()
    val generation = toaster.generation
    DisposableEffect(holder) { onDispose { holder.close() } }
    // A sheet or dialog opened over a SHOWING toast takes the main window's
    // focus: re-raise the stack above it (a new toast re-raises by itself).
    DisposableEffect(view, toaster) {
        val listener = ViewTreeObserver.OnWindowFocusChangeListener { hasFocus ->
            if (!hasFocus) toaster.raise()
        }
        view.viewTreeObserver.addOnWindowFocusChangeListener(listener)
        onDispose { view.viewTreeObserver.removeOnWindowFocusChangeListener(listener) }
    }
    // A dialog opened from a sheet or menu never touches the main window's
    // focus: while toasts show, check whether any window was added after ours.
    LaunchedEffect(holder, visible) {
        while (visible && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            delay(BURIED_POLL_MS)
            if (holder.isBuried()) toaster.raise()
        }
    }
    SideEffect {
        if (visible) {
            holder.raise(generation, parent, widthPx, yPx) {
                ToastStackBox(toaster, Modifier.fillMaxWidth())
            }
        } else {
            holder.close()
        }
    }
}

private const val BURIED_POLL_MS = 250L

/** Owns the current toast window; [raise] swaps in a fresh one on top. */
internal class ToastWindowHolder(private val anchor: View) {
    private var dialog: Dialog? = null
    private var shownGeneration = -1
    private var widthPx = 0
    private var yPx = 0

    fun raise(
        generation: Int,
        parent: CompositionContext,
        widthPx: Int,
        yPx: Int,
        content: @Composable () -> Unit,
    ) {
        val current = dialog
        if (current != null && generation == shownGeneration) {
            place(current, widthPx, yPx)
            return
        }
        if (anchor.context.findActivity()?.let { it.isFinishing || it.isDestroyed } != false) return
        val next = create(parent, content)
        place(next, widthPx, yPx, force = true)
        next.show()
        dialog = next
        shownGeneration = generation
        // The old window goes only once the new one has drawn, so the stack
        // never blinks while it is re-raised.
        if (current != null) {
            val view = next.window?.decorView
            if (view == null) {
                current.dismiss()
            } else {
                OneShotPreDrawListener.add(view) { view.post { current.dismiss() } }
            }
        }
    }

    /**
     * True when a visible window of this process was added after the toast
     * window — the newest same-type window draws on top, so it covers ours.
     */
    fun isBuried(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return false
        val ours = dialog?.window?.decorView ?: return false
        val views = WindowInspector.getGlobalWindowViews()
        val index = views.indexOf(ours)
        if (index < 0) return false
        return views.subList(index + 1, views.size).any {
            it.isAttachedToWindow && it.windowVisibility == View.VISIBLE
        }
    }

    fun close() {
        dialog?.dismiss()
        dialog = null
        shownGeneration = -1
    }

    private fun create(parent: CompositionContext, content: @Composable () -> Unit): Dialog {
        val context = anchor.context
        val dialog = Dialog(context, android.R.style.Theme_DeviceDefault_Dialog_NoActionBar)
        dialog.setCancelable(false)
        dialog.setCanceledOnTouchOutside(false)
        val window = dialog.window!!
        window.requestFeature(Window.FEATURE_NO_TITLE)
        window.setBackgroundDrawable(ColorDrawable(Color.TRANSPARENT))
        window.clearFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND)
        window.setDimAmount(0f)
        window.addFlags(
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
        )
        window.setWindowAnimations(0)
        window.setGravity(Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // y measures from the display's bottom edge, insets included.
            window.attributes = window.attributes.apply { fitInsetsTypes = 0 }
        } else {
            window.addFlags(
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                    WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            )
        }
        val composeView = ComposeView(context).apply {
            setParentCompositionContext(parent)
            setContent(content)
        }
        dialog.setContentView(composeView)
        window.decorView.apply {
            setViewTreeLifecycleOwner(anchor.findViewTreeLifecycleOwner())
            setViewTreeViewModelStoreOwner(anchor.findViewTreeViewModelStoreOwner())
            setViewTreeSavedStateRegistryOwner(anchor.findViewTreeSavedStateRegistryOwner())
            elevation = 0f
            setPadding(0, 0, 0, 0)
        }
        return dialog
    }

    private fun place(dialog: Dialog, widthPx: Int, yPx: Int, force: Boolean = false) {
        if (!force && widthPx == this.widthPx && yPx == this.yPx) return
        this.widthPx = widthPx
        this.yPx = yPx
        val window = dialog.window ?: return
        window.attributes = window.attributes.apply {
            width = widthPx
            height = WindowManager.LayoutParams.WRAP_CONTENT
            y = yPx
        }
    }
}

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}
