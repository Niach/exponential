package com.exponential.app.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay

/** How long typing has to settle before a search surface asks the server. */
const val SERVER_SEARCH_DEBOUNCE_MS = 250L

/**
 * EXP-892 — the debounced server half of every issue search: the hits for
 * [query], or an empty list while none have landed FOR THAT QUERY.
 *
 * The rules are the Search tab's, in one place so no surface can drift from
 * it: a response is pinned to the query it answered (a slow one can never be
 * merged under a fresher query), a new query cancels the in-flight round trip,
 * and any failure degrades silently to local-only — typing never waits on the
 * network. [search] null (a host with no API to call) means no hits, ever.
 *
 * [query] is null while the surface is shut.
 */
@Composable
internal fun <H> rememberServerSearchHits(
    query: String?,
    search: (suspend (String) -> List<H>)?,
): List<H> {
    var landed by remember { mutableStateOf<Pair<String, List<H>>?>(null) }
    val current by rememberUpdatedState(search)
    // Keyed on the QUERY (and on whether there is anything to call at all),
    // never on the lambda's identity: the handler carrying it is rebuilt on
    // every Electric tick, and that must not re-fire the round trip.
    LaunchedEffect(query, search != null) {
        val call = current ?: return@LaunchedEffect
        if (query == null) return@LaunchedEffect
        val trimmed = query.trim()
        if (trimmed.isEmpty() || landed?.first == trimmed) return@LaunchedEffect
        delay(SERVER_SEARCH_DEBOUNCE_MS)
        val hits = try {
            call(trimmed)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            return@LaunchedEffect
        }
        landed = trimmed to hits
    }
    return landed?.takeIf { it.first == query?.trim() }?.second.orEmpty()
}
