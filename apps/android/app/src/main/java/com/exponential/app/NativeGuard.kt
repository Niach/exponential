package com.exponential.app

import android.util.Log
import kotlinx.coroutines.CancellationException

/**
 * One native library's calls (UniFFI over JNA): the `.so` loads on the first
 * call, so a missing or refused library surfaces there as an [Error]
 * (`UnsatisfiedLinkError`, `NoClassDefFoundError` after a failed `UniffiLib`
 * init), which a `catch (e: Exception)` lets through to the crash handler.
 * A [LinkageError] turns the library OFF for this process: reported once,
 * every later call answers null without touching it. Any other [Error] is
 * reported, an [Exception] only logged; both answer null and the next call
 * tries again. Never throws, but cancellation.
 */
class NativeGuard(
    private val name: String,
    private val report: (message: String, error: Throwable) -> Unit = { message, error -> Observability.nonFatal("NativeGuard", message, error) },
) {
    /** The load failure that turned the library off (null = available). */
    @Volatile var off: Throwable? = null
        private set

    val available: Boolean get() = off == null

    fun <T> call(what: String, block: () -> T): T? {
        if (off != null) return null
        return try {
            block()
        } catch (e: CancellationException) {
            throw e
        } catch (e: LinkageError) {
            synchronized(this) {
                if (off == null) {
                    off = e
                    report("$name unavailable ($what): ${e.javaClass.simpleName}", e)
                }
            }
            null
        } catch (e: Error) {
            report("$name $what failed: ${e.javaClass.simpleName}", e)
            null
        } catch (e: Exception) {
            Log.w("NativeGuard", "$name $what failed: ${e.message}")
            null
        }
    }
}
