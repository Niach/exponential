package com.exponential.app.ui.components

import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.AccountOption
import com.exponential.app.domain.AgentHealthRules
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.statusIcon
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.ui.components.picker.AccountPickerOption
import com.exponential.app.ui.components.picker.AssigneePickerMember
import com.exponential.app.ui.components.picker.BoardPickerBoard
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.components.picker.IssuePickerIssue
import com.exponential.app.ui.components.picker.LabelPickerLabel
import com.exponential.app.ui.components.picker.PriorityPickerOption
import com.exponential.app.ui.components.picker.StatusPickerStatus
import com.exponential.app.ui.theme.resolvedStatusColor
import com.exponential.app.ui.theme.statusColor

/**
 * EXP-1021: the app's rows as the shared picker contract's rows. The picker
 * package is the CONTRACT (the same five types on web, iOS and the IDE), so it
 * never learns about Room entities — the mapping lives here, once, and every
 * screen that picks a status/priority/assignee/board/label/issue goes through
 * it. A second copy of one of these is how the four sheets drifted apart in
 * the first place.
 */

/**
 * A resolved team status (EXP-314) as a picker row. Glyph and tint are handed
 * in RESOLVED: a started row's glyph is its position's pie clock and a builtin
 * row's colour is its semantic token, neither of which the category alone
 * carries.
 */
fun ResolvedIssueStatus.toPickerRow(): StatusPickerStatus = StatusPickerStatus(
    id = id,
    name = name,
    category = category.wire,
    colorHex = colorHex,
    iconName = iconName,
    color = resolvedStatusColor(this),
)

/** The contract's priority table in its display order (urgent → none). */
fun issuePriorityPickerOptions(): List<PriorityPickerOption> =
    issuePriorityOrder.map { PriorityPickerOption(value = it.wire, label = it.label) }

fun UserEntity.toPickerMember(): AssigneePickerMember = AssigneePickerMember(
    id = id,
    name = userDisplayName(this, id),
    email = email.takeIf { it.isNotBlank() },
    image = image,
)

fun BoardEntity.toPickerBoard(): BoardPickerBoard = BoardPickerBoard(
    id = id,
    name = name,
    // `boardIcon`'s repo fallback is the board's own, so resolve the NAME here
    // rather than letting the picker guess from a null.
    icon = icon ?: if (repositoryId != null) "code" else null,
    colorHex = color,
)

fun LabelEntity.toPickerLabel(): LabelPickerLabel =
    LabelPickerLabel(id = id, name = name, colorHex = color)

/**
 * An issue as a picker row, with the STATUS glyph the relations linker and the
 * duplicate picker have always drawn on it — the anchor enum's pair, exactly
 * what [StatusIcon] renders in a list row, so the sheet and the list say the
 * same thing about the same issue.
 */
fun IssueEntity.toPickerIssue(disabled: Boolean = false): IssuePickerIssue {
    val anchor = IssueStatus.fromWire(status)
    return IssuePickerIssue(
        id = id,
        identifier = identifier,
        title = title,
        disabled = disabled,
        icon = statusIcon(anchor),
        color = statusColor(anchor),
    )
}

/**
 * A machine as a device row (EXP-432: a teammate's shared server carries its
 * owner, so [deviceOptionLabel] is what a machine READS as everywhere).
 */
fun SteerDevice.toPickerDevice(): DevicePickerDevice = DevicePickerDevice(
    id = deviceId,
    name = deviceOptionLabel(this),
    icon = icon,
    isServer = isServer,
)

/**
 * One of a machine's logins (EXP-872) as an account row: the health verdict
 * rides as the badge text and the EXP-992 windows as the row's limit preview,
 * so the sheet row says exactly what the EXP-991 menu row said.
 */
fun AccountOption.toPickerAccount(): AccountPickerOption = AccountPickerOption(
    key = key,
    agent = agent,
    email = email,
    healthNote = AgentHealthRules.badgeLabel(health),
    limits = limits,
)

/** The priority a picker value names. */
fun pickedPriority(values: Set<String>): IssuePriority? =
    values.firstOrNull()?.let(IssuePriority::fromWire)
