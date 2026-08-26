package com.exponential.app

import android.Manifest
import android.os.Build
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.espresso.Espresso
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import tools.fastlane.screengrab.Screengrab
import tools.fastlane.screengrab.UiAutomatorScreenshotStrategy

/**
 * Shared driving machinery for the screengrab capture suites
 * ([StoreScreenshotsTest] and [StyleguideScreenshotsTest]).
 *
 * Both suites drive the REAL app UI end-to-end against a locally seeded backend
 * (apps/web/scripts/seed-screenshots.ts), so they share the instance picker →
 * password login flow, the polling helpers and the diff expansion.
 *
 * Synchronization notes:
 * - [waitFor] polls semantics without requiring Compose idleness, so infinite
 *   animations (indeterminate progress spinners while auth-config / Electric
 *   sync load) cannot hang it — this is why every step gates on content
 *   appearing rather than on waitForIdle.
 * - Screenshots use [UiAutomatorScreenshotStrategy]: the default reflection
 *   -based strategy renders a blank window for Compose surfaces.
 */
class ScreenshotFlow(private val composeRule: ComposeTestRule) {

    companion object {
        const val DEFAULT_INSTANCE_URL = "http://10.0.2.2:5173"
        const val DEMO_EMAIL = "demo@exponential.at"
        const val DEMO_PASSWORD = "screenshots-demo"

        /**
         * The seed's SECOND identity (EXP-627): verified, member of nothing,
         * with a null `onboardingCompletedAt`. Signing in as them is the only
         * way to photograph the first-run wizard — the demo user finished it
         * long ago and is bounced straight to a board. Accounts are keyed per
         * (instance, userId) — `ServerAccount.makeId` — so "Add server" pointed
         * at the SAME instance adds the newcomer ALONGSIDE the demo account
         * rather than replacing it (and `AccountDeduplicator` only prunes
         * TOKENLESS duplicates, so the signed-in demo row survives). Keep in
         * lockstep with `apps/web/scripts/screenshot-demo.ts`.
         */
        const val NEWCOMER_EMAIL = "newcomer@exponential.at"
        const val NEWCOMER_PASSWORD = "screenshots-newcomer"

        /** APP-5 — the seeded showcase issue (description + the only comment thread). */
        const val SHOWCASE_ISSUE_TITLE = "Reduce cold start below 800 ms"

        /** InstanceScreen's demoted self-hosting entry (EXP-14). */
        const val SELF_HOST_LINK = "Use a self-hosted instance"

        const val NAV_TIMEOUT = 30_000L
        const val SYNC_TIMEOUT = 60_000L

        /**
         * Backend the emulator talks to. `instanceUrl` is passed through by the
         * fastlane lanes from SCREENGRAB_INSTANCE_URL.
         */
        fun instanceUrl(): String =
            InstrumentationRegistry.getArguments().getString("instanceUrl") ?: DEFAULT_INSTANCE_URL

        /**
         * Pre-grant POST_NOTIFICATIONS (SDK 33+) so MainActivity's permission
         * request never shows a system dialog over the screenshots. Must be
         * chained OUTSIDE the compose rule so the grant lands before the
         * activity (and its permission request) launches.
         */
        fun permissionRule(): GrantPermissionRule =
            if (Build.VERSION.SDK_INT >= 33) {
                GrantPermissionRule.grant(Manifest.permission.POST_NOTIFICATIONS)
            } else {
                GrantPermissionRule.grant()
            }

        /** Compose surfaces only render into a screenshot via UiAutomator. */
        fun useUiAutomatorScreenshots() {
            Screengrab.setDefaultScreenshotStrategy(UiAutomatorScreenshotStrategy())
        }

        /**
         * The optional per-run shot allowlist (EXP-642).
         *
         * `bundle exec fastlane screenshots shots:1_board,2_issue-detail`
         * appends `shots <ids>` to screengrab's launch arguments, which arrive
         * here as the `shots` instrumentation argument. Unset = capture
         * everything, which is what a plain lane run does.
         *
         * Only the CAPTURE is skipped, never the navigation: the suites are one
         * long scripted walk through the app, and skipping a tap would strand
         * every later shot on the wrong screen. A subset run is therefore not
         * faster, only narrower — exactly what the automation needs when a diff
         * touched two views.
         */
        private val requestedShots: Set<String>? by lazy {
            InstrumentationRegistry.getArguments().getString("shots")
                ?.split(',', ' ', '\n')
                ?.map { it.trim() }
                ?.filter { it.isNotEmpty() }
                ?.toSet()
                ?.takeIf { it.isNotEmpty() }
        }

        /** Every id a suite actually reached — the typo check compares it. */
        private val offeredShots = mutableSetOf<String>()

        /** Records [name] as reached and reports whether to capture it. */
        fun isShotWanted(name: String): Boolean {
            offeredShots += name
            return requestedShots?.contains(name) ?: true
        }

        /**
         * Requested ids the suite never reached — a misspelt `shots:` value, or
         * a name from the other lane. Almost always a typo, which would
         * otherwise look like a perfectly green empty run.
         */
        fun unreachedShots(): List<String> =
            requestedShots?.minus(offeredShots)?.sorted() ?: emptyList()
    }

