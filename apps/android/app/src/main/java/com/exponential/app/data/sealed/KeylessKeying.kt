package com.exponential.app.data.sealed

import com.exponential.app.data.api.RequestKeyResult
import com.exponential.app.data.api.TrpcException
import kotlinx.coroutines.delay
import kotlin.coroutines.cancellation.CancellationException

/** What a writer needs to key an EXISTING keyless root (the app's [SealedWriter], a fake in tests). */
internal interface KeylessApi {
    /** #EXP-2123: whether this install makes chat keys (joined). */
    suspend fun makesKeys(): Boolean

    /** `sessionKeys.create` of a fresh first key; null = this install cannot make it; throws the server's refusal. */
    suspend fun create(rootId: String, ownOnly: Boolean): PublicTreeKey?

    /** `sessionKeys.requestKey`: the tree's devices asked to key it; throws the server's refusal. */
    suspend fun requestKey(rootId: String): RequestKeyResult

    /** The root's key once one exists (Room first, else the server); null = none yet. */
    suspend fun currentKey(rootId: String): PublicTreeKey?

    /** The server's key of the root; null = keyless. */
    suspend fun publicKey(rootId: String): PublicTreeKey?
}

/** How a keyless root was keyed. */
internal sealed interface Keyless {
    /** `clear_tree`: plaintext, never keyed. */
    data object Clear : Keyless

    /** This install made [key]. */
    data class Created(val key: PublicTreeKey) : Keyless

    /** Keyed by another (its device, a concurrent writer); null = no key: the send fails. */
    data class Arrived(val key: PublicTreeKey?) : Keyless
}

/**
 * #EXP-1960 (sealed-content.json `clearTree.keylessCases`): a keyless root
 * keyed by this install, else by its devices (`sessionKeys.requestKey`).
 * #EXP-2123 (install-trust.json `keyMaker`): an install that waits for
 * approval (by its own store or by the server's [KEY_MAKER_REFUSAL]) creates
 * nothing: its devices are asked, the send seals to their key or fails.
 */
internal class KeylessKeying(
    private val api: KeylessApi,
    private val keyWaitMs: Long = SEAL_KEY_WAIT_MS,
    private val pollMs: Long = KEY_POLL_MS,
    private val retryMs: Long = SEAL_CREATE_RETRY_MS,
) {
    suspend fun key(rootId: String): Keyless {
        if (keyMakerStep(api.makesKeys(), KeyWrite.FirstKey) != KeyMakerStep.Make) return askDevice(rootId)
        var ownOnly = false
        var attempt = 1
        while (true) {
            val failure = try {
                val created = api.create(rootId, ownOnly) ?: return Keyless.Arrived(null)
                return Keyless.Created(created)
            } catch (err: CancellationException) {
                throw err
            } catch (err: TrpcException) {
                // The server lists this install as waiting: its devices are asked.
                if (isKeyMakerRefusal(err.message)) CreateFailure.NotReader else createFailure(err.code, err.message)
            } catch (err: Exception) {
                CreateFailure.Transient
            }
            when (keylessStep(failure, attempt, ownOnly)) {
                KeylessStep.Plain -> return Keyless.Clear
                KeylessStep.ServerKey -> return quietly { api.publicKey(rootId) }?.let { Keyless.Arrived(it) } ?: askDevice(rootId)
                KeylessStep.RequestKey -> return askDevice(rootId)
                KeylessStep.RetryOwn -> ownOnly = true
                KeylessStep.Retry -> delay(retryMs * attempt)
                else -> return Keyless.Arrived(null)
            }
            attempt++
        }
    }

    /** The tree's devices asked to key it, then its key waited for; no host = fails at once. */
    private suspend fun askDevice(rootId: String): Keyless {
        var answer = RequestKeyAnswer.Error
        val result = try {
            api.requestKey(rootId)
        } catch (err: CancellationException) {
            throw err
        } catch (err: Exception) {
            if (err is TrpcException && err.code == "NOT_FOUND") answer = RequestKeyAnswer.NotFound
            null
        }
        if (result != null) {
            answer = RequestKeyAnswer.entries.firstOrNull { it.name.replaceFirstChar(Char::lowercase) == result.state } ?: RequestKeyAnswer.Error
        }
        return when (keylessStep(answer)) {
            KeylessStep.Plain -> Keyless.Clear
            KeylessStep.Failed -> Keyless.Arrived(null)
            KeylessStep.Sealed -> Keyless.Arrived(
                result?.let { r -> r.publicKey?.let { key -> r.epoch?.let { PublicTreeKey(key, it) } } },
            )
            else -> Keyless.Arrived(awaitWithin(keyWaitMs, pollMs) { quietly { api.currentKey(rootId) } })
        }
    }

    private suspend fun <T> quietly(block: suspend () -> T?): T? =
        try {
            block()
        } catch (err: CancellationException) {
            throw err
        } catch (_: Exception) {
            null
        }

    companion object {
        /** How often a send waiting for a keyless tree's key asks again (Room, then the server). */
        const val KEY_POLL_MS = 2_000L
    }
}
