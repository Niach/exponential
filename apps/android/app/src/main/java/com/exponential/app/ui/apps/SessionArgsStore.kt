package com.exponential.app.ui.apps

import android.os.Bundle
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.hilt.navigation.HiltViewModelFactory
import androidx.lifecycle.DEFAULT_ARGS_KEY
import androidx.lifecycle.HasDefaultViewModelProviderFactory
import androidx.lifecycle.VIEW_MODEL_STORE_OWNER_KEY
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.viewmodel.CreationExtras
import androidx.lifecycle.viewmodel.MutableCreationExtras
import androidx.lifecycle.viewmodel.compose.LocalViewModelStoreOwner
import androidx.navigation.NavBackStackEntry

/**
 * VAPP-110: an embedded conversation's OWN [ViewModelStoreOwner] whose
 * view models read `sessionId` = [sessionId] from their SavedStateHandle,
 * as if they sat on the `thread/{sessionId}` route (the thread screen's
 * [com.exponential.app.ui.thread.ThreadViewModel] serves the app editor's
 * chat unchanged). Cleared when it leaves composition (the
 * [com.exponential.app.ui.components.PageViewModelStore] rule).
 */
@Composable
fun SessionArgsStore(sessionId: String, content: @Composable () -> Unit) {
    val parent = checkNotNull(LocalViewModelStoreOwner.current) { "SessionArgsStore needs a ViewModelStoreOwner" }
    val context = LocalContext.current
    val owner = remember(parent, sessionId) {
        val factory: ViewModelProvider.Factory = when (parent) {
            is NavBackStackEntry -> HiltViewModelFactory(context, parent.defaultViewModelProviderFactory)
            is HasDefaultViewModelProviderFactory -> parent.defaultViewModelProviderFactory
            else -> ViewModelProvider.NewInstanceFactory()
        }
        val base = (parent as? HasDefaultViewModelProviderFactory)?.defaultViewModelCreationExtras ?: CreationExtras.Empty
        val extras = MutableCreationExtras(base).apply {
            val args = Bundle(base[DEFAULT_ARGS_KEY] ?: Bundle())
            args.putString("sessionId", sessionId)
            set(DEFAULT_ARGS_KEY, args)
        }
        ArgsOwner(factory, extras).also { owner ->
            // Release train 2026-10-23 (VAPP-110): `createSavedStateHandle`
            // caches each view model's handle in the VIEW_MODEL_STORE_OWNER's
            // store under the view model's key; left on the editor screen's
            // entry, a new [sessionId] would get the old handle (old
            // sessionId). Keyed to this owner, the handle dies with it.
            extras[VIEW_MODEL_STORE_OWNER_KEY] = owner
        }
    }
    DisposableEffect(owner) {
        onDispose { owner.viewModelStore.clear() }
    }
    CompositionLocalProvider(LocalViewModelStoreOwner provides owner, content = content)
}

private class ArgsOwner(
    override val defaultViewModelProviderFactory: ViewModelProvider.Factory,
    override val defaultViewModelCreationExtras: CreationExtras,
) : ViewModelStoreOwner, HasDefaultViewModelProviderFactory {
    override val viewModelStore = ViewModelStore()
}
