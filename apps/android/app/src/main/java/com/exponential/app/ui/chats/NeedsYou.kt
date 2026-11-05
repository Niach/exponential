package com.exponential.app.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.NeedsYou
import com.exponential.app.ui.components.ChatEntryRow
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LocalPageHeaderActions
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PageHeaderActions
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.threads.ThreadsViewModel

// #FEED-191 NEEDS YOU ×Android (fixtures/needs-you.json; web
// `packages/ui/src/needs-you.tsx`, iOS ExpUI `NeedsYou.swift`): the model is
// [NeedsYou.needsYou]; these only paint it.
//
//   NeedsYouEntryRow  the first row of Chats (the Archived row's box)
//   needsYouList      one reading column of THE inbox page's row look, else
//                     THE EmptyState
//   NeedsYouPage      the pushed page: the page header with back over the list

/** The entry's count Badge: the needs-you tone, 16 tall (web `Badge`). */
@Composable
fun NeedsYouCountBadge(count: Int, modifier: Modifier = Modifier) {
    Box(
        modifier
            .heightIn(min = 16.dp)
            .background(DesignTokens.Semantic.Yellow, CircleShape)
            .padding(horizontal = 5.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            count.toString(),
            style = MaterialTheme.typography.labelSmall.copy(fontFeatureSettings = "tnum"),
            fontWeight = FontWeight.SemiBold,
            color = Color.Black,
            maxLines = 1,
        )
    }
}

/** THE ENTRY on phones: as tall as the Archived row, the glyph in the needs-you
 *  tone centred in the lead disc's column (no disc), the title `copy.entry`,
 *  the count Badge in the needs-you tone trailing. */
@Composable
fun NeedsYouEntryRow(count: Int, onClick: (() -> Unit)?, modifier: Modifier = Modifier) {
    ChatEntryRow(
        glyph = ExpIcons.navNeedsYou,
        glyphTint = DesignTokens.Semantic.Yellow,
        title = NeedsYou.Copy.ENTRY,
        onClick = onClick,
        modifier = modifier,
        testTag = NeedsYou.TestIds.ENTRY,
    ) {
        NeedsYouCountBadge(count)
    }
}

private val NeedsYou.Row.glyph: ImageVector
    get() = when (kind) {
        NeedsYou.Kind.Plan -> ExpIcons.codingPlan
        NeedsYou.Kind.Question -> ExpIcons.uiHelp
        NeedsYou.Kind.Pr -> ExpIcons.prOpen
    }

/** One row (THE inbox page's flat row): the leading glyph in the needs-you tone,
 *  line 1 the title then the age muted trailing, line 2 the chat then ` · meta`,
 *  muted. */
@Composable
fun NeedsYouRowItem(row: NeedsYou.Row, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        modifier
            .fillMaxWidth()
            .flatRow()
            .clickable(onClick = onClick)
            .testTag(NeedsYou.TestIds.ROW)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(
            row.glyph,
            contentDescription = null,
            tint = DesignTokens.Semantic.Yellow,
            modifier = Modifier.padding(top = 2.dp).size(16.dp),
        )
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    row.title,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Text(row.age, style = MaterialTheme.typography.bodySmall, color = muted, maxLines = 1)
            }
            Text(
                if (row.meta.isEmpty()) row.chat else "${row.chat} · ${row.meta}",
                style = MaterialTheme.typography.bodySmall,
                color = muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** THE list's items: one reading column; no row = THE EmptyState. */
fun LazyListScope.needsYouList(rows: List<NeedsYou.Row>, onOpen: (NeedsYou.Row) -> Unit) {
    if (rows.isEmpty()) {
        item(key = "__empty__") {
            Box(Modifier.fillMaxWidth().height(320.dp)) {
                EmptyState(title = NeedsYou.Copy.EMPTY, icon = ExpIcons.navNeedsYou)
            }
        }
    } else {
        items(rows, key = { "needs_you_${it.messageId}" }) { row ->
            NeedsYouRowItem(row, onClick = { onOpen(row) }, modifier = Modifier.padding(horizontal = 8.dp))
        }
    }
}

/** The page's stateless body (the `needs-you` walk and the live route). */
@Composable
fun NeedsYouPageContent(
    rows: List<NeedsYou.Row>,
    onBack: () -> Unit,
    onOpen: (NeedsYou.Row) -> Unit,
    onOpenInbox: () -> Unit,
    inboxUnread: Boolean,
    modifier: Modifier = Modifier,
) {
    val shell = LocalPageHeaderActions.current
    LazyColumn(
        modifier = modifier.fillMaxSize().navigationBarsPadding().testTag(NeedsYou.TestIds.LIST),
        contentPadding = PaddingValues(top = 4.dp, bottom = 16.dp),
    ) {
        item(key = "__header__") {
            PageHeader(
                title = NeedsYou.Copy.ENTRY,
                onBack = onBack,
                actions = PageHeaderActions(
                    onSearch = { shell?.onSearch?.invoke() },
                    onInbox = onOpenInbox,
                    inboxUnread = inboxUnread,
                ),
            )
        }
        needsYouList(rows, onOpen)
    }
}

/** #FEED-191: the pushed page (needs-you.json THE LIST), the ACTIVE team's rows. */
@Composable
fun NeedsYouPage(
    onBack: () -> Unit,
    onOpen: (NeedsYou.Row) -> Unit,
    onOpenInbox: () -> Unit,
    inboxUnread: Boolean,
    viewModel: ThreadsViewModel = hiltViewModel(),
) {
    val rows by viewModel.needsYouRows.collectAsStateWithLifecycle()
    Scaffold(containerColor = Color.Transparent) { padding ->
        NeedsYouPageContent(
            rows = rows,
            onBack = onBack,
            onOpen = onOpen,
            onOpenInbox = onOpenInbox,
            inboxUnread = inboxUnread,
            modifier = Modifier.padding(top = padding.calculateTopPadding()),
        )
    }
}
