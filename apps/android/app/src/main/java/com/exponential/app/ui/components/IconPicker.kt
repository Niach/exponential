package com.exponential.app.ui.components

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-575: THE icon picker — one slim 36dp swatch showing the current pick
 * that opens the curated grid ([IconSwatchGrid]) in a [GlassSheet], so the
 * 60-glyph grid never sits inline in a form. Every surface that picks an icon
 * (create-board form, Start-coding `icon` inputs) renders this; web, desktop
 * and iOS ship the same shape.
 *
 * [selected] is a registry NAME; anything [pickableIconName] rejects reads as
 * "no icon" and draws a dashed placeholder. [allowsNone] hosts (optional action
 * inputs) get a "No icon" reset in the sheet, reported as `""`.
 */
@Composable
fun IconPicker(
    selected: String?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    allowsNone: Boolean = false,
    /** Tint of the picked glyph (the board form passes its color). */
    accentColor: Color = MaterialTheme.colorScheme.primary,
) {
    var open by remember { mutableStateOf(false) }
    val picked = pickableIconName(selected)
    val glyph = picked?.let { ExpIcons.byName(it) }
    val borderColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary)
    val shape = RoundedCornerShape(10.dp)
    Box(
        modifier = modifier
            .size(36.dp)
            .then(
                if (glyph != null) Modifier.border(1.dp, borderColor, shape)
                else Modifier.drawBehind {
                    drawRoundRect(
                        color = borderColor,
                        cornerRadius = CornerRadius(10.dp.toPx()),
                        style = Stroke(
                            width = 1.dp.toPx(),
                            pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 6f)),
                        ),
                    )
                },
            )
            .clickable { open = true }
            .semantics { contentDescription = picked?.let { "Icon: $it" } ?: "Pick an icon" },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            glyph ?: ExpIcons.uiIconPlaceholder,
            contentDescription = null,
            tint = if (glyph != null) accentColor
            else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.size(20.dp),
        )
    }
    if (open) {
        GlassSheet(title = "Icon", onDismiss = { open = false }) {
            Column(modifier = Modifier.padding(horizontal = 20.dp)) {
                if (allowsNone && picked != null) {
                    TextButton(onClick = {
                        onSelect("")
                        open = false
                    }) { Text("No icon") }
                }
                IconSwatchGrid(
                    selected = picked,
                    onSelect = {
                        onSelect(it)
                        open = false
                    },
                    accentColor = accentColor,
                    modifier = Modifier.padding(bottom = 12.dp),
                )
            }
        }
    }
}
