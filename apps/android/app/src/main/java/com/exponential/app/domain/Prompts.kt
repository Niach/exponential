package com.exponential.app.domain

/**
 * EXP-1215: the wording of every confirm and choice prompt, byte-identical ×3
 * (web `packages/ui` Prompt, iOS `GlassAlert`, Android `GlassAlert`), mirrored
 * from `packages/domain-contract/fixtures/prompts.json` and locked against it
 * by `PromptsTest`. A site renders ONE entry by its builder (it fills the
 * entry's params, nothing else) and hands the result to `PromptAlert`, keying
 * its handlers by [Prompts.Action.id]. Prompts whose copy is locked elsewhere
 * (issue-draft, blocked-start, stack-merge-choice, the agent-account
 * sentences) are not here.
 */
object Prompts {

    /** The contract's `roles`, in its order. */
    enum class Role(val key: String) {
        Cancel("cancel"),
        Default("default"),
        Primary("primary"),
        Destructive("destructive"),
        QuietDestructive("quietDestructive"),
    }

    /** One answer: [id] keys the site's handler, [label] is what it reads. */
    data class Action(val id: String, val label: String, val role: Role)

    /** A rendered prompt: [actions] in DISPLAY order, [focus] = an action id. */
    data class Prompt(
        val title: String,
        val body: String?,
        val actions: List<Action>,
        val focus: String,
    )

    /**
     * One fixture entry: [copy] = every `title*`/`body*` key with its
     * `{param}` template, [params] = the param names in fixture order.
     */
    class Spec(
        val id: String,
        val copy: Map<String, String>,
        val params: List<String>,
        val actions: List<Action>,
        val focus: String,
    ) {
        /** Fills [titleKey] (and [bodyKey] when given) with [values]. */
        fun render(titleKey: String, bodyKey: String?, vararg values: Pair<String, String>): Prompt =
            Prompt(
                title = fill(copy.getValue(titleKey), values),
                body = bodyKey?.let { fill(copy.getValue(it), values) },
                actions = actions,
                focus = focus,
            )

        private fun fill(template: String, values: Array<out Pair<String, String>>): String =
            values.fold(template) { text, (name, value) -> text.replace("{$name}", value) }
    }

    private val CANCEL = Action("cancel", "Cancel", Role.Cancel)
    private fun destructive(id: String, label: String) = Action(id, label, Role.Destructive)
    private fun primary(id: String, label: String) = Action(id, label, Role.Primary)

    /** A plain destructive confirm: Cancel · [label] (destructive), Cancel focused. */
    private fun destructiveSpec(
        id: String,
        copy: Map<String, String>,
        params: List<String>,
        actionId: String,
        label: String,
    ) = Spec(id, copy, params, listOf(CANCEL, destructive(actionId, label)), focus = "cancel")

    /** A safe confirm: Cancel · [label] (primary), the primary focused. */
    private fun primarySpec(
        id: String,
        copy: Map<String, String>,
        params: List<String>,
        actionId: String,
        label: String,
    ) = Spec(id, copy, params, listOf(CANCEL, primary(actionId, label)), focus = actionId)

    object DeleteIssue {
        val spec = destructiveSpec(
            "delete-issue",
            mapOf("title" to "Delete {identifier}?", "body" to "Its comments and files are deleted with it."),
            listOf("identifier"),
            "delete",
            "Delete",
        )
        fun prompt(identifier: String) = spec.render("title", "body", "identifier" to identifier)
    }

    object DeleteIssues {
        val spec = destructiveSpec(
            "delete-issues",
            mapOf(
                "titleOne" to "Delete 1 issue?",
                "titleMany" to "Delete {count} issues?",
                "bodyOne" to "Its comments and files are deleted with it.",
                "bodyMany" to "Their comments and files are deleted with them.",
            ),
            listOf("count"),
            "delete",
            "Delete",
        )
        fun prompt(count: Int) =
            if (count == 1) spec.render("titleOne", "bodyOne")
            else spec.render("titleMany", "bodyMany", "count" to "$count")
    }

    object DeleteFile {
        val spec = destructiveSpec(
            "delete-file",
            mapOf(
                "title" to "Delete \"{filename}\"?",
                "body" to "A description or comment that embeds it shows a deleted image note instead.",
            ),
            listOf("filename"),
            "delete",
            "Delete",
        )
        fun prompt(filename: String) = spec.render("title", "body", "filename" to filename)
    }

    object MoveIssue {
        val spec = primarySpec(
            "move-issue",
            mapOf("title" to "Move {identifier} to \"{board}\"?", "body" to "It gets a new identifier in that board."),
            listOf("identifier", "board"),
            "move",
            "Move",
        )
        fun prompt(identifier: String, board: String) =
            spec.render("title", "body", "identifier" to identifier, "board" to board)
    }

