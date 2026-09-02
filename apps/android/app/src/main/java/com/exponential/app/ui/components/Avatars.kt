package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil3.compose.AsyncImage
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
    val fill = hue?.copy(alpha = 0.2f) ?: GlassTokens.RowFillActive
    val fg = hue ?: Color.White
    val fontSize = (size.value * 0.42f).sp
    Box(
        modifier = modifier
            .size(size)
            .background(fill, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            initials,
            color = fg,
            fontSize = fontSize,
            // Centering the Box centers the LINE box, not the glyph: with the
            // ambient line height (much taller than this font size) plus the
            // legacy font padding, the initials sat visibly below the circle's
            // middle (EXP-393 — it shipped in the store screenshots). Trim the
            // line box to the glyph and the two centres coincide.
            lineHeight = fontSize,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
            style = LocalTextStyle.current.merge(
                TextStyle(
                    platformStyle = PlatformTextStyle(includeFontPadding = false),
                    lineHeightStyle = LineHeightStyle(
                        alignment = LineHeightStyle.Alignment.Center,
                        trim = LineHeightStyle.Trim.Both,
                    ),
                ),
            ),
        )
    }
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
    val hueId = user?.id ?: userId
    val url = user?.image?.takeIf { it.isNotBlank() }
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
    val shape = RoundedCornerShape(size / 4)
    Box(
        modifier = modifier
            .size(size)
            .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.7f), shape),
        contentAlignment = Alignment.Center,
    ) {
        if (url != null) {
            AsyncImage(
                model = url,
                contentDescription = null,
                modifier = Modifier
                    .size(size)
                    .clip(shape),
            )
        } else {
            Text(
                initial,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onPrimary,
            )
        }
    }
}
