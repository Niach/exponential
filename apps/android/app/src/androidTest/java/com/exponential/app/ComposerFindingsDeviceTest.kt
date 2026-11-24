package com.exponential.app

import android.content.Context
import android.graphics.Bitmap
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import androidx.room.Room
import androidx.test.platform.app.InstrumentationRegistry
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.PromptEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.TeamMemberEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.EntityRef
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.ui.agent.AgentIssueInsertPage
import com.exponential.app.ui.agent.issueOption
import com.exponential.app.ui.markdown.AutocompleteRows
import com.exponential.app.ui.markdown.chatTeamIssueRefTargets
import com.exponential.app.ui.markdown.mentionCandidatesFor
import com.exponential.app.ui.markdown.mentionMembers
import com.exponential.app.ui.session.EntityRefPreviewSheet
import com.exponential.app.ui.session.EntityRefResolver
import com.exponential.app.ui.session.LocalEntityRefResolver
import com.exponential.app.ui.theme.ExponentialTheme
import java.io.File
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test

/**
 * #EXP-2095, #EXP-2096, #EXP-2097 on a device, no backend (seeded rows in an
 * in-memory database): the Assistant's chat (its session sits in the personal
 * team `p-me`, never synced; its chat teams are `t-a`, `t-b`) draws the `@`
 * list in the ONE order, the `+` › Issue rows with each issue's own status
 * glyph, and the board / prompt / label / member preview cards from the
 * entity's own team. With `-e proofShots true` each state lands in the app's
 * `files/composer-proof/`.
 */
class ComposerFindingsDeviceTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val context: Context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private lateinit var db: ExponentialDatabase

    private val boards = listOf(board("ba", "t-a", "Web"), board("bb", "t-b", "Mobile"))
    private val statuses = listOf(
        status("sb-backlog", "t-b", IssueStatusCategory.Backlog, "Backlog", "backlog", 0.0),
        status("sb-progress", "t-b", IssueStatusCategory.Started, "In Progress", "in_progress", 1.0),
        status("sb-qa", "t-b", IssueStatusCategory.Started, "QA", null, 2.0),
        status("sb-review", "t-b", IssueStatusCategory.Started, "In Review", "in_review", 3.0),
        status("sb-done", "t-b", IssueStatusCategory.Completed, "Done", "done", 4.0),
    )
    private val issues = listOf(
        issue("i1", 1, "Composer loses its draft on rotate", "backlog", "sb-backlog"),
        issue("i2", 2, "Voice wave stutters on cold start", "in_progress", "sb-progress"),
        issue("i3", 3, "Share sheet misses the board picker", "in_progress", "sb-qa"),
        issue("i4", 4, "Inbox badge counts muted chats", "in_review", "sb-review"),
        issue("i5", 5, "Widget keeps a stale sign-in", "done", "sb-done"),
    )

    // The roster (join) order of the issue: Jonas, Sofia, Alex, Hannes, Mira.
    private val roster = listOf(
        user("u-j", "Jonas Weber", "jonas@example.com"),
        user("u-s", "Sofia Marin", "sofia@example.com"),
        user("u-a", "Alex Kim", "alex@example.com"),
        user("u-h", "Hannes Berg", "hannes@example.com"),
        user("u-m", "Mira Okafor", "mira@example.com"),
    )

    @Before
    fun seed() {
        db = Room.inMemoryDatabaseBuilder(context, ExponentialDatabase::class.java).build()
        runBlocking {
            db.teamDao().upsert(TeamEntity(id = "t-a", name = "Acme", slug = "acme", createdAt = STAMP, updatedAt = STAMP))
            db.teamDao().upsert(TeamEntity(id = "t-b", name = "Birch", slug = "birch", createdAt = STAMP, updatedAt = STAMP))
            boards.forEach { db.boardDao().upsert(it) }
            statuses.forEach { db.issueStatusDao().upsert(it) }
            issues.forEach { db.issueDao().upsert(it) }
            roster.forEach { db.userDao().upsert(it) }
            db.teamMemberDao().upsert(TeamMemberEntity("m1", "t-b", "u-s", "owner", STAMP, STAMP))
            db.teamMemberDao().upsert(TeamMemberEntity("m2", "t-a", "u-j", "member", STAMP, STAMP))
            db.promptDao().upsert(PromptEntity(id = "pb", teamId = "t-b", name = "Triage the inbox"))
            db.labelDao().upsert(LabelEntity("lb", "t-b", "regression", "#ef4444", 0.0, STAMP, STAMP))
        }
    }

    @After
    fun close() {
        db.close()
    }

    private fun shot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("proofShots") != "true") return
        compose.waitForIdle()
        Thread.sleep(800)
        val bitmap = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot() ?: return
        val dir = File(context.filesDir, "composer-proof").apply { mkdirs() }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun show(content: @Composable () -> Unit) {
        compose.runOnUiThread {
            compose.activity.setContent {
                ExponentialTheme {
                    CompositionLocalProvider(
                        LocalContentColor provides MaterialTheme.colorScheme.onBackground,
                        // The Assistant's chat: the session's team is the
                        // personal one, its lists read the chat teams.
                        LocalEntityRefResolver provides EntityRefResolver(flowOf(db), "p-me", "u-me", listOf("t-a", "t-b")),
                    ) {
                        Box(Modifier.fillMaxSize().statusBarsPadding()) { content() }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun waitFor(text: String) {
        compose.waitUntil(15_000) { compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
    }

    @Test
    fun theAtListIsInTheOneOrder() {
        val members = mentionMembers(roster)
        org.junit.Assert.assertEquals(listOf("Alex Kim", "Hannes Berg", "Jonas Weber", "Mira Okafor", "Sofia Marin"), members.map { it.name })
        show {
            AutocompleteRows(
                mentionCandidates = mentionCandidatesFor(members, ""),
                refCandidates = emptyList(),
                emojiCandidates = emptyList(),
                selectedIndex = 0,
                onPickMention = {},
                onPickIssueRef = {},
                onPickEmoji = {},
                modifier = Modifier.fillMaxWidth().padding(16.dp),
            )
        }
        waitFor("Sofia Marin")
        shot("mention-order")
    }

    @Test
    fun theIssueRowsReadEachIssuesOwnStatus() {
        val options = chatTeamIssueRefTargets(issues, boards, statuses, listOf("t-a", "t-b")).map(::issueOption)
        org.junit.Assert.assertEquals(
            listOf("Done", "In Review", "QA", "In Progress", "Backlog"),
            options.map { it.glyph.name },
        )
        show { AgentIssueInsertPage(issues = options, onPick = {}) }
        waitFor("Voice wave stutters")
        shot("issue-rows")
    }

    private fun card(ref: EntityRef, expect: String, name: String) {
        show { EntityRefPreviewSheet(ref = ref, members = emptyList(), onDismiss = {}) }
        waitFor(expect)
        shot(name)
    }

    @Test
    fun aBoardCardReadsItsOwnTeam() = card(EntityRef("board", "bb", title = "Mobile"), "4 open issues", "card-board")

    @Test
    fun aPromptCardReadsItsOwnTeam() = card(EntityRef("prompt", "pb"), "Triage the inbox", "card-prompt")

    @Test
    fun aLabelCardReadsItsOwnTeam() = card(EntityRef("label", "lb"), "regression", "card-label")

    @Test
    fun aMemberCardReadsTheChatTeams() = card(EntityRef("member", "u-s", title = "Sofia Marin"), "Owner", "card-member")

    private companion object {
        const val STAMP = "2026-11-01T00:00:00Z"

        fun board(id: String, teamId: String, name: String) = BoardEntity(
            id = id, teamId = teamId, name = name, slug = id, prefix = "MOB", color = "#6366f1",
            sortOrder = 0.0, createdAt = STAMP, updatedAt = STAMP,
        )

        fun status(id: String, teamId: String, category: IssueStatusCategory, name: String, builtinKey: String?, sortOrder: Double) =
            IssueStatusEntity(
                id = id, teamId = teamId, category = category.wire, name = name, sortOrder = sortOrder,
                builtinKey = builtinKey, createdAt = STAMP, updatedAt = STAMP,
            )

        fun issue(id: String, number: Int, title: String, status: String, statusId: String) = IssueEntity(
            id = id, boardId = "bb", number = number, identifier = "MOB-$number", title = title,
            status = status, statusId = statusId, priority = "none", creatorId = "u-s", sortOrder = number.toDouble(),
            createdAt = "2026-11-0${number}T00:00:00Z", updatedAt = STAMP,
        )

        fun user(id: String, name: String, email: String) = UserEntity(id = id, name = name, email = email, createdAt = STAMP, updatedAt = STAMP)
    }
}
