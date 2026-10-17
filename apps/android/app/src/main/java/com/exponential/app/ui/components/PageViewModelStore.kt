package com.exponential.app.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.hilt.navigation.HiltViewModelFactory
import androidx.lifecycle.HasDefaultViewModelProviderFactory
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.viewmodel.CreationExtras
import androidx.lifecycle.viewmodel.compose.LocalViewModelStoreOwner
import androidx.navigation.NavBackStackEntry

/**
 * EXP-1343: a pager page's OWN [ViewModelStoreOwner], cleared the moment the
 * page leaves composition. Without it every `hiltViewModel(key = …)` a page
 * makes lands on the screen's nav entry and lives until the screen is popped:
 * each swipe to another issue (or session) added its view models — Room
 * flows, a relay connection — and none ever died. The owner delegates its
 * factory (Hilt, through the parent nav entry) and creation extras to the
 * parent, so `hiltViewModel` works inside [content] unchanged.
 */
@Composable
fun PageViewModelStore(content: @Composable () -> Unit) {
    val parent = checkNotNull(LocalViewModelStoreOwner.current) { "PageViewModelStore needs a ViewModelStoreOwner" }
    val context = LocalContext.current
    val owner = remember(parent) {
        val factory: ViewModelProvider.Factory = when (parent) {
            is NavBackStackEntry -> HiltViewModelFactory(context, parent.defaultViewModelProviderFactory)
            is HasDefaultViewModelProviderFactory -> parent.defaultViewModelProviderFactory
            else -> ViewModelProvider.NewInstanceFactory()
        }
        val extras = (parent as? HasDefaultViewModelProviderFactory)?.defaultViewModelCreationExtras
            ?: CreationExtras.Empty
        PageOwner(factory, extras)
    }
    DisposableEffect(owner) {
        onDispose { owner.viewModelStore.clear() }
    }
    CompositionLocalProvider(LocalViewModelStoreOwner provides owner, content = content)
}

private class PageOwner(
    override val defaultViewModelProviderFactory: ViewModelProvider.Factory,
    override val defaultViewModelCreationExtras: CreationExtras,
) : ViewModelStoreOwner, HasDefaultViewModelProviderFactory {
    override val viewModelStore = ViewModelStore()
}
