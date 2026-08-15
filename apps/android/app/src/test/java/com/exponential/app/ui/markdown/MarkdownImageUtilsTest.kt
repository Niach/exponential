package com.exponential.app.ui.markdown

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * REV-24: the create screen's pending-image map is add-only (the editor's
 * delete prunes only its own model), so the create path keeps just the
 * placeholders the submitted description still references —
 * [markdownImageUrls] is that membership check.
 */
class MarkdownImageUtilsTest {

    @Test
    fun collectsEveryReferencedImageUrl() {
        val markdown = """
            Intro text
            ![shot](draft://one)
            more text ![](draft://two) inline
            ![synced](/api/attachments/abc-123)
        """.trimIndent()
        assertEquals(
            setOf("draft://one", "draft://two", "/api/attachments/abc-123"),
            markdownImageUrls(markdown),
        )
    }

    @Test
    fun handlesEscapedAltAndTitleSuffix() {
        // Serializers escape markdown punctuation in alt (REV-6) and may
        // append a quoted title — neither hides the URL.
        assertEquals(
            setOf("draft://esc", "draft://titled"),
            markdownImageUrls("""![shot \[1\].png](draft://esc) ![t](draft://titled "cap")"""),
        )
    }

    @Test
    fun plainTextHasNoImageUrls() {
        assertEquals(emptySet<String>(), markdownImageUrls("no images here, just [a link](https://x)"))
    }

    @Test
    fun deletedDraftIsNoLongerReferenced() {
        // The user attached two images and removed one: its placeholder is
        // gone from the markdown while the screen map still holds both.
        val pending = mapOf("draft://kept" to "uriA", "draft://deleted" to "uriB")
        val markdown = "text ![kept](draft://kept) more"
        val referenced = pending.filterKeys { it in markdownImageUrls(markdown) }
        assertEquals(mapOf("draft://kept" to "uriA"), referenced)
    }
}
