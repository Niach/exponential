package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.HeaderActions
import com.exponential.app.domain.NavBadges
import com.exponential.app.domain.Chats
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

// #EXP-1384, #EXP-2109 (chats.json `shell.header`): THE phone page header.
// The root pages (`header.pages`: Chats, Teams, Settings) wear the large
// in-content title on the left and the round trailing circles
// `header.buttons` (the FAB chrome [BarCircle], web `FabButton`, at
// `header.circle`), SEARCH (pushes the phone search) then the INBOX (concept
// nav-inbox, the count badge = the unread notifications of every team; pushes
// the inbox page); [ownActions] (Teams' +) stand after them. A PUSHED
// page ([onBack]) wears the phone detail header: the back control, its leading
// glyph, the title, and its own trailing circle. Web `@exp/ui` `PageHeader` /
// `MobileDetailHeader` are the twins; iOS paints the native large title +
// toolbar circles.

/** What the header's two circles do — provided ONCE by the nav shell, so a
 *  page only names its title. */
data class PageHeaderActions(
    val onSearch: () -> Unit,
    val onInbox: () -> Unit,
    /** #FEED-232: every team's unread notifications: the inbox's
     *  muted count badge (0 = none). */
    val inboxUnread: Int,
)

/** Null outside the nav shell (previews, specimens): the circles no-op. */
val LocalPageHeaderActions = compositionLocalOf<PageHeaderActions?> { null }

@Composable
fun PageHeader(
    title: String,
    modifier: Modifier = Modifier,
    /** A pushed page's back control: the detail header. */
    onBack: (() -> Unit)? = null,
    /** A pushed page's glyph before its title (team mark, folder, board). */
    leading: (@Composable () -> Unit)? = null,
    /** The page's own circles, last (header-actions.json: Teams' +, a
     *  board's + and `…`). */
    ownActions: (@Composable () -> Unit)? = null,
    actions: PageHeaderActions? = LocalPageHeaderActions.current,
    /** EXP-1635: false = no Search + Inbox circles (a share's own step). */
    buttons: Boolean = true,
) {
    val circle = Chats.Shell.Header.CIRCLE.dp
    if (onBack != null) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            modifier = modifier
                .fillMaxWidth()
                .padding(start = 8.dp, end = 16.dp, top = 8.dp, bottom = 8.dp)
                .heightIn(min = circle)
                .testTag(Chats.TestIds.PAGE_HEADER),
        ) {
            TopBarBackButton(onClick = onBack)
            leading?.invoke()
            Text(
                title,
                style = MaterialTheme.typography.titleMedium.copy(fontSize = 17.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.sp),
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (ownActions != null) {
                Row(horizontalArrangement = Arrangement.spacedBy(HeaderActions.GAP.dp)) { ownActions() }
            }
        }
        return
    }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(HeaderActions.GAP.dp),
        modifier = modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 8.dp)
            .testTag(Chats.TestIds.PAGE_HEADER),
    ) {
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
        if (buttons) Chats.Shell.Header.BUTTONS.forEach { concept ->
            when (concept) {
                "nav-search" -> BarCircle(
                    onClick = { actions?.onSearch?.invoke() },
                    size = circle,
                    modifier = Modifier
                        .testTag(Chats.TestIds.SEARCH_BUTTON)
                        .semantics { contentDescription = "Search" },
                ) { HeaderGlyph(ExpIcons.navSearch) }
                "nav-inbox" -> Box {
                    BarCircle(
                        onClick = { actions?.onInbox?.invoke() },
                        size = circle,
                        modifier = Modifier
                            .testTag(Chats.TestIds.INBOX_BUTTON)
                            .semantics { contentDescription = Chats.Copy.INBOX },
                    ) { HeaderGlyph(ExpIcons.navInbox) }
                    // #FEED-232: THE count badge, muted, on the circle's corner.
                    CountBadge(
                        actions?.inboxUnread ?: 0,
                        NavBadges.Tone.Muted,
                        modifier = Modifier.align(Alignment.TopEnd).offset(x = 2.dp, y = (-2).dp),
                        testTag = "chats-inbox-badge",
                    )
                }
            }
        }
        ownActions?.invoke()
    }
}

/** THE round header button (header-actions.json): every action of a header but
 *  its status and its one labelled verb. */
@Composable
fun PageHeaderCircle(
    icon: ImageVector,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    /** #EXP-1878: the disabled look on a control that still takes its press. */
    dimmed: Boolean = false,
) {
    BarCircle(
        onClick = onClick,
        size = HeaderActions.CIRCLE.dp,
        enabled = enabled,
        modifier = modifier.semantics { contentDescription = description },
    ) { HeaderGlyph(icon, dimmed = dimmed || !enabled) }
}

/** A header circle's glyph: 20dp at full emphasis (web `FabButton emphasis="primary"`). */
@Composable
private fun HeaderGlyph(icon: ImageVector, dimmed: Boolean = false) {
    Icon(
        icon,
        contentDescription = null,
        modifier = Modifier.size(HeaderActions.GLYPH.dp),
        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = if (dimmed) TextEmphasis.Quaternary else 1f),
    )
}
