package com.exponential.app.domain

import com.exponential.app.data.media.clampToHeight
import com.exponential.app.data.media.withMp4Extension
import org.junit.Assert.assertEquals
import org.junit.Test

/** EXP-824: the pure helpers behind the inline-media block and its upload. */
class InlineMediaTest {

    @Test
    fun durationChipFormatsLikeEveryOtherClient() {
        assertEquals("0:07", formatDuration(7_000))
        assertEquals("2:34", formatDuration(154_000))
        assertEquals("1:02:03", formatDuration(3_723_000))
        assertEquals("0:00", formatDuration(0))
        assertEquals("0:00", formatDuration(null))
        assertEquals("0:00", formatDuration(-5_000))
    }

    @Test
    fun durationRoundsDownToWholeSeconds() {
        assertEquals("0:07", formatDuration(6_999))
        assertEquals("1:00:00", formatDuration(3_599_999))
        assertEquals("1:00:00", formatDuration(3_600_000))
    }

    @Test
    fun mediaLinkLabelKeepsTheLinkSyntaxIntact() {
        assertEquals("clip.mp4", mediaLinkLabel("clip.mp4"))
        assertEquals("take _1_.mov", mediaLinkLabel("take [1].mov"))
        assertEquals("a_b.mp4", mediaLinkLabel("a\nb.mp4"))
        assertEquals("with (parens).mp4", mediaLinkLabel("with (parens).mp4"))
        assertEquals("media", mediaLinkLabel(""))
        assertEquals("media", mediaLinkLabel(null))
        assertEquals("media", mediaLinkLabel("  "))
    }

    @Test
    fun clampToHeightMatchesTransformerPresentation() {
        // At or under 720 tall: untouched.
        assertEquals(1280 to 720, clampToHeight(1280, 720, 720))
        assertEquals(640 to 360, clampToHeight(640, 360, 720))
        // Over: scaled to 720 keeping the ratio, width rounded to even.
        assertEquals(1280 to 720, clampToHeight(1920, 1080, 720))
        assertEquals(2560 to 720, clampToHeight(3840, 1080, 720))
        // Portrait 1080x1920 → 405 → nearest even 406 x 720.
        assertEquals(406 to 720, clampToHeight(1080, 1920, 720))
        // Unknown or degenerate sizes pass through.
        assertEquals(null to null, clampToHeight(null, null, 720))
        assertEquals(0 to 10, clampToHeight(0, 10, 720))
    }

    @Test
    fun transcodedFilesGainTheMp4Extension() {
        assertEquals("clip.mp4", withMp4Extension("clip.mov"))
        assertEquals("clip.mp4", withMp4Extension("clip.mp4"))
        assertEquals("archive.tar.mp4", withMp4Extension("archive.tar.webm"))
        assertEquals("clip.mp4", withMp4Extension("clip"))
        assertEquals(".hidden.mp4", withMp4Extension(".hidden"))
    }

    @Test
    fun mediaClassificationIsAPrefixMatchOnTheCanonicalType() {
        assertEquals(true, isInlineVideo("video/mp4"))
        assertEquals(true, isInlineVideo("video/quicktime"))
        assertEquals(true, isInlineAudio("audio/mpeg"))
        assertEquals(true, isInlineMedia("audio/mp4"))
        assertEquals(false, isInlineMedia("image/png"))
        assertEquals(false, isInlineVideo("audio/mpeg"))
        assertEquals(false, isInlineAudio("video/mp4"))
        assertEquals(false, isInlineMedia(null))
        assertEquals(false, isInlineMedia(""))
        // Stored types are canonical; a non-canonical spelling is not media.
        assertEquals(false, isInlineVideo("VIDEO/MP4"))
    }
}
