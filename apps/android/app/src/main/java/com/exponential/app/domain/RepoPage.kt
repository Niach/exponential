package com.exponential.app.domain

/**
 * #EXP-1616 THE REPO PAGE ×4 (fixtures/repo-page.json: its `$comment` is the
 * spec). Pure folds every host replays: the PR tree ([repoPrTree]), the
 * branch list ([repoBranches]) and a repo row's checks rollup on Settings ›
 * Repositories ([repoChecksRollup]). Mirrors the TS reference
 * `packages/domain-contract/src/repo-page.ts`; `RepoPageTest` replays every
 * case, comparing the whole result.
 */
object RepoPage {

    /** repo-page.json `tabs` ids. */
    const val TAB_PULLS = "pulls"
    const val TAB_RUNS = "runs"
    const val TAB_BRANCHES = "branches"
    const val TAB_SETTINGS = "settings"

    data class Tab(val id: String, val label: String)

    /** repo-page.json `tabs`, in order. */
    val TABS: List<Tab> = listOf(
        Tab(TAB_PULLS, "Pull requests"),
        Tab(TAB_RUNS, "Runs"),
        Tab(TAB_BRANCHES, "Branches"),
        Tab(TAB_SETTINGS, "Settings"),
    )

    /** repo-page.json `copy`. */
    object Copy {
        object Provider {
            const val GITHUB = "GitHub"
            const val ECODE = "Exponential"
        }

        object Add {
            const val TITLE = "Add repository"
            const val GITHUB = "GitHub"
            const val ECODE = "Exponential"
            const val NAME = "Name"
            const val DEFAULT_BRANCH = "Default branch"
            const val MIRROR = "Mirror to GitHub"
            const val MIRROR_URL = "Mirror URL"
            const val CANCEL = "Cancel"
            const val CREATE = "Create"
        }

        object Pulls {
            const val EMPTY = "No open pull requests"
        }

        object Runs {
            const val EMPTY = "No runs yet"
        }

        object Branches {
            const val EMPTY = "No branches yet"
            const val DEFAULT = "Default"
        }

        object Settings {
            const val PROVIDER = "Provider"
            const val DEFAULT_BRANCH = "Default branch"
            const val CLONE_URL = "Clone URL"
            const val BACKUPS = "Backups"
            const val NO_BACKUPS = "No backups yet"
            const val RESTORE = "Restore…"
            const val RESTORE_TITLE = "Restore this backup?"
            const val RESTORE_CONFIRM = "Restore"
            const val MIRROR = "Mirror"
            const val MIRROR_URL = "Mirror URL"
        }

        data class Check(val label: String, val tone: String)

        /** `copy.checks`: a rollup state's chip label and dot tone (green | amber | red). */
        val CHECKS: Map<String, Check> = linkedMapOf(
            "success" to Check("Passing", "green"),
            "pending" to Check("Running", "amber"),
            "failure" to Check("Failing", "red"),
        )
    }

    /** `copy.provider` for a repo's provider (`ecode` | anything else = GitHub). */
    fun providerLabel(provider: String?): String =
        if (provider == PROVIDER_ECODE) Copy.Provider.ECODE else Copy.Provider.GITHUB

    const val PROVIDER_GITHUB = "github"
    const val PROVIDER_ECODE = "ecode"

    /** The Add form's default-branch options (`main` default). */
    val ADD_BRANCHES: List<String> = listOf("main", "master", "develop")

    /** The Add form's name rule: [A-Za-z0-9._-], not dot-led, no `.git` tail. */
    fun isValidName(name: String): Boolean =
        name.isNotEmpty() && name.all { it.isLetterOrDigit() && it.code < 128 || it == '.' || it == '_' || it == '-' } &&
            !name.startsWith(".") && !name.endsWith(".git")

    data class Pr(
        val id: String,
        val number: Int,
        val title: String?,
        val head: String,
        val base: String,
        val state: String,
        val checksState: String,
    )

