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

    // EXP-824: the media link form has the same draft lifecycle as images —
    // a `draft://` link must never leak into a saved body, and the create
    // flow keys its pending uploads on both forms.

    @Test
    fun embedUrlsIncludeTheMediaLinkForm() {
        val markdown = "![shot](draft://img)\n\n[clip.mp4](draft://vid)\n\n[docs](https://x)\n\n[a.mp4](/api/attachments/a)"
        assertEquals(setOf("draft://img"), markdownImageUrls(markdown))
        assertEquals(
            setOf("draft://img", "draft://vid", "https://x", "/api/attachments/a"),
            markdownEmbedUrls(markdown),
        )
    }

    @Test
    fun draftMediaLinksAreDetectedAndStripped() {
        val markdown = "before\n\n[clip.mp4](draft://vid)\n\nafter [kept](https://x)"
        assertEquals(true, hasDraftImages(markdown))
        assertEquals("before\n\n\n\nafter [kept](https://x)", stripDraftImages(markdown))
        assertEquals(false, hasDraftImages("[clip.mp4](/api/attachments/a) ![i](/api/attachments/b)"))
    }

    @Test
    fun mediaLinkUrlsAreReplacedAndRemovedInTheirOwnForm() {
        val markdown = "![s](draft://img) [clip.mp4](draft://vid) [gone.mp4](draft://old)"
        assertEquals(
            "![s](/api/attachments/1) [clip.mp4](/api/attachments/2) [gone.mp4](draft://old)",
            replaceMarkdownImageUrls(
                markdown,
                mapOf("draft://img" to "/api/attachments/1", "draft://vid" to "/api/attachments/2"),
            ),
        )
        assertEquals(
            "![s](draft://img) [clip.mp4](draft://vid) ",
            removeMarkdownImagesByUrl(markdown, listOf("draft://old")),
        )
    }

    @Test
    fun theImageFormIsNeverMatchedAsALink() {
        // `![alt](draft://x)` must be replaced ONCE, in the image form.
        assertEquals(
            "![alt](/api/attachments/1)",
            replaceMarkdownImageUrls("![alt](draft://x)", mapOf("draft://x" to "/api/attachments/1")),
        )
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
