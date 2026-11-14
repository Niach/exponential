package com.exponential.app.widget

import android.appwidget.AppWidgetManager
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.HomeWidgets
import com.exponential.app.ui.components.boardIconName
import com.exponential.app.ui.components.picker.BoardPickerBoard
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.components.picker.boardPickerItems
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.ExponentialTheme
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent

@EntryPoint
@InstallIn(SingletonComponent::class)
interface HomeWidgetConfigEntryPoint {
    fun authRepository(): AuthRepository
    fun databaseHolder(): DatabaseHolder
}

/**
 * #EXP-1830 (home-widgets.json): the pick when a widget is added (or
 * reconfigured): THE picker sheet over the app background, no new look.
 * `board` = one board of the ACTIVE account in sidebar order; `my-issues` =
 * its teams, several (none = every team). Dismissing saves the pick.
 */
abstract class HomeWidgetConfigActivity(private val kind: HomeWidgetKind) : ComponentActivity() {
    private var widgetId = AppWidgetManager.INVALID_APPWIDGET_ID

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        widgetId = intent?.extras?.getInt(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
            ?: AppWidgetManager.INVALID_APPWIDGET_ID
        setResult(RESULT_CANCELED, result())
        if (widgetId == AppWidgetManager.INVALID_APPWIDGET_ID) {
            finish()
            return
        }
        val entry = EntryPointAccessors.fromApplication(applicationContext, HomeWidgetConfigEntryPoint::class.java)
        val db = accountDatabaseFlow(entry.authRepository(), entry.databaseHolder())
        val boards = db.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() }
        val teams = db.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() }
        setContent {
            ExponentialTheme {
                AppBackground {
                    Box(Modifier.fillMaxSize()) {
                        val boardRows by boards.collectAsState(initial = emptyList())
                        val teamRows by teams.collectAsState(initial = emptyList())
                        when (kind) {
                            HomeWidgetKind.Board -> BoardPick(boardRows, teamRows)
                            HomeWidgetKind.MyIssues -> TeamsPick(teamRows)
                            HomeWidgetKind.NeedsYou -> Unit
                        }
                    }
                }
            }
        }
    }

    @Composable
    private fun BoardPick(boards: List<BoardEntity>, teams: List<TeamEntity>) {
        val teamRank = teams.sortedBy { it.name.lowercase() }.withIndex().associate { (i, t) -> t.id to i }
        val ordered = boards.filter { it.deletedAt == null }.sortedWith(
            compareBy<BoardEntity> { teamRank[it.teamId] ?: Int.MAX_VALUE }.thenBy { it.sortOrder }.thenBy { it.name },
        )
        var value by remember { mutableStateOf(HomeWidgetPrefs.board(this, widgetId)) }
        Picker(
            items = boardPickerItems(
                ordered.map { BoardPickerBoard(it.id, it.name, boardIconName(it.icon, it.repositoryId), it.color) },
            ),
            mode = PickerMode.Single,
            value = setOfNotNull(value),
            onChange = { picked ->
                picked.firstOrNull()?.let {
                    value = it
                    HomeWidgetPrefs.setBoard(this, widgetId, it)
                }
            },
            search = true,
            title = getString(com.exponential.app.R.string.home_widget_pick_board),
            emptyText = HomeWidgets.EMPTY_ISSUES,
            open = true,
            onOpenChange = { open -> if (!open) save() },
        )
    }

    @Composable
    private fun TeamsPick(teams: List<TeamEntity>) {
        var value by remember { mutableStateOf(HomeWidgetPrefs.teams(this, widgetId).toSet()) }
        Picker(
            items = teams.sortedBy { it.name.lowercase() }.map { PickerItem(value = it.id, label = it.name) },
            mode = PickerMode.Multi,
            value = value,
            onChange = { picked ->
                value = picked
                HomeWidgetPrefs.setTeams(this, widgetId, picked.toList())
            },
            title = getString(com.exponential.app.R.string.home_widget_pick_teams),
            open = true,
            onOpenChange = { open -> if (!open) save() },
        )
    }

    private fun save() {
        homeWidgetsRefresher(this).draw(kind, intArrayOf(widgetId))
        setResult(RESULT_OK, result())
        finish()
    }

    private fun result() = Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId)
}

class BoardWidgetConfigActivity : HomeWidgetConfigActivity(HomeWidgetKind.Board)

class MyIssuesWidgetConfigActivity : HomeWidgetConfigActivity(HomeWidgetKind.MyIssues)
