package com.exponential.app.widget

import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.Context
import android.os.Bundle
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent

/**
 * #EXP-1830 (home-widgets.json `kinds`): the three home screen widgets. Each
 * provider only asks [HomeWidgetsRefresher] to draw; the refresher owns the
 * data (the active account's database, in-process) and the RemoteViews.
 */
enum class HomeWidgetKind(val wire: String) {
    Board("board"),
    MyIssues("my-issues"),
    NeedsYou("needs-you"),
    ;

    fun providerClass(): Class<out AppWidgetProvider> = when (this) {
        Board -> BoardWidgetProvider::class.java
        MyIssues -> MyIssuesWidgetProvider::class.java
        NeedsYou -> NeedsYouWidgetProvider::class.java
    }
}

@EntryPoint
@InstallIn(SingletonComponent::class)
interface HomeWidgetsEntryPoint {
    fun homeWidgetsRefresher(): HomeWidgetsRefresher
}

internal fun homeWidgetsRefresher(context: Context): HomeWidgetsRefresher =
    EntryPointAccessors.fromApplication(context.applicationContext, HomeWidgetsEntryPoint::class.java)
        .homeWidgetsRefresher()

/** The shared provider: every system callback = draw these ids again. */
abstract class HomeWidgetProvider(private val kind: HomeWidgetKind) : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        homeWidgetsRefresher(context).draw(kind, ids)
    }

    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) {
        homeWidgetsRefresher(context).draw(kind, intArrayOf(id))
    }

    override fun onDeleted(context: Context, ids: IntArray) {
        ids.forEach { HomeWidgetPrefs.clear(context, it) }
    }
}

class BoardWidgetProvider : HomeWidgetProvider(HomeWidgetKind.Board)

class MyIssuesWidgetProvider : HomeWidgetProvider(HomeWidgetKind.MyIssues)

class NeedsYouWidgetProvider : HomeWidgetProvider(HomeWidgetKind.NeedsYou)

/** A widget's pick (home-widgets.json: the board, the teams), per widget id. */
object HomeWidgetPrefs {
    private const val PREFS = "home_widgets"

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun board(context: Context, id: Int): String? = prefs(context).getString("board:$id", null)

    fun setBoard(context: Context, id: Int, boardId: String) {
        prefs(context).edit().putString("board:$id", boardId).apply()
    }

    /** None picked = every team. */
    fun teams(context: Context, id: Int): List<String> =
        prefs(context).getString("teams:$id", null)?.split(',')?.filter { it.isNotEmpty() }.orEmpty()

    fun setTeams(context: Context, id: Int, teamIds: List<String>) {
        prefs(context).edit().putString("teams:$id", teamIds.joinToString(",")).apply()
    }

    fun clear(context: Context, id: Int) {
        prefs(context).edit().remove("board:$id").remove("teams:$id").apply()
    }
}
