package com.exponential.app.domain

/**
 * EXP-1121: whether an issue can Start coding RIGHT NOW, as three ordered
 * steps every client derives from data it already has — GitHub connected, a
 * repository on the board, a device online. Start coding always renders for a
 * member (dashed amber while a step is missing, the caption naming the FIRST
 * missing one); a tap opens the "Ready to code?" checklist, each unmet row
 * carrying its fix.
 *
 * Pure and FIXTURE-LOCKED ×4 (`packages/domain-contract/fixtures/
 * coding-readiness.json`): web `lib/coding-readiness.ts`, desktop
 * `domain::coding_readiness`, iOS `CodingReadiness.swift` return
 * byte-identical output, copy included.
 *
 * Remote start (the steer relay) is NOT a step: an instance without it has no
 * remote coding at all, so the button stays hidden (`visible == false`).
 */
object CodingReadiness {

    enum class StepKey(val wire: String) { GITHUB("github"), REPOSITORY("repository"), DEVICE("device") }

    /** `MET` = green tick; `CURRENT` = the FIRST unmet step (highlighted, fixes
     * shown); `PENDING` = unmet behind it (no fixes until it is current). */
    enum class StepState(val wire: String) { MET("met"), CURRENT("current"), PENDING("pending") }

    enum class Fix(val wire: String, val label: String) {
        CONNECT_GITHUB("connect_github", Copy.FIX_CONNECT_GITHUB),
        CHOOSE_REPOSITORY("choose_repository", Copy.FIX_CHOOSE_REPOSITORY),
        BOARD_SETTINGS("board_settings", Copy.FIX_BOARD_SETTINGS),
        OPEN_DEVICES("open_devices", Copy.FIX_OPEN_DEVICES),
        GET_DESKTOP_APP("get_desktop_app", Copy.FIX_GET_DESKTOP_APP),
        SET_UP_SERVER("set_up_server", Copy.FIX_SET_UP_SERVER),
    }

    data class Device(
        val label: String,
        /** Registered by the caller (a teammate's shared server is not). */
        val own: Boolean,
        val online: Boolean,
        /** Unix ms, null when never seen. */
        val lastSeenAtMs: Long?,
    )

    /** `label` = the connected account (`acme-inc`) for the met row's detail. */
    data class Github(val connected: Boolean, val label: String?)

    data class Input(
        val isMember: Boolean,
        /** `steer.config.enabled`; null while loading. */
        val remoteStartEnabled: Boolean?,
        val teamName: String,
        val boardName: String,
        /** The board's repository full name (`owner/name`), null = none. A board
         * with a `repositoryId` whose row has not resolved yet passes `""`. */
        val boardRepository: String?,
        /** Asked only while the board has no repository: does the team have a
         * GitHub installation or repository? null = still loading. */
        val github: Github?,
        /** Own devices + servers shared with the team; null while loading. */
        val devices: List<Device>?,
        val nowMs: Long,
    )

    data class Step(
        val key: StepKey,
        val state: StepState,
        val title: String,
        val body: String?,
        /** Right-aligned detail of a met row (account, repo, device label). */
        val detail: String?,
        val fixes: List<Fix>,
    )

    data class Readiness(
        /** Non-member or no remote start on this instance: render nothing. */
        val visible: Boolean,
        /** Inputs still loading: the button shows but stays inert, no caption. */
        val loading: Boolean,
        val ready: Boolean,
        val metCount: Int,
        val total: Int,
        val steps: List<Step>,
        /** "1 of 3 set up. Fix the rest here." */
        val summary: String,
        /** The one-line caption: the first missing step, null when ready or loading. */
        val caption: String?,
    )

