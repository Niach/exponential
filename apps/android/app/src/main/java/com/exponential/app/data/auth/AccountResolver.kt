package com.exponential.app.data.auth

/** New account list + active id produced by [resolveAccounts]. */
internal data class AccountResolution(
    val accounts: List<ServerAccount>,
    val activeId: String,
)

/**
 * Pure state machine behind [AccountStore.resolveActiveAccount]. A login has
 * just resolved a `userId` for `instanceUrl`; fold that into the stored
 * accounts keeping data per-user:
 *
 *  - **Same user, already resolved** (an account with the per-user id exists):
 *    update its token/fields in place — no wipe, its cached DB keeps syncing.
 *  - **New user on this server** (no per-user account yet): drop the pending
 *    URL-only record and add a fresh per-user account, so the new user gets a
 *    fresh DB file (`exponential-<perUserId>-v2.db`) — the first user's file,
 *    under its own id, is untouched.
 *
 * The pending URL-only record never carried a token, so no pipeline ever wrote
 * to its DB; discarding it loses nothing. Active is always the per-user id.
 */
internal fun resolveAccounts(
    accounts: List<ServerAccount>,
    instanceUrl: String,
    userId: String,
    token: String,
    email: String?,
    name: String?,
    isAdmin: Boolean,
    onboardingCompletedAt: String?,
    onboardingKnown: Boolean,
    now: Long,
): AccountResolution {
    val pendingId = ServerAccount.makeId(instanceUrl)
    val resolvedId = ServerAccount.makeId(instanceUrl, userId)
    val existingResolved = accounts.firstOrNull { it.id == resolvedId }

    val merged = (existingResolved ?: ServerAccount(id = resolvedId, instanceUrl = instanceUrl)).copy(
        id = resolvedId,
        instanceUrl = instanceUrl,
        token = token,
        userEmail = email,
        userName = name,
        userId = userId,
        isAdmin = isAdmin,
        onboardingCompletedAt = onboardingCompletedAt,
        onboardingKnown = onboardingKnown,
        lastUsedAt = now,
    )

    // Keep every other account; replace the (possibly pre-existing) resolved
    // record and drop the pending URL-only one this login consumed.
    val kept = accounts.filter { it.id != resolvedId && it.id != pendingId }
    return AccountResolution(accounts = kept + merged, activeId = resolvedId)
}

/**
 * The legacy URL-keyed DB id to wipe for [account] during the one-shot per-user
 * cleanup, or null to keep it. Wipe when:
 *  - the account was re-keyed to a per-user id (its old URL-keyed DB may hold
 *    another user's cache), or
 *  - the migration nulled its token because the userId was unknown — that
 *    account is STILL keyed by the URL-only id, so its DB is the exact
 *    wrong-user cache this cleanup exists to destroy. Safe to wipe: a tokenless
 *    account runs no pipeline, and a future login re-keys it to a fresh
 *    per-user DB.
 */
internal fun legacyDbIdToWipe(account: ServerAccount): String? {
    val urlOnlyId = ServerAccount.makeId(account.instanceUrl)
    return if (account.id != urlOnlyId || account.token == null) urlOnlyId else null
}