    object MergeIssuePr {
        val spec = primarySpec(
            "merge-issue-pr",
            mapOf(
                "title" to "Merge PR #{number}?",
                "titleNoNumber" to "Merge this pull request?",
                "bodyOne" to "It is squash-merged.",
                "bodyMany" to "It is squash-merged. It covers {count} issues.",
            ),
            listOf("number", "count"),
            "merge",
            "Merge",
        )

        /** [issueCount] = the issues the PR links (2+ = a batch PR). */
        fun prompt(number: Int?, issueCount: Int = 1) = spec.render(
            if (number != null) "title" else "titleNoNumber",
            if (issueCount > 1) "bodyMany" else "bodyOne",
            "number" to "${number ?: ""}",
            "count" to "$issueCount",
        )
    }

    object MergeRunPr {
        val spec = primarySpec(
            "merge-run-pr",
            mapOf(
                "title" to "Merge PR #{number}?",
                "titleNoNumber" to "Merge this pull request?",
                "body" to "It is squash-merged. No issue is linked to it.",
            ),
            listOf("number"),
            "merge",
            "Merge",
        )
        fun prompt(number: Int?) =
            spec.render(if (number != null) "title" else "titleNoNumber", "body", "number" to "${number ?: ""}")
    }

    /** EXP-1244: an open pull request NO issue or run links (Reviews' repository bands). */
    object MergeExternalPr {
        val spec = primarySpec(
            "merge-external-pr",
            mapOf(
                "title" to "Merge {repository}#{number}?",
                "body" to "It is squash-merged into {base}. No issue is linked to it.",
            ),
            listOf("repository", "number", "base"),
            "merge",
            "Merge",
        )
        fun prompt(repository: String, number: Int, base: String) =
            spec.render("title", "body", "repository" to repository, "number" to "$number", "base" to base)
    }

    object StopRun {
        val spec = destructiveSpec("stop-run", mapOf("title" to "Stop this run?"), emptyList(), "stop", "Stop")
        fun prompt() = spec.render("title", null)
    }

    object ResumeRun {
        val spec = primarySpec(
            "resume-run",
            mapOf(
                "title" to "Resume this run on {device}?",
                "titleNoDevice" to "Resume this run?",
                "body" to "The agent continues where it stopped, in the same worktree.",
            ),
            listOf("device"),
            "resume",
            "Resume",
        )
        fun prompt(device: String?) =
            if (device.isNullOrBlank()) spec.render("titleNoDevice", "body")
            else spec.render("title", "body", "device" to device)
    }

    object DeleteTrigger {
        val spec = destructiveSpec(
            "delete-trigger",
            mapOf("title" to "Delete this trigger?", "body" to "Past runs stay in Runs."),
            emptyList(),
            "delete",
            "Delete",
        )
        fun prompt() = spec.render("title", "body")
    }

    object DeleteAction {
        val spec = destructiveSpec(
            "delete-action",
            mapOf(
                "title" to "Delete \"{name}\"?",
                "body" to "Its triggers are deleted with it. Its runs stay and live ones keep going.",
            ),
            listOf("name"),
            "delete",
            "Delete",
        )
        fun prompt(name: String) = spec.render("title", "body", "name" to name)
    }

    object RemoveDevice {
        val spec = destructiveSpec(
            "remove-device",
            mapOf(
                "title" to "Remove \"{name}\"?",
                "body" to "A device with the daemon still running registers again on its next heartbeat.",
            ),
            listOf("name"),
            "remove",
            "Remove",
        )
        fun prompt(name: String) = spec.render("title", "body", "name" to name)
    }

    object DeleteTeam {
        val spec = destructiveSpec(
            "delete-team",
            mapOf("title" to "Delete \"{name}\"?", "body" to "All its boards, issues and files are deleted for good."),
            listOf("name"),
            "delete",
            "Delete",
        )
        fun prompt(name: String) = spec.render("title", "body", "name" to name)
    }

    object TrashBoard {
        val spec = destructiveSpec(
            "trash-board",
            mapOf(
                "title" to "Move \"{name}\" to trash?",
                "body" to "After 48 hours it is deleted with all its issues. " +
                    "Until then an owner can restore it in team settings on the web.",
            ),
            listOf("name"),
            "trash",
            "Move to trash",
        )
        fun prompt(name: String) = spec.render("title", "body", "name" to name)
    }

    object DeleteLabel {
        val spec = destructiveSpec(
            "delete-label",
            mapOf("title" to "Delete \"{name}\"?", "body" to "It comes off every issue that has it."),
            listOf("name"),
            "delete",
            "Delete",
        )
        fun prompt(name: String) = spec.render("title", "body", "name" to name)
    }

    object RemoveMember {
        val spec = destructiveSpec(
            "remove-member",
            mapOf("title" to "Remove {name} from this team?", "body" to "They lose access immediately."),
            listOf("name"),
            "remove",
            "Remove",
        )
        fun prompt(name: String) = spec.render("title", "body", "name" to name)
    }

