package com.exponential.app.spike

import android.os.SystemClock
import android.util.Log
import com.exponential.app.BuildConfig

/**
 * VAPP-3 spike call site (throwaway). Only live in `-PpeerSpike=true` builds, which add the
 * generated UniFFI bindings + libpeer_ffi.so (spike/vapp-3/android/out). Reflection keeps this
 * file compiling in normal builds, where the `uniffi.peer_ffi` package does not exist.
 */
object PeerSpike {
    fun touch() {
        if (!BuildConfig.PEER_SPIKE) return
        val t0 = SystemClock.elapsedRealtime()
        val version = runCatching {
            Class.forName("uniffi.peer_ffi.Peer_ffiKt").getMethod("peerVersion").invoke(null) as String
        }.getOrElse { "failed: $it" }
        Log.i("VAPP3", "peer-ffi $version loaded in ${SystemClock.elapsedRealtime() - t0} ms")
    }
}
