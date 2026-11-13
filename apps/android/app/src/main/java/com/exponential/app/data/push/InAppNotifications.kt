package com.exponential.app.data.push

import android.content.Intent
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * #EXP-1815 (notification-delivery.json PRESENTATION): the bridge between
 * [FcmService] and the running UI. AppNavHost publishes the screen on top
 * ([viewing], a [ViewingScreen] the service turns into a thread) and collects
 * [toasts] into THE toast; the service asks [foreground] and, while the app is
 * up, hands a push here instead of posting it.
 */
@Singleton
class InAppNotifications @Inject constructor() {

    /** A push the app shows as THE toast: [open] = the push tap's intent. */
    data class Push(val title: String, val body: String?, val open: Intent)

    private val _viewing = MutableStateFlow<ViewingScreen?>(null)
    val viewing: StateFlow<ViewingScreen?> = _viewing.asStateFlow()

    private val _toasts = MutableSharedFlow<Push>(extraBufferCapacity = 8)
    val toasts: SharedFlow<Push> = _toasts.asSharedFlow()

    fun setViewing(screen: ViewingScreen?) {
        _viewing.value = screen
    }

    /** The app is in the foreground (a started activity). */
    fun foreground(): Boolean =
        ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)

    /** True when a toast host took it; false = nobody listens (post it instead). */
    fun toast(push: Push): Boolean = _toasts.subscriptionCount.value > 0 && _toasts.tryEmit(push)
}

/**
 * The screen on top, as navigation knows it. Everything maps straight to a
 * thread except a raw session view, whose chat (its root) the service looks
 * up in the synced sessions ([FcmService]).
 */
sealed interface ViewingScreen {
    /** A screen whose thread is known: `issue:<id>`, `session:<rootId>`, `device:<id>`, `inbox`. */
    data class Thread(val thread: String) : ViewingScreen

    /** The session view of [sessionId]: its thread is `session:<root>`. */
    data class Session(val sessionId: String) : ViewingScreen
}
