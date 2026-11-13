package com.exponential.app.data.outbox

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** #FEED-237 (message-delivery.json FILES): an outbox entry's persisted files. */
class OutboxFilesTest {

    @Test
    fun `the press plans one file per pick under the entry's dir`() {
        val dir = OutboxFiles.dir(File("/data/files"), "c1")
        assertEquals(File("/data/files/outbox/c1"), dir)
        val files = OutboxFiles.plan(dir, listOf("a.png" to "image/png", "b.pdf" to "application/pdf"))
        assertEquals(
            listOf(
                OutboxFile("a.png", "image/png", "/data/files/outbox/c1/0"),
                OutboxFile("b.pdf", "application/pdf", "/data/files/outbox/c1/1"),
            ),
            files,
        )
    }

    @Test
    fun `the column round-trips and nothing pending is null`() {
        val files = listOf(OutboxFile("a.png", "image/png", "/x/0"), OutboxFile("b.pdf", "application/pdf", "/x/1"))
        assertEquals(files, OutboxFiles.decode(OutboxFiles.encode(files)))
        assertNull(OutboxFiles.encode(emptyList()))
        assertEquals(emptyList<OutboxFile>(), OutboxFiles.decode(null))
        assertEquals(emptyList<OutboxFile>(), OutboxFiles.decode("not json"))
    }

    @Test
    fun `each landed file leaves the pending list and adds its id in order`() {
        val a = OutboxFile("a.png", "image/png", "/x/0")
        val b = OutboxFile("b.pdf", "application/pdf", "/x/1")
        // One upload already landed before the press (a composer stamp).
        var uploads = OutboxFiles.Uploads(listOf("f0"), listOf(a, b))
        uploads = uploads.landed(a, "f1")
        assertEquals(listOf("f0", "f1"), uploads.attachmentIds)
        assertEquals(listOf(b), uploads.pending)
        assertEquals("f0,f1", uploads.attachmentIdsWire)
        // A restart reads the columns back and resumes with b alone.
        val resumed = OutboxFiles.Uploads(
            uploads.attachmentIdsWire!!.split(','),
            OutboxFiles.decode(uploads.pendingWire),
        )
        assertEquals(listOf(b), resumed.pending)
        val done = resumed.landed(b, "f2")
        assertEquals(listOf("f0", "f1", "f2"), done.attachmentIds)
        assertTrue(done.pending.isEmpty())
        assertNull(done.pendingWire)
    }

    @Test
    fun `planned paths hold the bytes and a gone file is unreadable`() {
        val root = Files.createTempDirectory("outbox").toFile()
        try {
            val dir = OutboxFiles.dir(root, "c1")
            val file = OutboxFiles.plan(dir, listOf("a.png" to "image/png")).single()
            File(file.path).apply { parentFile?.mkdirs() }.writeBytes(byteArrayOf(1, 2, 3))
            assertEquals(listOf<Byte>(1, 2, 3), File(file.path).readBytes().toList())
            dir.deleteRecursively()
            assertFalse(File(file.path).exists())
            assertTrue(runCatching { File(file.path).readBytes() }.isFailure)
        } finally {
            root.deleteRecursively()
        }
    }
}
