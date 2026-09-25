package com.exponential.app.ui.components.picker

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.AccountLimits
import com.exponential.app.ui.components.AccountLimitBars
import com.exponential.app.ui.components.AccountLimitBarsWidth
import com.exponential.app.ui.components.agentIconPainter
import com.exponential.app.ui.components.agentIconTint
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-1029 contract, EXP-1021 implementation — the account picker: every
 * signed-in login the bound machine reports, across agents, so picking one
 * IMPLIES its agent (EXP-872). It carries what the EXP-991 account menu
 * carried, its options AND its preview, onto [Picker]; that menu is gone, and
 * every surface that picks a login (the composer, the automation editor, the
 * device settings) goes through this one sheet.
 *
 * A row says exactly what the menu row said: the agent's brand mark, the
 * login's EMAIL, its health badge beside it when the credential is dead, and
 * the EXP-992 rate-limit bars (`5h` / `week` / model) UNDER it — the
 * touch-platform placement. Those two are why this picker draws its own row
 * BODY: a brand mark is a drawable, not an `AppIcons` vector, and the bars
 * stack under the email rather than sitting in the row's single leading slot.
 * The highlight and the click stay the primitive's, like every other row.
 */
data class AccountPickerOption(
    /** The option key (`<agent>:<profile>`), `AccountOption.key` today. */
    val key: String,
    /** Contract `codingAgent`. */
    val agent: String,
    val email: String,
    /** A dead credential's health badge text, if any. */
    val healthNote: String? = null,
    /** EXP-992: the window fractions (0-1); null until a probe landed. */
    val limits: AccountLimits? = null,
)

fun accountPickerItems(options: List<AccountPickerOption>): List<PickerItem<String>> =
    options.map { option ->
        PickerItem(
            value = option.key,
            label = option.email,
            // The badge rides as the row's note; the body draws it BESIDE the
            // email (the menu row's shape), because the second line is the
            // bars'.
            description = option.healthNote,
            keywords = listOf(option.email, option.agent),
        )
    }

@Composable
fun AccountPicker(
    options: List<AccountPickerOption>,
    value: String?,
    onChange: (String) -> Unit,
    /**
     * EXP-1021: the sheet CONTROLLED by the caller, for a picker that is a
     * state machine rather than a chip (the issue screens open theirs from a
     * properties sheet that has already closed).
     */
    open: Boolean? = null,
    onOpenChange: ((Boolean) -> Unit)? = null,
    trigger: @Composable (open: () -> Unit) -> Unit = {},
) {
    val byKey = remember(options) { options.associateBy { it.key } }
    Picker(
        items = accountPickerItems(options),
        mode = PickerMode.Single,
        value = setOfNotNull(value),
        onChange = { picked -> picked.firstOrNull()?.let(onChange) },
        emptyText = "No accounts",
        title = "Account",
        open = open,
        onOpenChange = onOpenChange,
        renderItem = { item -> AccountRowBody(item, byKey[item.value]) },
        trigger = trigger,
    )
}

/**
 * The login row's body. Split out of the picker call so the layout reads as
 * what it is: mark · (email + badge) over the limit preview.
 */
@Composable
private fun RowScope.AccountRowBody(item: PickerItem<String>, option: AccountPickerOption?) {
    if (option != null) {
        Icon(
            agentIconPainter(option.agent),
            contentDescription = null,
            modifier = Modifier.size(PickerDefaults.IconSize),
            // claude's mark keeps its own brand orange; every other one takes
            // the row's glyph tone.
            tint = agentIconTint(option.agent, Color.White.copy(alpha = TextEmphasis.Secondary)),
        )
        Spacer(Modifier.width(PickerDefaults.LeadingGap))
    }
    Column(modifier = Modifier.weight(1f)) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                item.label,
                style = MaterialTheme.typography.bodyMedium,
                color = PickerDefaults.labelColor(enabled = !item.disabled),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (item.description != null) {
                Text(
                    item.description,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        // EXP-992: the preview the EXP-991 menu row carried, kept on the move
        // to the sheet — the same [AccountLimitBars] block, so a percentage
        // can never read in two tones.
        option?.limits?.let { limits ->
            Spacer(Modifier.height(3.dp))
            AccountLimitBars(limits, modifier = Modifier.width(AccountLimitBarsWidth))
        }
    }
}