    object LeaveTeam {
        val spec = destructiveSpec(
            "leave-team",
            mapOf("title" to "Leave \"{name}\"?", "body" to "You need a new invite to come back."),
            listOf("name"),
            "leave",
            "Leave",
        )
        fun prompt(name: String) = spec.render("title", "body", "name" to name)
    }

    object MakeOwner {
        val spec = primarySpec(
            "make-owner",
            mapOf(
                "title" to "Make {name} an owner?",
                "body" to "Owners can manage members and billing, delete boards and delete the team.",
            ),
            listOf("name"),
            "make-owner",
            "Make owner",
        )
        fun prompt(name: String) = spec.render("title", "body", "name" to name)
    }

    object MakeMember {
        val spec = primarySpec(
            "make-member",
            mapOf(
                "title" to "Make {name} a member?",
                "body" to "They can no longer manage members or delete boards.",
            ),
            listOf("name"),
            "make-member",
            "Make member",
        )
        fun prompt(name: String) = spec.render("title", "body", "name" to name)
    }

    object RemoveRepository {
        val spec = destructiveSpec(
            "remove-repository",
            mapOf("title" to "Remove {fullName} from this team?"),
            listOf("fullName"),
            "remove",
            "Remove",
        )
        fun prompt(fullName: String) = spec.render("title", null, "fullName" to fullName)
    }

    object UnlinkSignInMethod {
        val spec = destructiveSpec(
            "unlink-sign-in-method",
            mapOf(
                "title" to "Unlink {provider}?",
                "body" to "Your other sign-in methods keep working. You can link it again any time.",
            ),
            listOf("provider"),
            "unlink",
            "Unlink",
        )
        fun prompt(provider: String) = spec.render("title", "body", "provider" to provider)
    }

    object RemovePassword {
        val spec = destructiveSpec(
            "remove-password",
            mapOf("title" to "Remove your password?", "body" to "Your other sign-in methods keep working."),
            emptyList(),
            "remove",
            "Remove",
        )
        fun prompt() = spec.render("title", "body")
    }

    object RemovePasskey {
        val spec = destructiveSpec(
            "remove-passkey",
            mapOf(
                "title" to "Remove the passkey \"{name}\"?",
                "body" to "The copy on your device stays until you delete it there.",
            ),
            listOf("name"),
            "remove",
            "Remove",
        )

        /** [name] null or blank = the contract's "Passkey". */
        fun prompt(name: String?) =
            spec.render("title", "body", "name" to (name?.takeIf { it.isNotBlank() } ?: "Passkey"))
    }

    object DeleteAccount {
        val spec = destructiveSpec(
            "delete-account",
            mapOf(
                "title" to "Delete your account?",
                "titleOnServer" to "Delete your account on {server}?",
                "body" to "Teams where you are the only member are deleted with all their issues. " +
                    "In shared teams your issues stay and your comments are deleted.",
            ),
            listOf("server"),
            "delete",
            "Delete",
        )

        /** Android can hold several servers, so it names the one. */
        fun prompt(server: String) = spec.render("titleOnServer", "body", "server" to server)
    }

    object RemoveServer {
        val spec = destructiveSpec(
            "remove-server",
            mapOf(
                "title" to "Remove {server}?",
                "body" to "You are signed out and its data on this device is deleted. You can add it again any time.",
            ),
            listOf("server"),
            "remove",
            "Remove",
        )
        fun prompt(server: String) = spec.render("title", "body", "server" to server)
    }

    /** VAPP-91: a host asking before a surface acts as the person. Deny takes
     *  focus and Enter; Allow is the explicit `default` answer. */
    object ExponentialUiConsent {
        val spec = Spec(
            "exponential-ui-consent",
            mapOf(
                "title" to "Allow this surface to run {tool}?",
                "body" to "It acts as you, with your access to this team.",
            ),
            listOf("tool"),
            listOf(Action("deny", "Deny", Role.Cancel), Action("allow", "Allow", Role.Default)),
            focus = "deny",
        )
        fun prompt(tool: String) = spec.render("title", "body", "tool" to tool)
    }

    /** Every entry, keyed by its fixture id (the lock test walks it). Lazy:
     *  a nested object's first access initialises this one first. */
    val all: Map<String, Spec> by lazy {
        listOf(
            DeleteIssue.spec, DeleteIssues.spec, DeleteFile.spec, MoveIssue.spec, MergeIssuePr.spec,
            MergeRunPr.spec, MergeExternalPr.spec, StopRun.spec, ResumeRun.spec, DeleteTrigger.spec, DeleteAction.spec, RemoveDevice.spec,
            DeleteTeam.spec, TrashBoard.spec, DeleteLabel.spec, RemoveMember.spec, LeaveTeam.spec,
            MakeOwner.spec, MakeMember.spec, RemoveRepository.spec, UnlinkSignInMethod.spec,
            RemovePassword.spec, RemovePasskey.spec, DeleteAccount.spec, RemoveServer.spec,
            ExponentialUiConsent.spec,
        ).associateBy { it.id }
    }
}
