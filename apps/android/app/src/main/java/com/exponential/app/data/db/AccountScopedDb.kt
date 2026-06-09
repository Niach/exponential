package com.exponential.app.data.db

import com.exponential.app.data.auth.AuthRepository
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map

/**
 * Reactive account scoping for per-screen ViewModels.
 *
 * ViewModels used to snapshot `auth.activeAccountId.value` + the Room instance
 * at construction, which raced account switches (the nav shell papered over it
 * with a `key(activeAccountId)` rebuild). Instead, derive the active account's
 * database as a Flow: every query chain hangs off it via [scopedQuery] /
 * `flatMapLatest`, so an account switch transparently re-scopes all live data
 * with no rebuild and no pending-handoff plumbing.
 */
fun accountDatabaseFlow(auth: AuthRepository, holder: DatabaseHolder): Flow<ExponentialDatabase?> =
    auth.activeAccountId
        .map { id -> id?.takeIf { it.isNotBlank() }?.let { holder.database(forAccountId = it) } }
        .distinctUntilChanged()

/**
 * Observe a query against the active account's DB, emitting [empty] while no
 * account is active and re-subscribing on account switch.
 */
@OptIn(ExperimentalCoroutinesApi::class)
fun <T> Flow<ExponentialDatabase?>.scopedQuery(
    empty: T,
    query: (ExponentialDatabase) -> Flow<T>,
): Flow<T> = flatMapLatest { db -> if (db == null) flowOf(empty) else query(db) }
