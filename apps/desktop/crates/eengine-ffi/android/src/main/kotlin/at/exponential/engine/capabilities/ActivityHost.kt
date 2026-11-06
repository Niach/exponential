// #VAPP-265: how a blocking capability call reaches the UI. The Engine calls on
// its worker thread; the call posts its UI work (a picker, a chooser, the print
// dialog, a permission prompt) to the main thread through the host's current
// ComponentActivity and waits on a latch for the answer. On the main thread
// it refuses (MainThreadCallException → `failed: main thread`).
package at.exponential.engine.capabilities

import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContract
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/** The main looper. */
object AndroidMainThread : MainThreadCheck {
    override fun isMainThread(): Boolean = Looper.myLooper() == Looper.getMainLooper()
}

/** The host's UI: the activity a blocking call shows its platform UI over. */
interface ActivityHost {
    /** Any context of the app (content resolver, system services). */
    val context: Context

    /**
     * Run [block] on the main thread with the current activity and block the
     * caller until it calls `done` (once; later calls are ignored). Throws
     * [MainThreadCallException] on the main thread and [ToolFailure] when there
     * is no activity or it goes away first.
     */
    fun <T> await(block: (activity: ComponentActivity, done: (Result<T>) -> Unit) -> Unit): T
}

/**
 * The [ActivityHost] over the app's foreground ComponentActivity:
 * `ComponentActivityHost(applicationContext) { currentActivity }`. A pending
 * call fails (`failed: activity gone`) when that activity is destroyed,
 * including a configuration change (hosts that keep a picker open across a
 * rotation declare `android:configChanges`).
 */
class ComponentActivityHost(
    override val context: Context,
    private val activity: () -> ComponentActivity?,
) : ActivityHost {
    private val main = Handler(Looper.getMainLooper())

    override fun <T> await(block: (activity: ComponentActivity, done: (Result<T>) -> Unit) -> Unit): T {
        if (AndroidMainThread.isMainThread()) throw MainThreadCallException()
        val latch = CountDownLatch(1)
        val answer = AtomicReference<Result<T>>()
        val finished = AtomicBoolean(false)
        main.post {
            val current = activity()?.takeIf { !it.isFinishing && !it.isDestroyed }
            if (current == null) {
                answer.set(Result.failure(ToolFailure.failed("no activity")))
                latch.countDown()
                return@post
            }
            var observer: LifecycleEventObserver? = null
            val done: (Result<T>) -> Unit = { result ->
                if (finished.compareAndSet(false, true)) {
                    answer.set(result)
                    latch.countDown()
                    main.post { observer?.let { current.lifecycle.removeObserver(it) } }
                }
            }
            observer = LifecycleEventObserver { _, event ->
                if (event == Lifecycle.Event.ON_DESTROY) done(Result.failure(ToolFailure.failed("activity gone")))
            }
            current.lifecycle.addObserver(observer)
            try {
                block(current, done)
            } catch (e: Exception) {
                done(Result.failure(e))
            }
        }
        latch.await()
        return answer.get().getOrThrow()
    }
}

private val launchKeys = AtomicInteger()

/** Launch an Activity Result contract on the host and block for its output. */
fun <I, O> ActivityHost.launch(contract: ActivityResultContract<I, O>, input: I): O = await { activity, done ->
    val key = "at.exponential.engine.capabilities.${launchKeys.incrementAndGet()}"
    var launcher: androidx.activity.result.ActivityResultLauncher<I>? = null
    launcher = activity.activityResultRegistry.register(key, contract) { output ->
        launcher?.unregister()
        done(Result.success(output))
    }
    launcher.launch(input)
}

/** Ask for runtime permissions (only those not granted yet): true = any of [anyOf] is granted after. */
fun ActivityHost.requestAnyPermission(anyOf: List<String>): Boolean {
    if (anyOf.any { granted(it) }) return true
    val result = launch(androidx.activity.result.contract.ActivityResultContracts.RequestMultiplePermissions(), anyOf.toTypedArray())
    return result.values.any { it } || anyOf.any { granted(it) }
}

internal fun ActivityHost.granted(permission: String): Boolean =
    androidx.core.content.ContextCompat.checkSelfPermission(context, permission) == android.content.pm.PackageManager.PERMISSION_GRANTED
