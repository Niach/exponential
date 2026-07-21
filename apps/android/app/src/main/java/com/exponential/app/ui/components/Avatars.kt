package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil3.compose.AsyncImage
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.UserEntity

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

/**
 * Circular initials avatar (iOS user avatar). Defaults to the translucent
 * white fill iOS uses in issue rows / comments.
 */
@Composable
fun InitialsAvatar(
    nameOrEmail: String?,
    modifier: Modifier = Modifier,
    size: Dp = 28.dp,
    background: Color = Color.White.copy(alpha = 0.15f),
    contentColor: Color = Color.White,
) {
    val initials = remember(nameOrEmail) { initialsFor(nameOrEmail) }
    Box(
        modifier = modifier
            .size(size)
            .background(background, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            initials,
            color = contentColor,
            fontSize = (size.value * 0.42f).sp,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
        )
    }
}

/**
 * Circular user avatar: the synced `user.image` (Google/GitHub photo) clipped to
 * a circle when present, else the initials fallback. `nameOrEmail` seeds the
 * initials (pass the resolved display name so a name-less Apple user still gets
 * email initials rather than "?").
 */
@Composable
fun UserAvatar(
    user: UserEntity?,
    nameOrEmail: String?,
    modifier: Modifier = Modifier,
    size: Dp = 28.dp,
) {
    val url = user?.image?.takeIf { it.isNotBlank() }
    if (url != null) {
        // Draw the initials underneath so a still-loading or failed image
        // degrades to initials (parity with iOS/web) instead of a blank
        // circle; the loaded image paints over them.
        Box(modifier = modifier.size(size), contentAlignment = Alignment.Center) {
            InitialsAvatar(nameOrEmail, size = size)
            AsyncImage(
                model = url,
                contentDescription = null,
                modifier = Modifier
                    .size(size)
                    .clip(CircleShape),
            )
        }
    } else {
        InitialsAvatar(nameOrEmail, modifier = modifier, size = size)
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
