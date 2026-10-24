package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil3.compose.AsyncImage
import at.exponential.ui.primitives.AvatarView
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens

/**
 * Up-to-two-letter initials derived from a display name or email. Unifies the
 * three slightly different algorithms that were duplicated across the app
 * (avatar menu, drawer, comment rows, assignee chip).
 */
fun initialsFor(nameOrEmail: String?): String {
    if (nameOrEmail.isNullOrBlank()) return "?"
    val base = nameOrEmail.trim()
    val local = if (base.contains('@')) base.substringBefore('@') else base
    val parts = local.split(' ', '.', '_', '-', '+').filter { it.isNotBlank() }
    return when {
        parts.size >= 2 -> "${parts[0].first()}${parts[1].first()}".uppercase()
        local.length >= 2 -> local.take(2).uppercase()
        local.isNotEmpty() -> local.take(1).uppercase()
        else -> "?"
    }
}

/** FNV-1a 32-bit offset basis / prime — the hash every client agrees on. */
private val FnvOffsetBasis: Int = 0x811C9DC5.toInt()
private const val FnvPrime: Int = 0x01000193

/**
 * Which fallback hue a user id lands on (EXP-698 r4): FNV-1a 32-bit over the
 * id's UTF-8 bytes, modulo [DesignTokens.Avatar.Hues]. The four clients each
 * pin the SAME eight-id fixture (`AvatarHueTest` here, `avatar-color.test.ts`
 * on web) — the hash and the palette order together ARE the contract, so a
 * person keeps one colour everywhere. Never reorder the palette.
 */
fun avatarHueIndex(userId: String?): Int {
    var hash = FnvOffsetBasis
    for (byte in (userId ?: "").toByteArray(Charsets.UTF_8)) {
        hash = hash xor (byte.toInt() and 0xFF)
        hash *= FnvPrime
    }
    return (hash.toUInt() % DesignTokens.Avatar.Hues.size.toUInt()).toInt()
}

/**
 * Circular initials avatar (iOS user avatar). With a [userId] the chip takes
 * that person's hashed hue — a 20%-alpha fill under the initials in the full
 * colour, no stroke (EXP-698 r4); without one it falls back to the neutral
 * translucent white fill iOS uses in issue rows / comments. There is no colour
 * override: an avatar's colour IS the hash, so a caller that wants a different
 * one is drawing something that is not a person.
 */
@Composable
fun InitialsAvatar(
    nameOrEmail: String?,
    modifier: Modifier = Modifier,
    size: Dp = 28.dp,
    userId: String? = null,
) {
    val initials = remember(nameOrEmail) { initialsFor(nameOrEmail) }
    val hue = remember(userId) { userId?.let { DesignTokens.Avatar.Hues[avatarHueIndex(it)] } }
    // SLOP-18 / VAPP-89: drawn by the SDK's `AvatarView`, with the app's own
    // initials and FNV hue rule passed in. Its letters sit on a line box
    // trimmed to the glyph (the EXP-393 centring fix, now shared).
    AvatarView(
        name = nameOrEmail.orEmpty(),
        modifier = modifier,
        size = size,
        fill = hue?.copy(alpha = 0.2f) ?: GlassTokens.RowFillActive,
        ink = hue ?: Color.White,
        fontSize = (size.value * 0.42f).sp,
        initials = initials,
        textStyle = LocalTextStyle.current,
        contentDescription = null,
    )
}

/**
 * Circular user avatar: the synced `user.image` (Google/GitHub photo) clipped to
 * a circle when present, else the initials fallback on that user's own hue
 * (EXP-698 r4). `nameOrEmail` seeds the initials (pass the resolved display
 * name so a name-less Apple user still gets email initials rather than "?").
 * [userId] is the fallback the hue hashes when the `user` ROW is missing — a
 * comment or session whose author has not synced yet still carries the id, and
 * web/iOS hash exactly that, so passing it keeps one colour per person across
 * the four clients instead of a grey chip here and a coloured one there.
 */
@Composable
fun UserAvatar(
    user: UserEntity?,
    nameOrEmail: String?,
    modifier: Modifier = Modifier,
    size: Dp = 28.dp,
    userId: String? = null,
) {
    MemberAvatar(
        imageUrl = user?.image,
        nameOrEmail = nameOrEmail,
        userId = user?.id ?: userId,
        modifier = modifier,
        size = size,
    )
}

/**
 * [UserAvatar] for a caller that holds the three fields but not the ROW — the
 * assignee picker's member list (EXP-1021), whose items are the shared picker
 * contract's, not Room entities.
 */
