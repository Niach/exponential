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

/**
 * The writer's refusal BEFORE it keys anything (#EXP-1955): only a missing key
 * holder (or install) fails up front. A keyless tree's [reader] / [keyArrives]
 * are only known after `sessionKeys.create` / the key wait, so that decision is
 * [sendWire]'s, made in the create branch, never here.
 */
fun refusesUpFront(tree: SendTree, holder: SendHolder, hasInstall: Boolean): Boolean =
    tree != SendTree.Clear && (holder == SendHolder.None || !hasInstall)

/** #EXP-1960 (`clearTree.keylessCases`): how often a blip on `sessionKeys.create` is retried, the backoff unit. */
const val SEAL_CREATE_ATTEMPTS = 3
const val SEAL_CREATE_RETRY_MS = 500L

/** The server's refusal of a wrap for an install that is not a reader. */
const val WRAP_REFUSAL = "A wrap names an install that is not a reader"

/** A failed `sessionKeys.create`. */
enum class CreateFailure { Clear, Stale, NotReader, WrapRefused, Refused, Transient }

/** `sessionKeys.requestKey`'s answer ([NotFound] = a real NOT_FOUND, [Error] = no answer / an older server). */
enum class RequestKeyAnswer { Keyed, Clear, Requested, NoHost, NotFound, Error }

/** What a writer does next with a keyless tree. Never a silent wait. */
enum class KeylessStep { Plain, ServerKey, RequestKey, RetryOwn, Retry, Failed, Sealed, Wait }

private val REFUSAL_CODES = setOf(
    "BAD_REQUEST", "UNAUTHORIZED", "FORBIDDEN", "NOT_FOUND", "CONFLICT", "PRECONDITION_FAILED",
    "PAYLOAD_TOO_LARGE", "UNPROCESSABLE_CONTENT", "METHOD_NOT_SUPPORTED",
)

/** A failed create read from its tRPC [code] + [message] (no code = no answer: a blip). */
fun createFailure(code: String?, message: String?): CreateFailure = when {
    message == CLEAR_TREE_REFUSAL -> CreateFailure.Clear
    code == "CONFLICT" || isStaleTreeRefusal(message) -> CreateFailure.Stale
    message == WRAP_REFUSAL -> CreateFailure.WrapRefused
    code == "NOT_FOUND" -> CreateFailure.NotReader
    code != null && code in REFUSAL_CODES -> CreateFailure.Refused
    else -> CreateFailure.Transient
}

/** #EXP-1960: the next step after a failed create (the TS `keylessStep`). */
fun keylessStep(failure: CreateFailure, attempt: Int = 1, ownOnly: Boolean = false): KeylessStep = when (failure) {
    CreateFailure.Clear -> KeylessStep.Plain
    CreateFailure.Stale -> KeylessStep.ServerKey
    CreateFailure.NotReader -> KeylessStep.RequestKey
    CreateFailure.WrapRefused -> if (ownOnly) KeylessStep.RequestKey else KeylessStep.RetryOwn
    CreateFailure.Refused -> KeylessStep.Failed
    CreateFailure.Transient -> if (attempt < SEAL_CREATE_ATTEMPTS) KeylessStep.Retry else KeylessStep.Failed
}

/** #EXP-1960: the next step once the tree's devices were asked. */
fun keylessStep(answer: RequestKeyAnswer): KeylessStep = when (answer) {
    RequestKeyAnswer.Keyed -> KeylessStep.Sealed
    RequestKeyAnswer.Clear -> KeylessStep.Plain
    RequestKeyAnswer.NoHost -> KeylessStep.Failed
    RequestKeyAnswer.Requested, RequestKeyAnswer.NotFound, RequestKeyAnswer.Error -> KeylessStep.Wait
}

// ── rotation (sealed-content.json `rotation`, #EXP-1951) ──────────────────

/** What a holder does with the answer of a rotation's `sessionKeys.create`. */
enum class RotationAfter { Adopt, Reread, Leave, Retry }

/**
 * The epoch a holder creates for a root that is due (`stepCases`), null =
 * nothing to rotate here: [flagged] = the root's `seal_rotate_epoch`,
 * [current] = its `seal_epoch` as this install knows it, [held] = this
 * install holds the current epoch's secret, [left] = the server refused this
 * (root, epoch) before.
 */
fun rotationEpoch(flagged: Int?, current: Int?, held: Boolean, left: Boolean = false): Int? =
    if (flagged == null || current == null || current != flagged || !held || left || current >= Int.MAX_VALUE) null else current + 1

/** `answerCases`: [failure] = [createFailure] of the rotation's create, null = created. */
fun rotationAfter(failure: CreateFailure?): RotationAfter = when (failure) {
    null -> RotationAfter.Adopt
    CreateFailure.Stale -> RotationAfter.Reread
    CreateFailure.Transient -> RotationAfter.Retry
    else -> RotationAfter.Leave
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
