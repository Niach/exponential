package com.exponential.app.ui.components

import androidx.compose.runtime.staticCompositionLocalOf
import com.exponential.app.data.db.AgentDefinitionEntity

/** S-3: the synced agent definitions by id, the marks a sender's avatar
 *  draws (the host screen provides them; empty = every agent the default
 *  mark). */
val LocalAgentMarks = staticCompositionLocalOf<Map<String, AgentDefinitionEntity>> { emptyMap() }
