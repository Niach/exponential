package com.exponential.app.ui.styleguide

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import com.exponential.app.domain.InstallApprovalView
import com.exponential.app.ui.chats.NeedsYouPageContent
import com.exponential.app.ui.session.PendingInstalls
import com.exponential.app.ui.theme.AppBackground

/*
 * #EXP-1950 capture-only specimens (`install-approval` · `install-waiting`,
 * shots sg_install-approval / sg_install-waiting; web `?specimen=`, iOS
 * `InstallTrustSpecimens`): the fixture's keys N (New install) and T3
 * (Reset), and N waiting with its code, over THE Needs you page content.
 * InstallTrustFixtureTest holds the rows to install-trust.json.
 */

const val InstallApprovalSpecimenTestId = "install-approval-specimen"
const val InstallWaitingSpecimenTestId = "install-waiting-specimen"

@Composable
fun InstallTrustSpecimenScreen(waiting: Boolean) {
    val rows = InstallApprovalView.SPECIMEN_ROWS
    AppBackground(Modifier.testTag(if (waiting) InstallWaitingSpecimenTestId else InstallApprovalSpecimenTestId)) {
        NeedsYouPageContent(
            rows = emptyList(),
            onBack = {},
            onOpen = {},
            onOpenInbox = {},
            inboxUnread = 1,
            waitingCode = if (waiting) rows[0].code else null,
            pendingInstalls = if (waiting) 0 else rows.size,
            band = { PendingInstalls(rows = rows, onApprove = {}, onRemove = {}) },
        )
    }
}
