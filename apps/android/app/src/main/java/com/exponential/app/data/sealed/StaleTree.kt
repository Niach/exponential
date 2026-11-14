package com.exponential.app.data.sealed

import com.exponential.app.data.api.TrpcException
import kotlin.coroutines.cancellation.CancellationException

/**
 * #FEED-250 (fixtures/sealed-content.json `staleTree`, the web's
 * `STALE_TREE_REFUSAL`): the server's CONFLICT for a key write into a tree
 * that moved on behind this install's Room.
 */
const val STALE_TREE_REFUSAL = "This chat's encryption key changed. Try again in a moment."

private val OLD_STALE_TREE = Regex("^The tree is at epoch \\d+$")

/** [STALE_TREE_REFUSAL], or an older server's `The tree is at epoch N`. */
fun isStaleTreeRefusal(message: String?): Boolean =
    message != null && (message == STALE_TREE_REFUSAL || OLD_STALE_TREE.matches(message))

/** A tree's current public key. */
internal data class PublicTreeKey(val publicKey: String, val epoch: Int)

/** The root Room has for a send (its key, when Room synced one). */
internal data class LocalRoot(val rootId: String, val key: PublicTreeKey?)

/** `sessions.sendTarget`: the caller's conversation root with an agent / app (null = none yet) and its key. */
internal data class ServerTree(val rootId: String?, val key: PublicTreeKey?)

/** How a send's text reaches its tree. */
internal sealed interface TreeWire {
    /** Sealed to [key] of the existing root [rootId]. */
    data class Sealed(val rootId: String, val key: PublicTreeKey) : TreeWire

    /** An existing keyless root: `sessionKeys.create` first, then sealed. */
    data class Create(val rootId: String) : TreeWire

    /** A root the send creates: its first key rides inline (`seal`, epoch 1). */
    data object Inline : TreeWire

    /** The tree cannot be told: the plaintext send of before. */
    data object Unknown : TreeWire
}

/**
 * The pure rule of `staleTree` (the web's `sealSendInput` / `sendSealed`):
 * Room is a CACHE of the tree, never its authority.
 */
internal object StaleTree {
    /**
     * The tree a send writes into. [conversation] = a send to an agent / app
     * (no session id); [explicitId] = the session / parent id it names;
     * [local] = Room's root; [fresh] = the server refused a key write once:
     * resolve from the server alone.
     */
    suspend fun resolve(
        conversation: Boolean,
        explicitId: String?,
        local: LocalRoot?,
        fresh: Boolean,
        sendTarget: suspend () -> ServerTree?,
        publicKey: suspend (rootId: String) -> PublicTreeKey?,
    ): TreeWire {
        val target = if (conversation && (local == null || fresh)) quietly { sendTarget() } else null
        if (target?.rootId != null) return target.key?.let { TreeWire.Sealed(target.rootId, it) } ?: TreeWire.Create(target.rootId)
        if (local != null && !(target != null && fresh)) {
            val key = if (fresh) quietly { publicKey(local.rootId) } ?: local.key else local.key
            return key?.let { TreeWire.Sealed(local.rootId, it) } ?: TreeWire.Create(local.rootId)
        }
        if (explicitId != null && !conversation) {
            // A session Room does not have: its key, when the server names one.
            if (!fresh) return TreeWire.Unknown
            return quietly { publicKey(explicitId) }?.let { TreeWire.Sealed(explicitId, it) } ?: TreeWire.Unknown
        }
        return TreeWire.Inline
    }

    /**
     * A keyless root keyed by this install ([create]: null = it cannot make
     * the key); refused (keyed meanwhile) = the server's key, else null
     * (plaintext stays legal).
     */
    suspend fun keyFirst(rootId: String, create: suspend () -> PublicTreeKey?, publicKey: suspend (rootId: String) -> PublicTreeKey?): PublicTreeKey? =
        quietly { create() } ?: quietly { publicKey(rootId) }

    /**
     * A send whose key write the server refused as stale re-resolves the
     * tree ([attempt] with `fresh = true`) and goes again ONCE, without
     * surfacing the first refusal; a second one fails with [STALE_TREE_REFUSAL],
     * never a raw epoch.
     */
    suspend fun <R> sendOnce(attempt: suspend (fresh: Boolean) -> R): R {
        try {
            return attempt(false)
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            if (!isStaleTreeRefusal(err.message)) throw err
        }
        try {
            return attempt(true)
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            if (!isStaleTreeRefusal(err.message)) throw err
            throw TrpcException(STALE_TREE_REFUSAL, status = (err as? TrpcException)?.status, code = (err as? TrpcException)?.code)
        }
    }

    private suspend fun <T> quietly(block: suspend () -> T?): T? =
        try {
            block()
        } catch (err: CancellationException) {
            throw err
        } catch (_: Throwable) {
            null
        }
}
