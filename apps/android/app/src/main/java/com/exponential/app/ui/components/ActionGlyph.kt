package com.exponential.app.ui.components

import androidx.compose.ui.graphics.vector.ImageVector
import com.exponential.app.data.api.ActionDto
import com.exponential.app.ui.icons.ExpIcons

/**
 * EXP-273: the action's own curated glyph (builtins set one too), falling back
 * to the generic action mark.
 *
 * EXP-721: the ONE resolver every surface that draws an action takes — the
 * Actions list, an automation's target, a session row's trailing control and
 * the Start-coding sheet's picker. The picker used to key off `isBuiltin` and
 * draw the create mark for every builtin and the generic mark for every real
 * action, so the same action wore a different glyph depending on which screen
 * you opened. [action] is nullable because an automation (or a session) can
 * reference a row the actions shape hasn't caught up with yet.
 */
fun actionGlyph(action: ActionDto?): ImageVector =
    action?.icon?.let { ExpIcons.byName(it) } ?: ExpIcons.actionDefault
