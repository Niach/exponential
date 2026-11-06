package at.exponential.ui

import at.exponential.ui.compose.dialogHoldOffset
import org.junit.Assert.assertEquals
import org.junit.Test

/** EXP-1676: a native Dialog window holds its bottom edge as it grows. */
class DialogHoldTest {
    @Test
    fun aGrownDialogMovesUpByHalfItsGrowth() {
        assertEquals("as opened: centred", 0, dialogHoldOffset(200f, 200f, 2f, 600f))
        // 60 dp taller: up 30 dp (60 px at density 2), the bottom edge holds.
        assertEquals(-60, dialogHoldOffset(260f, 200f, 2f, 600f))
        // Shrunk 40 dp: down 20 dp.
        assertEquals(40, dialogHoldOffset(160f, 200f, 2f, 600f))
    }

    @Test
    fun itNeverRisesPastTheTopPadding() {
        // 580 dp in a 600 dp frame: centred top = 10 dp, so it may rise 2 dp.
        assertEquals(-2, dialogHoldOffset(580f, 200f, 1f, 600f))
        assertEquals("no frame known: no clamp", -190, dialogHoldOffset(580f, 200f, 1f, 0f))
        assertEquals("not held yet", 0, dialogHoldOffset(580f, 0f, 1f, 600f))
    }
}
