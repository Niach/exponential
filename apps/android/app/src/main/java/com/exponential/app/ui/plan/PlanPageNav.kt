package com.exponential.app.ui.plan

import androidx.compose.runtime.staticCompositionLocalOf

/**
 * #EXP-1493 → #EXP-1662 WAYS IN ×4 (plan-page.json): every way in (the
 * Conversation's plan row, a Needs you row, an inbox row or push of a
 * waiting plan) opens the plan tab of the thread holding one plan card
 * (`messageId`) a session (`sessionId`) posted. [AppNavHost] provides it (the
 * thread's route), a [ThreadScreen] over it for its own plans (the tab in
 * place); a host without a nav graph (the styleguide) leaves it a no-op.
 */
fun interface OpenPlanPage {
    fun open(sessionId: String, messageId: String)
}

val LocalOpenPlanPage = staticCompositionLocalOf { OpenPlanPage { _, _ -> } }