    /**
     * Capture [name] — unless it is outside this run's `shots` allowlist, in
     * which case nothing is written.
     *
     * Every capture in both suites goes through this wrapper rather than
     * `Screengrab.screenshot` directly, so the allowlist can never be bypassed
     * by accident. The literal `screenshot("…"` at each call site is
     * load-bearing: `packages/view-catalog/src/views.test.ts` greps for it to
     * prove the suites and the catalog list the same shots.
     *
     * [popRects] additionally writes the store compositor's pop-out rect
     * sidecar for this shot (EXP-627, see [PopRects]) — the store lane only.
     */
    fun screenshot(name: String, popRects: Boolean = false) {
        if (!isShotWanted(name)) {
            android.util.Log.i("EXP-642", "shots: skipping $name — not in the `shots` allowlist")
            return
        }
        if (popRects) PopRects.dump(composeRule, name)
        Screengrab.screenshot(name)
    }

    /**
     * Waits out the cold launch until the instance picker is on screen,
     * touching NOTHING. Split out of [chooseInstance] (EXP-566) so the
     * styleguide suite can photograph the untouched picker before the sign-in
     * flow starts driving it.
     */
    fun awaitInstancePicker() {
        waitFor(hasText(SELF_HOST_LINK), NAV_TIMEOUT)
    }

    /**
     * Instance picker: cloud is the primary path (EXP-14), so reveal the
     * self-hosted URL field, then point the app at the seeded backend.
     */
    fun chooseInstance(instanceUrl: String) {
        awaitInstancePicker()
        composeRule.onNode(hasText(SELF_HOST_LINK)).performClick()
        waitFor(hasTestTag("instance-url-field"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("instance-url-field")).apply {
            performTextClearance()
            performTextInput(instanceUrl)
        }
        composeRule.onNode(hasText("Continue")).performClick()
    }

    /**
     * Waits for the login screen to finish resolving /api/auth-config — the
     * email field only exists once it has.
     */
    fun awaitLoginScreen() {
        waitFor(hasTestTag("login-email-field"), NAV_TIMEOUT)
    }

    /** Fills in the demo credentials and submits. */
    fun submitLogin(email: String = DEMO_EMAIL, password: String = DEMO_PASSWORD) {
        composeRule.onNode(hasTestTag("login-email-field")).performTextInput(email)
        composeRule.onNode(hasTestTag("login-password-field")).performTextInput(password)
        Espresso.closeSoftKeyboard()
        composeRule.onNode(hasTestTag("login-submit-button")).performClick()
    }

    /** Instance picker → login → submit, i.e. the whole cold-start sign-in. */
    fun signIn(instanceUrl: String) {
        chooseInstance(instanceUrl)
        awaitLoginScreen()
        submitLogin()
    }

    /**
     * Expands the diff's file sections so the review shot shows patches rather
     * than a bare filename list. Every file starts collapsed; the rows never
     * reorder, so index-addressing them is stable across the clicks.
     */
    fun expandDiffFiles() {
        val rows = composeRule.onAllNodes(hasTestTag("changes-file-row"))
        val count = minOf(rows.fetchSemanticsNodes().size, 5)
        for (index in 0 until count) {
            runCatching { rows[index].performClick() }
        }
    }

    /** Poll (without requiring Compose idleness) until [matcher] matches a node. */
    fun waitFor(matcher: SemanticsMatcher, timeoutMillis: Long) {
        composeRule.waitUntil(timeoutMillis) {
            composeRule.onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty()
        }
    }

    /**
     * Like [waitFor], but REPORTS the timeout instead of failing the run — for
     * the handful of gates whose content is optional (a seed that predates the
     * shot, an empty state that is itself worth photographing).
     */
    fun waitForOptional(matcher: SemanticsMatcher, timeoutMillis: Long): Boolean =
        runCatching { waitFor(matcher, timeoutMillis) }.isSuccess

    /** Poll until no node matches [matcher] anymore. */
    fun waitForGone(matcher: SemanticsMatcher, timeoutMillis: Long) {
        composeRule.waitUntil(timeoutMillis) {
            composeRule.onAllNodes(matcher).fetchSemanticsNodes().isEmpty()
        }
    }

    /** True if [matcher] currently matches at least one node (no waiting). */
    fun exists(matcher: SemanticsMatcher): Boolean =
        composeRule.onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty()

    /** Let animations / async images finish before capturing. */
    fun settle(longer: Boolean = false) {
        Thread.sleep(if (longer) 2_000 else 1_000)
    }
}