@Composable
fun MemberAvatar(
    imageUrl: String?,
    nameOrEmail: String?,
    userId: String?,
    modifier: Modifier = Modifier,
    size: Dp = 28.dp,
) {
    val hueId = userId
    val url = imageUrl?.takeIf { it.isNotBlank() }
    if (url != null) {
        // Draw the initials underneath so a still-loading or failed image
        // degrades to initials (parity with iOS/web) instead of a blank
        // circle; the loaded image paints over them.
        Box(modifier = modifier.size(size), contentAlignment = Alignment.Center) {
            InitialsAvatar(nameOrEmail, size = size, userId = hueId)
            AsyncImage(
                model = url,
                contentDescription = null,
                modifier = Modifier
                    .size(size)
                    .clip(CircleShape),
            )
        }
    } else {
        InitialsAvatar(nameOrEmail, modifier = modifier, size = size, userId = hueId)
    }
}

/**
 * Rounded-square team monogram (iOS `TeamAvatar`). Shows the
 * `iconUrl` image when set, else the first letter of the team name on a
 * tinted chip. Replaces the copy that lived in the now-deleted AppDrawer.
 */
@Composable
fun TeamAvatar(
    team: TeamEntity?,
    modifier: Modifier = Modifier,
    size: Dp = 28.dp,
) {
    val initial = (team?.name?.firstOrNull()?.toString() ?: "?").uppercase()
    val url = team?.iconUrl?.takeIf { it.isNotBlank() }
    // EXP-698 r5: the FULL primary, on every client — a white square with a
    // black initial. Drawn by the SDK's `AvatarView` as a rounded square
    // (VAPP-89); the icon image is its picture.
    AvatarView(
        name = team?.name.orEmpty(),
        modifier = modifier,
        size = size,
        fill = MaterialTheme.colorScheme.primary,
        ink = MaterialTheme.colorScheme.onPrimary,
        // Thread 58cb32df: the Team tab's 84 avatar scales its initial.
        fontSize = if (size > 32.dp) (size.value * 0.42f).sp else MaterialTheme.typography.labelMedium.fontSize,
        initials = initial,
        cornerRadius = size / 4,
        textStyle = if (size > 32.dp) {
            MaterialTheme.typography.labelMedium.copy(fontSize = (size.value * 0.42f).sp, fontWeight = androidx.compose.ui.text.font.FontWeight.Bold)
        } else {
            MaterialTheme.typography.labelMedium
        },
        contentDescription = null,
        picture = url?.let {
            {
                Box(
                    Modifier
                        .size(size)
                        .background(MaterialTheme.colorScheme.primary),
                ) {
                    AsyncImage(
                        model = it,
                        contentDescription = null,
                        modifier = Modifier.size(size),
                    )
                }
            }
        },
    )
}

/**
 * #EXP-1383 (agent-settings.json `actor` / `assignee`): an AGENT where a
 * person would be (an actor, an assignee) reads as THE AGENT MARK at the
 * avatar's size: its glyph in its colour on the muted disc — the Chats agent
 * bubble and the chat header's disc — never initials. The glyph follows the
 * web avatar table (16→10, 20→12, 24→14, 28→16, 32→18, 48→24).
 */
@Composable
fun AgentAvatar(
    icon: String?,
    color: String?,
    modifier: Modifier = Modifier,
    size: Dp = 28.dp,
) {
    val glyph = icon?.let { com.exponential.app.ui.icons.ExpIcons.byName(it) }
        ?: com.exponential.app.ui.icons.ExpIcons.byName(com.exponential.app.domain.SessionTree.ListCopy.AGENT_DEFAULT_ICON)
        ?: com.exponential.app.ui.icons.ExpIcons.settingsAgents
    val tint = remember(color) {
        com.exponential.app.ui.parseColor(color ?: com.exponential.app.domain.SessionTree.ListCopy.AGENT_DEFAULT_COLOR)
    }
    Box(
        modifier = modifier
            .size(size)
            .clip(CircleShape)
            .background(DesignTokens.Palette.Muted, CircleShape)
            .testTag(AGENT_AVATAR_TEST_TAG),
        contentAlignment = Alignment.Center,
    ) {
        androidx.compose.material3.Icon(
            glyph,
            contentDescription = null,
            tint = tint,
            modifier = Modifier.size(agentAvatarGlyph(size)),
        )
    }
}

/** The agent mark's glyph side for an avatar [size] (web `USER_AVATAR_SIZES.glyph`). */
internal fun agentAvatarGlyph(size: Dp): Dp = when (size.value.toInt()) {
    16 -> 10.dp
    20 -> 12.dp
    24 -> 14.dp
    28 -> 16.dp
    32 -> 18.dp
    48 -> 24.dp
    else -> size * 0.58f
}

const val AGENT_AVATAR_TEST_TAG = "agent-avatar"

/** #EXP-1383: a synced agent definition as the actor / assignee row. */
fun com.exponential.app.data.db.AgentDefinitionEntity.actorRow(): com.exponential.app.domain.AgentSettings.AgentRowLike =
    com.exponential.app.domain.AgentSettings.AgentRowLike(
        id = id,
        name = name,
        icon = icon,
        color = color,
        sortOrder = sortOrder,
    )
