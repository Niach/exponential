package com.exponential.app.ui.markdown

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Byte-parity fixtures for the block markdown parser + serializer. Each canonical
 * GFM string must survive `serialize(parse(md))` unchanged — this is the contract
 * that keeps the Android editor's output byte-compatible with the web
 * (tiptap-markdown) and iOS (cmark-gfm) clients.
 */
class MarkdownRoundTripTest {

    private fun roundTrip(md: String): String =
        MarkdownSerializer.blocksToMarkdown(MarkdownParser.parse(md))

    private fun assertStable(md: String) = assertEquals(md, roundTrip(md))

    @Test fun plainParagraph() = assertStable("Hello world")

    @Test fun bold() = assertStable("This is **bold** text")

    @Test fun italic() = assertStable("This is *italic* text")

    @Test fun boldItalic() = assertStable("This is ***both*** text")

    @Test fun strikethrough() = assertStable("This is ~~struck~~ text")

    @Test fun inlineCode() = assertStable("This is `code` text")

    @Test fun link() = assertStable("A [link](https://example.com) here")

    @Test fun relativeLink() = assertStable("See [docs](/help/page) now")

    @Test fun heading1() = assertStable("# Heading 1")

    @Test fun heading2() = assertStable("## Heading 2")

    @Test fun heading3() = assertStable("### Heading 3")

    @Test fun headingThenParagraph() = assertStable("# Title\n\nSome body text")

    @Test fun bulletList() = assertStable("- one\n- two\n- three")

    @Test fun orderedList() = assertStable("1. one\n2. two\n3. three")

    @Test fun taskList() = assertStable("- [ ] todo\n- [x] done")

    @Test fun blockquote() = assertStable("> quoted text")

    @Test fun codeBlockWithLang() = assertStable("```js\nconst x = 1\n```")

    @Test fun codeBlockNoLang() = assertStable("```\nplain code\n```")

    @Test fun multiLineCodeBlock() = assertStable("```kotlin\nval a = 1\nval b = 2\n```")

    @Test fun blockImage() = assertStable("![diagram](/api/attachments/abc123)")

    @Test fun textImageText() =
        assertStable("before\n\n![alt](/api/attachments/abc)\n\nafter")

    @Test fun nestedBulletList() = assertStable("- parent\n  - child")

    @Test fun mixedDocument() = assertStable(
        "# Title\n\nA paragraph with **bold**.\n\n- item 1\n- item 2\n\n> a quote",
    )

    @Test fun multipleParagraphs() = assertStable("First paragraph.\n\nSecond paragraph.")

    @Test fun boldAtStart() = assertStable("**Bold** start")

    @Test fun multipleMarksOneLine() =
        assertStable("A **bold** and *italic* and `code` mix")

    // --- Idempotency: a second round-trip must equal the first. ---

    @Test fun idempotentMixed() {
        val once = roundTrip("# T\n\ntext **b** *i*\n\n- a\n- b\n\n> q\n\n```js\nx\n```")
        assertEquals(once, roundTrip(once))
    }

    // --- Normalization (intentionally lossy, matches iOS). ---

    @Test fun boldSuppressedInHeading() =
        assertEquals("# bold title", roundTrip("# **bold** title"))

    @Test fun blankInputProducesEmpty() =
        assertEquals("", roundTrip(""))

    // --- Regression: bare URLs stay bare (no autolink — web parity). ---

    @Test fun bareUrlStaysBare() =
        assertStable("see https://example.com here")

    // --- Regression: thematic break round-trips to canonical `---`. ---

    @Test fun thematicBreakRoundTrips() =
        assertEquals("---", roundTrip("---"))

    @Test fun thematicBreakBetweenParagraphs() =
        assertEquals("before\n\n---\n\nafter", roundTrip("before\n\n---\n\nafter"))
}
