package com.exponential.app.data.sealed

import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeoutOrNull

/**
 * #EDE-97 (fixtures/sealed-content.json `clearTree`, the TS `sendWire` +
 * `NO_KEY_REFUSAL`): every chat tree is sealed except a `clear_tree` root (an
 * outside MCP client's). NO SILENT PLAINTEXT: a writer that cannot seal fails
 * the send with [NO_KEY_REFUSAL] (the failed-send toast's description).
 */
const val NO_KEY_REFUSAL = "This chat is end-to-end encrypted and this device holds no key for it yet. Try again in a moment."

/** The server's refusal of a key for a tree an outside MCP client started. */
const val CLEAR_TREE_REFUSAL = "This chat stays in clear: an MCP client outside Exponential started it"

/** How long a writer waits for its key holder (the Keystore vault + this install's registration). */
const val SEAL_HOLDER_WAIT_MS = 20_000L

/** How long a writer waits for a keyless tree's key it may not make itself. */
const val SEAL_KEY_WAIT_MS = 20_000L

/** Where the text goes: a clear root, a root the send creates, a keyed or a keyless root. */
enum class SendTree { Clear, New, Keyed, Keyless }

/** This process's key holder: [Late] = it came up within [SEAL_HOLDER_WAIT_MS]. */
enum class SendHolder { Ready, Late, None }

/** What the text goes out as. */
enum class SendWire { Plain, Inline, Create, Sealed, Failed }

/**
 * The pure rule: only a clear tree takes plaintext; everything else seals or
 * fails. [reader] = this install may key the keyless tree; [keyArrives] = the
 * device's claim keyed it within [SEAL_KEY_WAIT_MS].
 */
fun sendWire(tree: SendTree, holder: SendHolder, reader: Boolean = false, keyArrives: Boolean = false): SendWire = when {
    tree == SendTree.Clear -> SendWire.Plain
    holder == SendHolder.None -> SendWire.Failed
    tree == SendTree.New -> SendWire.Inline
    tree == SendTree.Keyed -> SendWire.Sealed
    reader -> SendWire.Create
    keyArrives -> SendWire.Sealed
    else -> SendWire.Failed
}

/** [probe] until it answers non-null, every [pollMs], for at most [timeoutMs]; null = it never did. */
internal suspend fun <T : Any> awaitWithin(timeoutMs: Long, pollMs: Long, probe: suspend () -> T?): T? =
    withTimeoutOrNull(timeoutMs) {
        var found = probe()
        while (found == null) {
            delay(pollMs)
            found = probe()
        }
        found
    }
