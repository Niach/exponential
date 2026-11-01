package com.exponential.app.ui.plan

import androidx.compose.runtime.staticCompositionLocalOf

/**
 * #EXP-1493 WAYS IN ×4 (plan-page.json): every way in (a story's Open plan,
 * the chat plan card's header action, the Dock sheet's `ui-fullscreen`
 * glyph, an inbox row of a waiting plan) pushes THE plan page of one plan
 * card (`messageId`) a session (`sessionId`) posted. [AppNavHost] provides
 * it; a host without a nav graph (the styleguide) leaves it a no-op.
 */
fun interface OpenPlanPage {
    fun open(sessionId: String, messageId: String)
}

val LocalOpenPlanPage = staticCompositionLocalOf { OpenPlanPage { _, _ -> } }
