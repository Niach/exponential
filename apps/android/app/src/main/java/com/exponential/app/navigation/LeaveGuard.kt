package com.exponential.app.navigation

import androidx.compose.runtime.staticCompositionLocalOf

/**
 * EXP-1212: the ONE choke point a page may HOLD navigation at. The New issue
 * page registers while it is up; every navigation AppNavHost starts on its
 * own (deep links, push taps, shares, entity previews) goes through
 * [navigate]. The holder either lets it pass (returns false) or keeps the
 * `proceed` and runs it later, or never (the user stayed).
 *
 * Back and the page's own exits (Create, a confirmed Discard) never come
 * here: the page owns those.
 */
class LeaveGuard {
    private var holder: ((proceed: () -> Unit) -> Boolean)? = null

    /** Hold navigation while registered; the returned call unregisters. */
    fun register(hold: (proceed: () -> Unit) -> Boolean): () -> Unit {
        holder = hold
        return { if (holder === hold) holder = null }
    }

    /**
     * Run [navigation] now, or hand it to the holder. A held navigation runs
     * [beforeHeld] first (dropping the page that held it, so Back never
     * returns to a page that has already left).
     */
    fun navigate(beforeHeld: () -> Unit = {}, navigation: () -> Unit) {
        val hold = holder ?: return navigation()
        if (!hold { beforeHeld(); navigation() }) navigation()
    }
}

/** The app's guard, provided over the authenticated NavHost. */
val LocalLeaveGuard = staticCompositionLocalOf<LeaveGuard?> { null }
