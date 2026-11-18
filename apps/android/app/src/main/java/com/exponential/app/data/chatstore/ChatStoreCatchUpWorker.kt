package com.exponential.app.data.chatstore

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.util.Log
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequest
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * #EDE-114 (chat-store.json THE PHONE WALK): the background catch-up runs
 * only inside the OS's own window that asks for an UNMETERED network AND
 * external power: a WorkManager periodic job with both constraints. The
 * window ending cancels [doWork], which stops the walk ([CatchUpDriver]: the
 * watermarks keep the place).
 */
class ChatStoreCatchUpWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    @EntryPoint
    @InstallIn(SingletonComponent::class)
    interface Deps {
        fun chatStores(): ChatStoreManager
    }

    override suspend fun doWork(): Result {
        val stores = EntryPointAccessors.fromApplication(applicationContext, Deps::class.java).chatStores()
        val ran = runCatching { stores.backgroundPass() }
            .onFailure { if (it is kotlinx.coroutines.CancellationException) throw it }
            .getOrDefault(false)
        Log.i("ChatStore", "background pass ran=$ran")
        return Result.success()
    }

    companion object {
        private const val NAME = "chat-store-catch-up"

        /** The window's constraints: unmetered + charging. */
        fun constraints(): Constraints = Constraints.Builder()
            .setRequiredNetworkType(NetworkType.UNMETERED)
            .setRequiresCharging(true)
            .build()

        /** Enqueued once per install (KEEP): as often as WorkManager allows (15 min), whenever the window is open. */
        fun schedule(context: Context) {
            runCatching {
                val request = PeriodicWorkRequestBuilder<ChatStoreCatchUpWorker>(
                    PeriodicWorkRequest.MIN_PERIODIC_INTERVAL_MILLIS,
                    TimeUnit.MILLISECONDS,
                )
                    .setConstraints(constraints())
                    .build()
                WorkManager.getInstance(context).enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
            }.onFailure { Log.w("ChatStore", "scheduling the background catch-up failed: ${it.javaClass.simpleName}") }
        }
    }
}

/** #EDE-114: whether the default network is unmetered (Wi-Fi, ethernet): foreground passes only then. */
object UnmeteredNetwork {
    fun watch(context: Context): StateFlow<Boolean> {
        val state = MutableStateFlow(false)
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return state.asStateFlow()
        runCatching {
            state.value = manager.getNetworkCapabilities(manager.activeNetwork)
                ?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) == true
            manager.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
                override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                    state.value = caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
                }

                override fun onLost(network: Network) {
                    state.value = false
                }
            })
        }.onFailure { Log.w("ChatStore", "network callback not registered: ${it.message}") }
        return state.asStateFlow()
    }
}
