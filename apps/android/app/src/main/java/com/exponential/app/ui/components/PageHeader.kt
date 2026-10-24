package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.Chats
import com.exponential.app.ui.icons.ExpIcons

// #EXP-1384 (chats.json `shell.header`, owner 2026-10-24): THE phone page
// header on every top-level page (`header.pages`: Board, Chats, Apps, Team,
// Devices, Agent): the large in-content title on the left and the round
// trailing circles `header.buttons`, SEARCH (pushes the phone search) then
// the INBOX (concept nav-inbox, a dot while a notification of the active team
// is unread; pushes the inbox-only page). A pushed page (Devices, from the
// Team page) puts the back control before the title. Web `@exp/ui`
// `PageHeader` is the twin; iOS paints the native large title + toolbar
// circles.

/** What the header's two circles do — provided ONCE by the nav shell, so a
 *  page only names its title. */
data class PageHeaderActions(
    val onSearch: () -> Unit,
    val onInbox: () -> Unit,
    /** A notification of the active team is unread: the inbox dot. */
    val inboxUnread: Boolean,
)

/** Null outside the nav shell (previews, specimens): the circles no-op. */
val LocalPageHeaderActions = compositionLocalOf<PageHeaderActions?> { null }

@Composable
fun PageHeader(
    title: String,
    modifier: Modifier = Modifier,
    /** A pushed page's back control, drawn before the title. */
    onBack: (() -> Unit)? = null,
    actions: PageHeaderActions? = LocalPageHeaderActions.current,
) {
    val circle = Chats.Shell.Header.CIRCLE.dp
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        modifier = modifier
            .fillMaxWidth()
            .padding(start = if (onBack != null) 8.dp else 16.dp, end = 16.dp, top = 8.dp, bottom = 8.dp)
            .testTag(Chats.TestIds.PAGE_HEADER),
    ) {
        if (onBack != null) TopBarBackButton(onClick = onBack)
        Text(
            title,
            style = MaterialTheme.typography.headlineLarge.copy(
                fontWeight = FontWeight.Bold,
                fontSize = 34.sp,
                lineHeight = 40.sp,
            ),
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Chats.Shell.Header.BUTTONS.forEach { concept ->
            when (concept) {
                "nav-search" -> CircleIconButton(
                    ExpIcons.navSearch,
                    contentDescription = "Search",
                    onClick = { actions?.onSearch?.invoke() },
                    size = circle,
                    glyphSize = 20.dp,
                    modifier = Modifier.testTag(Chats.TestIds.SEARCH_BUTTON),
                )
                "nav-inbox" -> Box {
                    CircleIconButton(
                        ExpIcons.navInbox,
                        contentDescription = Chats.Copy.INBOX,
                        onClick = { actions?.onInbox?.invoke() },
                        size = circle,
                        glyphSize = 20.dp,
                        modifier = Modifier.testTag(Chats.TestIds.INBOX_BUTTON),
                    )
                    if (actions?.inboxUnread == true) {
                        Box(
                            Modifier
                                .align(Alignment.TopEnd)
                                .offset(x = (-3).dp, y = 3.dp)
                                .size(10.dp)
                                .background(ChatsInboxDot, CircleShape)
                                .testTag("chats-inbox-dot"),
                        )
                    }
                }
            }
        }
    }
}
