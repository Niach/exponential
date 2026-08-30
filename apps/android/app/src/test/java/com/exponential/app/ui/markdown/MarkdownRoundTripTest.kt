package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.RichText
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

    // --- Regression (REV2-19): formatting INSIDE link text must survive, and a
    // partly formatted link must stay ONE link. The parser overlaps the Link
    // mark with the marks nested in its text, so the serializer has to compose
    // the delimiters inside the brackets instead of picking one. ---

    @Test fun boldInsideLink() = assertStable("A [**bold**](https://example.com) here")

    @Test fun italicInsideLink() = assertStable("A [*it*](https://example.com) here")

    @Test fun strikethroughInsideLink() = assertStable("A [~~gone~~](https://example.com) here")

    @Test fun codeInsideLink() = assertStable("A [`code`](https://example.com) here")

    @Test fun partiallyBoldLinkStaysOneLink() =
        assertStable("A [**bold** rest](https://example.com) here")

    @Test fun boldItalicInsideLink() = assertStable("A [***both***](https://example.com) here")

    @Test fun formattedLinkIsIdempotent() {
        val once = roundTrip("[**b** and `c` and *i*](https://example.com)")
        assertEquals(once, roundTrip(once))
    }

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

    // --- EXP-689: intentional blank lines. GFM folds bare blank-line runs, so
    // the contract stores each interior empty paragraph as an `&nbsp;` line
    // (web MarkdownParagraph, EXP-7). Byte-locked ×4. ---

    @Test fun blankLineBetweenParagraphs() = assertStable("First\n\n&nbsp;\n\nSecond")

    @Test fun twoBlankLinesBetweenParagraphs() =
        assertStable("First\n\n&nbsp;\n\n&nbsp;\n\nSecond")

    @Test fun blankLineParsesToAnEmptyEditorLine() {
        // No invisible U+00A0 in the editor: the marker folds to a truly empty line.
        val block = MarkdownParser.parse("First\n\n&nbsp;\n\nSecond").single() as ContentBlock.TextBlock
        assertEquals("First\n\nSecond", block.content.text)
    }

    @Test fun editorTypedBlankLineIsWrittenAsTheMarker() {
        // Two Enters in the editor = an empty line inside the text block.
        val blocks = listOf(ContentBlock.TextBlock(content = RichText.plain("First\n\nSecond")))
        assertEquals("First\n\n&nbsp;\n\nSecond", MarkdownSerializer.blocksToMarkdown(blocks))
    }

    @Test fun leadingAndTrailingBlankLinesAreDropped() {
        val blocks = listOf(ContentBlock.TextBlock(content = RichText.plain("\n\nOnly line\n")))
        assertEquals("Only line", MarkdownSerializer.blocksToMarkdown(blocks))
        assertEquals("Only line", roundTrip("&nbsp;\n\nOnly line\n\n&nbsp;"))
    }

    @Test fun literalNoBreakSpaceParagraphConvergesToTheMarker() =
        assertEquals("First\n\n&nbsp;\n\nSecond", roundTrip("First\n\n\u00A0\n\nSecond"))

    @Test fun blankLinesInsideAFenceStayCode() =
        assertStable("```\na\n\nb\n```")

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

    // --- Interchange tokens stay plain text (masterplan §5e): `#IDENTIFIER`
    // issue refs and `@email` mentions pill only at render time — the parser/
    // serializer must never touch or escape them. ---

    @Test fun issueRefStaysPlainText() =
        assertStable("duplicate of #MET-115, closing")

    @Test fun issueRefAtLineStartIsNotAHeading() =
        assertStable("#EXP-7 needs review")

    @Test fun issueRefInsideListItem() =
        assertStable("- fix #EXP-42 first\n- then #EXP-43")

    @Test fun issueRefNextToMarks() =
        assertStable("**urgent** see #MET-1 and `#MET-2`")

    @Test fun mentionStaysPlainText() =
        assertStable("ping @dev@example.com about #APP-9")

    // --- Emoji (EXP-551) are ordinary text: unicode in the markdown, so a
    // surrogate pair, a ZWJ sequence and a skin-tone modifier must all survive
    // parse+serialize byte-identically (the parser walks UTF-16 offsets). ---

    @Test fun emojiRoundTrips() = assertStable("ship it \uD83D\uDE80")

    @Test fun zwjEmojiRoundTrips() = assertStable("family \uD83D\uDC69\u200D\uD83D\uDCBB here")

    @Test fun skinTonedEmojiRoundTrips() = assertStable("nice \uD83D\uDC4D\uD83C\uDFFD work")

    @Test fun emojiNextToMarksRoundTrips() =
        assertStable("**bold \uD83C\uDF89** and `code \uD83D\uDE00` and [link \u2764\uFE0F](https://example.com)")

    @Test fun emojiInListItemsRoundTrips() =
        assertStable("- \uD83C\uDF89 done\n- \uD83D\uDC4D\uD83C\uDFFF next")

    /** A literal `:shortcode:` is plain text — nothing converts it on parse. */
    @Test fun literalShortcodeStaysPlainText() = assertStable("wrote :tada: by hand")
}
