package com.exponential.app.data.chatstore

import android.content.Context
import android.content.SharedPreferences
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * #EDE-114 (chat-store.json THE STORAGE SETTING): `Keep tool steps`, per
 * device: how long the store keeps tool steps (prose and messages stay
 * regardless). `null` days = Forever.
 */
object KeepToolSteps {
    const val LABEL = "Keep tool steps"
    const val TEST_ID = "settings-keep-tool-steps"

    /** chat-store.json `constants.retentionDays.phone` (= the facade's `phoneRetentionDays()`). */
    const val DEFAULT_DAYS = 30

    /** One option: [days] null = Forever. */
    data class Option(val days: Int?, val label: String) {
        /** The picker's value (`forever` for null). */
        val id: String get() = days?.toString() ?: FOREVER
    }

    private const val FOREVER = "forever"

    val OPTIONS = listOf(
        Option(7, "7 days"),
        Option(30, "30 days"),
        Option(90, "90 days"),
        Option(365, "1 year"),
        Option(null, "Forever"),
    )

    fun option(days: Int?): Option = OPTIONS.firstOrNull { it.days == days } ?: OPTIONS.first { it.days == DEFAULT_DAYS }

    fun fromId(id: String): Option? = OPTIONS.firstOrNull { it.id == id }

    /** The stored value → days (absent = the default; -1 = Forever). */
    fun decode(stored: Int?): Int? = when {
        stored == null -> DEFAULT_DAYS
        stored < 0 -> null
        else -> option(stored).days
    }

    fun encode(days: Int?): Int = days ?: -1
}

/** Where the setting lives: this device's prefs (never synced). */
interface KeepToolStepsPrefs {
    fun read(): Int?
    fun write(value: Int)
}

/** #EDE-114: the setting, persisted per device and applied at once ([ChatStoreManager] collects [days]). */
@Singleton
class ChatStoreSettings internal constructor(private val prefs: KeepToolStepsPrefs) {
    @Inject constructor(@ApplicationContext context: Context) : this(SharedKeepToolStepsPrefs(context))

    private val _days = MutableStateFlow(KeepToolSteps.decode(prefs.read()))

    /** The window in days; null = Forever. */
    val days: StateFlow<Int?> = _days.asStateFlow()

    fun setDays(days: Int?) {
        val option = KeepToolSteps.option(days)
        prefs.write(KeepToolSteps.encode(option.days))
        _days.value = option.days
    }
}

private class SharedKeepToolStepsPrefs(context: Context) : KeepToolStepsPrefs {
    private val prefs: SharedPreferences = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    override fun read(): Int? = if (prefs.contains(KEY)) prefs.getInt(KEY, KeepToolSteps.DEFAULT_DAYS) else null

    override fun write(value: Int) {
        prefs.edit().putInt(KEY, value).apply()
    }

    private companion object {
        const val PREFS = "exp.chatStore"
        const val KEY = "keepToolStepsDays"
    }
}
