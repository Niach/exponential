package com.exponential.app.domain

// The Guide's section pages + the session view's shared labels (EXP-1251;
// kept from the retired Work-screen faces, EXP-1291: the Guide lives on the
// THREAD now, the session view is the raw transcript). Web
// `guideSectionPage` (`lib/work-faces.ts`).

/** EXP-1251: the Guide (contract `diffUi.guideFace`). */
val GUIDE_FACE_LABEL: String = DomainContract.diffUiGuideFace
/** EXP-933: the transcript card under a settled `sessions_guide` call that
 *  opens the Guide. */
val OPEN_RESULTS_LABEL: String = "Open $GUIDE_FACE_LABEL"
/** The Start circle's label (an issue's bar). */
const val START_CODING_LABEL = "Start coding"

/** The steer composer's placeholder, byte-identical ×4. */
const val STEER_COMPOSER_PLACEHOLDER = "Type / for commands"
/** The composer footer's plan-mode word, blue while plan mode is on. */
const val PLAN_MODE_LABEL = "Plan mode"

// ── EXP-1251: the Guide's section pages ─────────────────────────────────────
// A Changes row opens its section's changes as a page under the Guide: a
// numbered section, the Summary's own files ([GuideSectionKey.Lead]), the
// automatic Other changes ([GuideSectionKey.Other]) or Show complete diff
// ([GuideSectionKey.All]). Web `guideSectionPage` (`lib/work-faces.ts`).

sealed class GuideSectionKey {
    data class Numbered(val index: Int) : GuideSectionKey()
    data object Lead : GuideSectionKey()
    data object Other : GuideSectionKey()
    data object All : GuideSectionKey()
}

/** A section page: the back row's caption (`02 / 06`, numbered sections
 *  only), its title, the files it covers and their summed counts. */
data class GuideSectionPage(
    val section: GuideSectionKey,
    val caption: String?,
    val title: String,
    val files: List<Diff.File>,
    val additions: Int,
    val deletions: Int,
)

/** The page [section] opens over [files] (the diff the Guide counts): the
 *  same coverage the Guide's rows read, so a page never shows a file its row
 *  did not count. Null = no such section (a stale link) or no diff loaded.
 *  [numbered] = false for the PR-body fallback (an unnumbered Guide): its
 *  section page carries no `01 / 01` caption, as its band shows no number. */
fun guideSectionPage(
    groups: List<SessionResultGroup>,
    files: List<Diff.File>?,
    section: GuideSectionKey,
    numbered: Boolean = true,
): GuideSectionPage? {
    if (files == null) return null
    val coverage = guideCoverage(groups, files)
    fun page(title: String, caption: String?, set: GuideChangeSet?): GuideSectionPage? =
        set?.let { GuideSectionPage(section, caption, title, it.files, it.additions, it.deletions) }
    return when (section) {
        GuideSectionKey.All -> page(GUIDE_CHANGES_TOPIC, null, coverage.complete)
        GuideSectionKey.Other -> coverage.other?.let { page(it.topic, null, it.changes) }
        GuideSectionKey.Lead -> coverage.lead?.let { page(it.group.topic, null, it.changes) }
        is GuideSectionKey.Numbered -> coverage.sections.firstOrNull { it.index == section.index }?.let {
            page(it.group.topic, if (numbered) guideSectionCaption(it.index, it.total) else null, it.changes)
        }
    }
}

/** EXP-1154: the PR-body fallback's band label without a PR title (web
 *  `PR_DESCRIPTION_FALLBACK_TOPIC`). */
const val PR_FALLBACK_TITLE = "Pull request"

/** EXP-1154: the PR-body fallback's text for a blank body (web `PR_DESCRIPTION_EMPTY`). */
const val PR_FALLBACK_EMPTY_BODY = "No description."

/**
 * EXP-1154 (web `prDescriptionGroups`): the open PR's GitHub body as ONE
 * Guide group: band = the PR title (else `Pull request`), text = the body
 * (`No description.` when blank). EXP-1251: no report + a PR = ONE Changes
 * section, so the group claims EVERY diff path: its band carries the one
 * Changes row, nothing is left for `Other changes`, and `Show complete diff`
 * still closes the page.
 */
fun prDescriptionGroup(title: String?, body: String?, files: List<Diff.File>?): SessionResultGroup =
    SessionResultGroup(
        topic = title?.trim()?.takeIf { it.isNotEmpty() } ?: PR_FALLBACK_TITLE,
        entries = emptyList(),
        text = body?.trim()?.takeIf { it.isNotEmpty() } ?: PR_FALLBACK_EMPTY_BODY,
        files = files.orEmpty().map { it.path },
    )

/** The section page's back-row summary: `+A −D · N files`. */
fun guideSectionSummary(page: GuideSectionPage): String =
    "${Diff.additionsLabel(page.additions)} ${Diff.deletionsLabel(page.deletions)} · ${guideFileCountLabel(page.files.size)}"


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