    data class PrRow(
        val id: String,
        val number: Int,
        /** `#<number>`. */
        val identifier: String,
        val title: String,
        val depth: Int,
        /** `<head> → <base>`. */
        val subtitle: String,
        val draft: Boolean,
        /** The rollup chip; null = no checks (no chip). */
        val checks: String?,
    )

    data class BranchRow(
        val name: String,
        /** The first 7 characters of the sha. */
        val sha: String,
        val isDefault: Boolean,
        /** The live PR whose head this branch is, null = none. */
        val prNumber: Int?,
    )

    private val LIVE = setOf("open", "draft")

    /** Code-point order (Rust's `str` order). */
    private val byCodePoint = Comparator<String> { a, b ->
        val x = a.codePoints().toArray()
        val y = b.codePoints().toArray()
        for (i in 0 until minOf(x.size, y.size)) {
            val d = x[i] - y[i]
            if (d != 0) return@Comparator if (d < 0) -1 else 1
        }
        x.size - y.size
    }

    /** The live PRs as a tree: a PR's parent is the live PR whose head is its
     *  base (lowest number when two share a head). Roots newest first;
     *  children oldest first, depth-first; a loop starts at its newest. */
    fun repoPrTree(prs: List<Pr>): List<PrRow> {
        val live = prs.filter { it.state in LIVE }
        val byHead = LinkedHashMap<String, Pr>()
        for (pr in live.sortedBy { it.number }) if (!byHead.containsKey(pr.head)) byHead[pr.head] = pr
        fun parentOf(pr: Pr): Pr? = byHead[pr.base]?.takeIf { it.id != pr.id }
        val children = LinkedHashMap<String, MutableList<Pr>>()
        val roots = mutableListOf<Pr>()
        for (pr in live) {
            val parent = parentOf(pr)
            if (parent != null) children.getOrPut(parent.id) { mutableListOf() }.add(pr) else roots.add(pr)
        }
        val rows = mutableListOf<PrRow>()
        val seen = HashSet<String>()
        fun visit(pr: Pr, depth: Int) {
            if (!seen.add(pr.id)) return
            rows.add(
                PrRow(
                    id = pr.id,
                    number = pr.number,
                    identifier = "#${pr.number}",
                    title = pr.title ?: "",
                    depth = depth,
                    subtitle = "${pr.head} → ${pr.base}",
                    draft = pr.state == "draft",
                    checks = if (pr.checksState == "none") null else pr.checksState,
                ),
            )
            for (child in (children[pr.id] ?: emptyList<Pr>()).sortedBy { it.number }) visit(child, depth + 1)
        }
        for (root in roots.sortedByDescending { it.number }) visit(root, 0)
        // A loop of live PRs (no root): its newest member starts it at depth 0.
        for (pr in live.sortedByDescending { it.number }) visit(pr, 0)
        return rows
    }

    /** The default branch first, then by name (code points). */
    fun repoBranches(branches: List<Pair<String, String>>, defaultBranch: String, prs: List<Pr>): List<BranchRow> {
        fun prOf(name: String): Int? =
            prs.filter { it.state in LIVE && it.head == name }.minByOrNull { it.number }?.number
        return branches
            .sortedWith { a, b ->
                when {
                    a.first == defaultBranch && b.first == defaultBranch -> 0
                    a.first == defaultBranch -> -1
                    b.first == defaultBranch -> 1
                    else -> byCodePoint.compare(a.first, b.first)
                }
            }
            .map { (name, sha) -> BranchRow(name, sha.take(7), name == defaultBranch, prOf(name)) }
    }

    /** A repo row's checks chip over its live PRs: failure (any) · pending
     *  (any) · success (all of the ones with checks) · null = none has checks. */
    fun repoChecksRollup(prs: List<Pr>): String? {
        val states = prs.filter { it.state in LIVE && it.checksState != "none" }.map { it.checksState }
        return when {
            states.isEmpty() -> null
            "failure" in states -> "failure"
            "pending" in states -> "pending"
            else -> "success"
        }
    }

    /** The tab a route's `tab` param opens (unknown = pulls). */
    fun repoTab(param: String?): String = TABS.firstOrNull { it.id == param }?.id ?: TAB_PULLS
}
