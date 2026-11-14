package com.exponential.app.ui.components

import android.graphics.Bitmap
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.Chats
import com.exponential.app.domain.SidebarFolders
import com.exponential.app.ui.apps.AppListEntry
import com.exponential.app.ui.apps.AppsListContent
import com.exponential.app.ui.components.picker.BoardPickerBoard
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.components.picker.boardPickerGroupedItems
import com.exponential.app.ui.components.picker.boardPickerItems
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.ExponentialTheme
import java.io.File
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * #EXP-1832: PR pictures of the phone's READ-ONLY sidebar folders, drawn by
 * the real composables over the demo seed's layout (boards: Mobile App, the
 * "Product" folder with Launch Marketing + Product Feedback; apps: the "Ops
 * tools" folder with Team dashboard + Working hours). Test-only (never in the
 * app); run it with `-e class` and pull `files/sidebar-folders/<name>.png`.
 */
@RunWith(AndroidJUnit4::class)
class SidebarFoldersPicturesTest {

    @get:Rule
    val composeRule = createComposeRule()

    private val boards = listOf(
        board("app", "Mobile App", "code", "#6366f1", 1.0),
        board("mkt", "Launch Marketing", "square-kanban", "#f59e0b", 2.0),
        board("fb", "Product Feedback", "megaphone", "#22c55e", 3.0),
        board("ops", "Operations", "server", "#06b6d4", 4.0),
        board("res", "Research", "flask-conical", "#a855f7", 5.0),
        board("web", "Website", "globe", "#ef4444", 6.0),
    )

    private val boardEntries = listOf(
        entry("e-app", "item", "app", null, null, 1.0),
        entry("f-product", "folder", null, null, "Product", 2.0),
        entry("e-mkt", "item", "mkt", "f-product", null, 1.0),
        entry("e-fb", "item", "fb", "f-product", null, 2.0),
        entry("f-infra", "folder", null, null, "Infrastructure", 3.0),
        entry("e-ops", "item", "ops", "f-infra", null, 1.0),
        entry("e-res", "item", "res", "f-infra", null, 2.0),
    )

    private fun board(id: String, name: String, icon: String, color: String, order: Double) = BoardEntity(
        id = id, teamId = "t1", name = name, slug = id, prefix = id.uppercase(), color = color, icon = icon,
        sortOrder = order, createdAt = "", updatedAt = "",
    )

    private fun entry(id: String, kind: String, target: String?, parent: String?, name: String?, order: Double) =
        SidebarFolders.Entry(id, kind, target, parent, name, order)

    private fun boardRows(closed: Set<String>) =
        SidebarFolders.rows(SidebarFolders.tree(boards.map { it.id }, boardEntries)) { it !in closed }

    private fun capture(name: String) {
        composeRule.waitForIdle()
        Thread.sleep(800)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val bitmap = instrumentation.uiAutomation.takeScreenshot()
        val dir = File(instrumentation.targetContext.filesDir, "sidebar-folders").apply { mkdirs() }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    @Test
    fun boardPickerAndBoardMenu() {
        var shot by androidx.compose.runtime.mutableStateOf("picker")
        composeRule.setContent {
            ExponentialTheme {
                AppBackground {
                    if (shot == "picker") {
                        val pickerBoards = boards.map {
                            BoardPickerBoard(it.id, it.name, it.icon, it.color, teamId = it.teamId)
                        }
                        Picker(
                            items = boardPickerItems(pickerBoards),
                            groupedItems = boardPickerGroupedItems(pickerBoards, boardRows(closed = setOf("f-infra"))),
                            mode = PickerMode.Single,
                            value = setOf("app"),
                            onChange = {},
                            search = true,
                            emptyText = "No boards",
                            title = "Move to board",
                            open = true,
                        )
                    } else {
                        Box(Modifier.fillMaxSize()) {
                            Column(Modifier.fillMaxWidth().statusBarsPadding()) {
                                PageHeader(title = "Mobile App", actions = PageHeaderActions({}, {}, inboxUnread = 0))
                            }
                            BottomNavBar(
                                selected = "board",
                                agentActive = false,
                                board = boards.first(),
                                onBoardView = true,
                                team = TeamEntity(id = "t1", name = "Acme", slug = "acme", createdAt = "", updatedAt = ""),
                                composeEnabled = true,
                                onSelectTab = {},
                                onCompose = {},
                                onChat = {},
                                modifier = Modifier.align(Alignment.BottomCenter),
                                boardMenu = PhoneBoardMenu.boards(
                                    boards = boards,
                                    activeBoardId = "app",
                                    onSelect = {},
                                    onNewBoard = {},
                                    folders = SidebarFolderLayout(boardRows(closed = setOf("f-infra"))) {},
                                ),
                                menuOpen = true,
                            )
                        }
                    }
                }
            }
        }
        capture("board-picker")
        shot = "menu"
        capture("board-menu")
    }

    @Test
    fun appsList() {
        val apps = listOf(
            app("dash", "Team dashboard", "layout-dashboard", "#6366f1", 2),
            app("hours", "Working hours", "clock", "#22c55e", 1),
            app("standup", "Standup", "users", "#06b6d4", 0),
            app("invoices", "Invoices", "receipt", "#f59e0b", 3),
            app("crm", "CRM", "contact", "#ef4444", 1),
        )
        val entries = listOf(
            entry("f-ops", "folder", null, null, "Ops tools", 1.0),
            entry("e-dash", "item", "dash", "f-ops", null, 1.0),
            entry("e-hours", "item", "hours", "f-ops", null, 2.0),
            entry("f-sales", "folder", null, null, "Sales", 2.0),
            entry("e-inv", "item", "invoices", "f-sales", null, 1.0),
            entry("e-crm", "item", "crm", "f-sales", null, 2.0),
        )
        val rows = SidebarFolders.rows(SidebarFolders.tree(apps.sortedBy { it.name.lowercase() }.map { it.id }, entries)) { it != "f-sales" }
        composeRule.setContent {
            ExponentialTheme {
                AppBackground {
                    Column(Modifier.fillMaxSize().statusBarsPadding()) {
                        PageHeader(title = Chats.Copy.HEADERS.getValue("apps"))
                        Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
                            AppsListContent(
                                apps = apps,
                                folders = SidebarFolderLayout(rows) {},
                                canManage = { true },
                                onOpen = {},
                                onNew = {},
                                onDelete = {},
                            )
                        }
                    }
                }
            }
        }
        capture("apps")
    }

    private fun app(id: String, name: String, icon: String, color: String, version: Int) = AppListEntry(
        id = id, slug = id, name = name, icon = icon, color = color, description = "", headVersion = version,
        createdBy = null, teamId = "t1", packageJson = null, publishedVersion = version,
    )
}
