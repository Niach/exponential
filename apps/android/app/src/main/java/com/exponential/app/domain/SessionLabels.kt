package com.exponential.app.domain

// The session view's shared labels + the composer's model pin (kept from the
// retired Guide sections file; S-3: the Guide tab is gone ×4).

/** EXP-933: the transcript card under a settled `sessions_guide` call that
 *  opens the results (contract `diffUi.guideFace`). */
val OPEN_RESULTS_LABEL: String = "Open ${DomainContract.diffUiGuideFace}"

/** The steer composer's placeholder, byte-identical ×4. */
const val STEER_COMPOSER_PLACEHOLDER = "Type / for commands"
/** The composer footer's plan-mode word, blue while plan mode is on. */
const val PLAN_MODE_LABEL = "Plan mode"

/** The live relay model — the `model` config option's value, null when the
 *  engine reported none or a blank. */
fun sessionModel(config: SessionConfigState?): String? {
    val value = config?.options?.firstOrNull { it.id == "model" }?.value
    return if (value.isNullOrEmpty()) null else value
}

/**
 * The composer's model pin ×4 (web `session-composer.tsx` `fallbackModel`):
 * the live relay model, else the session row's own pick, else the device's
 * work model for the row's agent (contract default absent). NEW-8: no
 * dispatcher reader (the server never sets `sessions.dispatch`).
 */
fun composerModel(
    config: SessionConfigState?,
    rowModel: String?,
    cli: String?,
    launchDefaults: com.exponential.app.data.api.DeviceLaunchDefaults?,
): String {
    sessionModel(config)?.let { return it }
    if (!rowModel.isNullOrEmpty()) return rowModel
    val agent = cli?.takeIf { it.isNotEmpty() } ?: "claude"
    val codex = agent == "codex"
    return launchDefaults?.agents?.get(agent)?.model?.takeIf { it.isNotEmpty() }
        ?: if (codex) DomainContract.deviceAgentDefaultsCodexModel else DomainContract.deviceAgentDefaultsModel
}
