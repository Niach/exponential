package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// Wave D ×4: "Add file or image" takes ANY file — images 4 × 10 MB, files
// 4 × 50 MB, ONE size-refusal copy, a separate file-cap copy.
class SteerAttachmentRulesTest {

    private val mb = 1024L * 1024

    @Test
    fun `the copy is the pinned contract`() {
        assertEquals("Images up to 10 MB and files up to 50 MB can be attached", STEER_ATTACHMENT_REJECTED)
        assertEquals("Up to 4 files per message", STEER_FILES_CAP)
        assertEquals("Up to 4 images per message", STEER_IMAGES_CAP)
    }

    @Test
    fun `an image over 10 MB is refused with the one copy`() {
        assertEquals(STEER_ATTACHMENT_REJECTED, steerAttachmentRefusal(0, 0, isImage = true, sizeBytes = 10 * mb + 1))
        assertNull(steerAttachmentRefusal(0, 0, isImage = true, sizeBytes = 10 * mb))
    }

    @Test
    fun `a file may be up to 50 MB`() {
        assertNull(steerAttachmentRefusal(0, 0, isImage = false, sizeBytes = 50 * mb))
        assertEquals(STEER_ATTACHMENT_REJECTED, steerAttachmentRefusal(0, 0, isImage = false, sizeBytes = 50 * mb + 1))
    }

    @Test
    fun `images and files cap separately`() {
        assertEquals(STEER_IMAGES_CAP, steerAttachmentRefusal(4, 0, isImage = true, sizeBytes = 1))
        assertNull(steerAttachmentRefusal(4, 0, isImage = false, sizeBytes = 1))
        assertEquals(STEER_FILES_CAP, steerAttachmentRefusal(0, 4, isImage = false, sizeBytes = 1))
        assertNull(steerAttachmentRefusal(0, 4, isImage = true, sizeBytes = 1))
    }

    @Test
    fun `only images are numbered`() {
        val kinds = listOf(false, true, false, true)
        assertNull(steerImageNumberAt(kinds, 0))
        assertEquals(1, steerImageNumberAt(kinds, 1))
        assertNull(steerImageNumberAt(kinds, 2))
        assertEquals(2, steerImageNumberAt(kinds, 3))
        assertNull(steerImageNumberAt(kinds, 4))
    }

    @Test
    fun `the start prompt carries file lines after the embeds`() {
        val id = "11111111-1111-4111-8111-111111111111"
        val file = "22222222-2222-4222-8222-222222222222"
        assertEquals(
            "go\n\n![image](/api/attachments/$id)\n[spec.pdf](/api/attachments/$file)",
            AgentComposerPrompt.build(" go ", listOf(id), listOf(SteerFile(file, "spec.pdf"))),
        )
        assertEquals(
            "[spec.pdf](/api/attachments/$file)",
            AgentComposerPrompt.build("", emptyList(), listOf(SteerFile(file, "spec.pdf"))),
        )
    }

    // Release train 2026-10-10, F6: a non-image file needs the chosen device
    // to localize it (`steer-files` cap); older hosts get images only, with
    // the server's own sentence as the toast.
    @Test
    fun `a file for a device without the steer-files cap is refused with the contract sentence`() {
        assertEquals(
            "Attaching files needs the device on 0.14.66 or newer; images still work",
            steerFilePickRefusal(isImage = false, deviceCaps = listOf("actions", "action-inputs")),
        )
        assertEquals(DomainContract.composerUiFilesNeedNewerDevice, steerFilePickRefusal(isImage = false, deviceCaps = null))
        assertEquals(DomainContract.composerUiFilesNeedNewerDevice, steerFilePickRefusal(isImage = false, deviceCaps = emptyList()))
    }

    @Test
    fun `an image passes any device and a file passes one with the cap`() {
        assertNull(steerFilePickRefusal(isImage = true, deviceCaps = null))
        assertNull(steerFilePickRefusal(isImage = true, deviceCaps = emptyList()))
        assertNull(steerFilePickRefusal(isImage = false, deviceCaps = listOf("actions", DEVICE_CAP_STEER_FILES)))
        assertEquals("steer-files", DEVICE_CAP_STEER_FILES)
    }
}
