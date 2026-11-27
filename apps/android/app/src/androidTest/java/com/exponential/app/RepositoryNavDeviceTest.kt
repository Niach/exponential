package com.exponential.app

import android.content.Context
import android.graphics.Bitmap
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import com.exponential.app.domain.Chats
import com.exponential.app.domain.RepoPage
import com.exponential.app.domain.SidebarMenus
import com.exponential.app.ui.components.LocalOverlayToaster
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.ToastHost
import com.exponential.app.ui.components.Toaster
import com.exponential.app.ui.repo.NEW_REPOSITORY_TEST_ID
import com.exponential.app.ui.repo.NewRepositoryForm
import com.exponential.app.ui.repo.NewRepositoryPage
import com.exponential.app.ui.components.PhoneDock
import com.exponential.app.ui.components.PhoneMoreMenu
import com.exponential.app.ui.teams.TEAMS_REPOSITORY_SPECIMEN_TEST_ID
import com.exponential.app.ui.teams.TeamsRepositorySpecimenScreen
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.ExponentialTheme
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

/**
 * #EXP-2171 on a device, no backend: the Teams tab's repository rows (the
 * checks dot, the GitHub mark), a repository row's menu, the New › Repository
 * page (Create sends the form) and the phones' More menu. With
 * `-e proofShots true` each screen lands in the app's `files/repository-nav-proof/`.
 */
class RepositoryNavDeviceTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val context: Context get() = InstrumentationRegistry.getInstrumentation().targetContext

    private fun shot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("proofShots") != "true") return
        compose.waitForIdle()
        Thread.sleep(800)
        val bitmap = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot() ?: return
        val dir = File(context.filesDir, "repository-nav-proof").apply { mkdirs() }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun show(content: @androidx.compose.runtime.Composable () -> Unit) {
        compose.runOnUiThread {
            compose.activity.setContent {
                ExponentialTheme {
                    val toaster = remember { Toaster() }
                    CompositionLocalProvider(
                        LocalContentColor provides MaterialTheme.colorScheme.onBackground,
                        LocalToaster provides toaster,
                        LocalOverlayToaster provides toaster,
                    ) {
                        AppBackground {
                            Box(Modifier.fillMaxSize()) {
                                content()
                                ToastHost(toaster)
                            }
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun teamsTabRepositoryRowsAndMenu() {
        show { TeamsRepositorySpecimenScreen() }
        compose.onNodeWithTag(TEAMS_REPOSITORY_SPECIMEN_TEST_ID).assertExists()
        // Rows titled by the full name's last segment.
        compose.onNode(hasText("engine")).assertExists()
        compose.onNode(hasText("site")).assertExists()
        compose.onNode(hasTestTag("team-repository-github"), useUnmergedTree = true).assertExists()
        shot("android-teams-repositories")
        show { TeamsRepositorySpecimenScreen(menuOpen = true) }
        compose.onNode(hasTestTag("team-menu-new-repo-chat"), useUnmergedTree = true).assertExists()
        compose.onNode(hasText(SidebarMenus.COPY.getValue("copy-clone-url")), useUnmergedTree = true).assertExists()
        shot("android-repository-menu")
    }

    @Test
    fun newRepositoryPage() {
        var sent: NewRepositoryForm? = null
        show { NewRepositoryPage(busy = false, onBack = {}, onCreate = { sent = it }, github = {}) }
        compose.onNodeWithTag(NEW_REPOSITORY_TEST_ID).assertExists()
        compose.onNode(hasText(RepoPage.Copy.New.HOSTED)).assertExists()
        shot("android-new-repository")
        compose.onNodeWithTag("new-repo-create").performClick()
        // An empty name is refused: nothing was sent.
        assertEquals(null, sent)
    }

    @Test
    fun moreMenu() {
        // The Chats tab with More open: THE dock and its menu, as AppNavHost lays them.
        show {
            Box(Modifier.fillMaxSize()) {
                PhoneMoreMenu(onPick = {}, onDismiss = {})
                PhoneDock(
                    selected = "chats",
                    newOpen = false,
                    moreOpen = true,
                    badges = emptyMap(),
                    onSelect = {},
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }
        }
        compose.onNode(hasTestTag("${Chats.TestIds.MORE_MENU_ROW}-devices"), useUnmergedTree = true).assertExists()
        compose.onNode(hasTestTag("${Chats.TestIds.MORE_MENU_ROW}-settings"), useUnmergedTree = true).assertExists()
        compose.onAllNodes(hasText(Chats.Copy.TABS.getValue("more")), useUnmergedTree = true).onFirst().assertExists()
        shot("android-more-menu")
    }
}
