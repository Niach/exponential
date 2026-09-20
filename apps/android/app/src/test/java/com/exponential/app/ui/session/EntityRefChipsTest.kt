package com.exponential.app.ui.session

import org.junit.Assert.assertEquals
import org.junit.Test

/** EXP-920: the card excerpt reads a GFM body as one plain line. */
class EntityRefChipsTest {

    @Test
    fun `a body excerpt strips marks, links and fences and cuts at the cap`() {
        val body = """
            ## Reviewed the fix

            The **debounce** is fine, see [the trace](/api/attachments/1) and ![shot](/api/attachments/2).

            ```ts
            const x = 1
            ```

            - one
            - two
        """.trimIndent()
        assertEquals(
            "Reviewed the fix The debounce is fine, see the trace and . one two",
            plainExcerpt(body, 240),
        )
        assertEquals("Reviewed…", plainExcerpt(body, 10))
        assertEquals("🚀🚀🚀…", plainExcerpt("🚀🚀🚀🚀🚀🚀", 4))
    }
}