    // ── Copy (byte-identical ×4) ────────────────────────────────────────
    object Copy {
        const val TITLE = "Ready to code?"
        const val START = "Start coding"
        const val CLOSE = "Close"
        const val CAPTION_GITHUB = "Needs GitHub"
        const val CAPTION_REPOSITORY = "Needs a repository"
        const val CAPTION_DEVICE = "No device online"
        const val GITHUB_MET = "GitHub connected"
        const val GITHUB_UNMET = "Connect GitHub"
        const val GITHUB_BODY =
            "Start coding clones a repository from a GitHub account or organization connected to the team."
        const val REPOSITORY_MET = "Repository connected"
        const val DEVICE_MET = "Device online"
        const val DEVICE_UNMET = "A device online"
        const val DEVICE_NEVER_BODY =
            "Coding runs on the desktop app or on a server running the CLI. You haven’t set one up yet."
        const val FIX_CONNECT_GITHUB = "Connect GitHub"
        const val FIX_CHOOSE_REPOSITORY = "Choose repository"
        const val FIX_BOARD_SETTINGS = "Board settings"
        const val FIX_OPEN_DEVICES = "Open Devices"
        const val FIX_GET_DESKTOP_APP = "Get the desktop app"
        const val FIX_SET_UP_SERVER = "Set up a server"
        const val PICKER_SEARCH = "Search repositories…"
        const val PICKER_MATCHES_BOARD = "matches board"
        const val PICKER_ADD_FROM_GITHUB = "Add another repository from GitHub…"
        const val PICKER_EMPTY = "No repositories connected to the team yet."
        const val ALL_SET = "All set."
        const val ONE_LEFT = "One step left."
        const val FIX_REST = "Fix the rest here."
        const val JUST_NOW = "just now"

        /** Every constant, keyed like web's `READINESS_COPY` (the fixture's `copy` table). */
        val table: Map<String, String> = mapOf(
            "title" to TITLE,
            "start" to START,
            "close" to CLOSE,
            "captionGithub" to CAPTION_GITHUB,
            "captionRepository" to CAPTION_REPOSITORY,
            "captionDevice" to CAPTION_DEVICE,
            "githubMet" to GITHUB_MET,
            "githubUnmet" to GITHUB_UNMET,
            "githubBody" to GITHUB_BODY,
            "repositoryMet" to REPOSITORY_MET,
            "deviceMet" to DEVICE_MET,
            "deviceUnmet" to DEVICE_UNMET,
            "deviceNeverBody" to DEVICE_NEVER_BODY,
            "fixConnectGithub" to FIX_CONNECT_GITHUB,
            "fixChooseRepository" to FIX_CHOOSE_REPOSITORY,
            "fixBoardSettings" to FIX_BOARD_SETTINGS,
            "fixOpenDevices" to FIX_OPEN_DEVICES,
            "fixGetDesktopApp" to FIX_GET_DESKTOP_APP,
            "fixSetUpServer" to FIX_SET_UP_SERVER,
            "pickerSearch" to PICKER_SEARCH,
            "pickerMatchesBoard" to PICKER_MATCHES_BOARD,
            "pickerAddFromGithub" to PICKER_ADD_FROM_GITHUB,
            "pickerEmpty" to PICKER_EMPTY,
            "allSet" to ALL_SET,
            "oneLeft" to ONE_LEFT,
            "fixRest" to FIX_REST,
            "justNow" to JUST_NOW,
        )
    }

    fun repositoryTitle(board: String): String = "Connect a repository to $board"

    fun repositoryBody(board: String): String =
        "Start coding clones the board’s repository. $board has none yet."

    fun deviceBody(team: String): String =
        "None of your devices, or the ones shared with $team, is online."

    fun lastSeen(label: String, ago: String): String = "Your $label was last seen $ago."

    fun pickerUsedBy(board: String): String = "used by $board"

    fun summary(met: Int, total: Int): String {
        val tail = when {
            met == total -> Copy.ALL_SET
            total - met == 1 -> Copy.ONE_LEFT
            else -> Copy.FIX_REST
        }
        return "$met of $total set up. $tail"
    }

    /** "just now" / "5 min ago" / "2 h ago" / "3 d ago" (floored). */
    fun ago(nowMs: Long, thenMs: Long): String {
        val seconds = maxOf(0L, Math.floorDiv(nowMs - thenMs, 1000L))
        if (seconds < 60) return Copy.JUST_NOW
        val minutes = seconds / 60
        if (minutes < 60) return "$minutes min ago"
        val hours = minutes / 60
        if (hours < 24) return "$hours h ago"
        return "${hours / 24} d ago"
    }

