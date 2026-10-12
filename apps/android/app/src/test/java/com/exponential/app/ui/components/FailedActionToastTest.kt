package com.exponential.app.ui.components

import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.ToastKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** FEED-79 (EXP-1259 D10): the failed-action toast replays `feedback-states.json` `errors.cases`. */
class FailedActionToastTest {
    @Test
    fun aFailedSaveIsAnErrorToastWithRetry() {
        val toaster = Toaster()
        var retried = 0
        toaster.failedAction("save", "the team prompt", "  The server said no  ") { retried++ }
        val toast = toaster.toasts.single()
        assertEquals(ToastKind.Error, toast.kind)
        assertEquals("Couldn't save the team prompt", toast.title)
        assertEquals("The server said no", toast.description)
        assertEquals("Retry", toast.action?.label)
        toast.action?.onClick?.invoke()
        assertEquals(1, retried)
    }

    @Test
    fun aFailedDeleteHasNoRetry() {
        val toaster = Toaster()
        toaster.failedAction("delete", "the board", "Boards with open runs can't be deleted")
        val toast = toaster.toasts.single()
        assertEquals("Couldn't delete the board", toast.title)
        assertNull(toast.action)
    }

    @Test
    fun theFixtureTitles() {
        val toaster = Toaster()
        toaster.failedAction("start", "the session")
        toaster.failedAction("connect", "GitHub")
        assertEquals(
            listOf("Couldn't start the session", "Couldn't connect GitHub"),
            toaster.toasts.map { it.title },
        )
    }

    @Test
    fun aBlankServerMessageIsNoDescription() {
        val toaster = Toaster()
        toaster.failedAction("load", "the changes", "   ")
        toaster.failedAction("load", "the preview", null)
        assertEquals(listOf(null, null), toaster.toasts.map { it.description })
    }

    // The fixture's `background` case (`FeedbackMarkTest` replays every kind):
    // automatic work never reaches `failedAction`, so it raises no toast.
    @Test
    fun backgroundWorkMapsToNoToast() {
        assertEquals("quiet", FeedbackStates.errorSurface("background"))
        assertEquals("toast", FeedbackStates.errorSurface("action"))
        assertEquals("inline", FeedbackStates.errorSurface("input"))
    }
}
