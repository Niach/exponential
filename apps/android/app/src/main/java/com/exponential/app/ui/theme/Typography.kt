package com.exponential.app.ui.theme

import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import com.exponential.app.R

/**
 * #EXP-1629: every mono run is JetBrains Mono (OFL-1.1), the face the IDE
 * bundles and the web self-hosts: the same look ×4, never the system mono.
 * The genUI `exponential` theme's `type.family.mono` resolves to it too
 * (`ExponentialApp` registers it under its family name).
 */
val ExpMono = FontFamily(
    Font(R.font.jetbrains_mono_regular, FontWeight.Normal, FontStyle.Normal),
    Font(R.font.jetbrains_mono_italic, FontWeight.Normal, FontStyle.Italic),
    Font(R.font.jetbrains_mono_bold, FontWeight.Bold, FontStyle.Normal),
    Font(R.font.jetbrains_mono_bold_italic, FontWeight.Bold, FontStyle.Italic),
)

/** The family name genUI themes ask for. */
const val EXP_MONO_FAMILY = "JetBrains Mono"