    private fun caption(key: StepKey): String = when (key) {
        StepKey.GITHUB -> Copy.CAPTION_GITHUB
        StepKey.REPOSITORY -> Copy.CAPTION_REPOSITORY
        StepKey.DEVICE -> Copy.CAPTION_DEVICE
    }

    /** The caller's most recently seen OWN device, for "Your … was last seen"
     * (a teammate's shared server is never "yours"). First wins a tie. */
    private fun lastSeenDevice(devices: List<Device>): Device? {
        var best: Device? = null
        for (device in devices) {
            val seen = device.lastSeenAtMs
            if (!device.own || seen == null) continue
            if (best == null || seen > (best.lastSeenAtMs ?: 0L)) best = device
        }
        return best
    }

    fun derive(input: Input): Readiness {
        val hasRepository = input.boardRepository != null
        val visible = input.isMember && input.remoteStartEnabled != false
        val loading = input.remoteStartEnabled == null ||
            input.devices == null ||
            (!hasRepository && input.github == null)

        val devices = input.devices.orEmpty()
        val online = devices.firstOrNull { it.online }
        val githubMet = hasRepository || input.github?.connected == true
        val met = mapOf(
            StepKey.GITHUB to githubMet,
            StepKey.REPOSITORY to hasRepository,
            StepKey.DEVICE to (online != null),
        )
        val order = listOf(StepKey.GITHUB, StepKey.REPOSITORY, StepKey.DEVICE)
        val firstMissing = order.firstOrNull { met[it] != true }
        fun state(key: StepKey): StepState = when {
            met[key] == true -> StepState.MET
            key == firstMissing -> StepState.CURRENT
            else -> StepState.PENDING
        }

        val seen = lastSeenDevice(devices)
        val seenAt = seen?.lastSeenAtMs
        val lastSeenLine = if (seen != null && seenAt != null) {
            lastSeen(seen.label, ago(input.nowMs, seenAt))
        } else {
            null
        }
        val registered = devices.any { it.own }

        val steps = order.map { key ->
            val s = state(key)
            when (key) {
                StepKey.GITHUB -> if (s == StepState.MET) {
                    Step(key, s, Copy.GITHUB_MET, null, input.github?.label, emptyList())
                } else {
                    Step(key, s, Copy.GITHUB_UNMET, Copy.GITHUB_BODY, null, listOf(Fix.CONNECT_GITHUB))
                }
                StepKey.REPOSITORY -> if (s == StepState.MET) {
                    Step(
                        key, s, Copy.REPOSITORY_MET, null,
                        input.boardRepository?.takeIf { it.isNotEmpty() }, emptyList(),
                    )
                } else {
                    Step(
                        key, s,
                        repositoryTitle(input.boardName),
                        repositoryBody(input.boardName),
                        null,
                        if (s == StepState.CURRENT) {
                            listOf(Fix.CHOOSE_REPOSITORY, Fix.BOARD_SETTINGS)
                        } else {
                            emptyList()
                        },
                    )
                }
                StepKey.DEVICE -> when (s) {
                    StepState.MET -> Step(key, s, Copy.DEVICE_MET, null, online?.label, emptyList())
                    StepState.PENDING ->
                        Step(key, s, Copy.DEVICE_UNMET, lastSeenLine, null, emptyList())
                    StepState.CURRENT -> {
                        val body = if (!registered) {
                            Copy.DEVICE_NEVER_BODY
                        } else {
                            listOfNotNull(deviceBody(input.teamName), lastSeenLine)
                                .filter { it.isNotEmpty() }
                                .joinToString(" ")
                        }
                        Step(
                            key, s, Copy.DEVICE_UNMET, body, null,
                            if (registered) {
                                listOf(Fix.OPEN_DEVICES, Fix.GET_DESKTOP_APP, Fix.SET_UP_SERVER)
                            } else {
                                listOf(Fix.GET_DESKTOP_APP, Fix.SET_UP_SERVER)
                            },
                        )
                    }
                }
            }
        }

        val metCount = order.count { met[it] == true }
        return Readiness(
            visible = visible,
            loading = loading,
            ready = !loading && firstMissing == null,
            metCount = metCount,
            total = order.size,
            steps = steps,
            summary = summary(metCount, order.size),
            caption = if (!visible || loading || firstMissing == null) null else caption(firstMissing),
        )
    }
}
