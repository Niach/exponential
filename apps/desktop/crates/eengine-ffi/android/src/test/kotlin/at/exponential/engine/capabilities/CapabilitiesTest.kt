// #VAPP-265: the native capabilities' pure parts on the plain JVM (fakes behind
// every platform seam; no Robolectric, no .so). Every success is checked
// against capabilities.json's `nativeOutputSchema`.
package at.exponential.engine.capabilities

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ToolCatalogTest {
    @Test
    fun toolsJsonIsTheFixturesNativeTools() {
        val caps = listOf(
            PhotosCapability(FakeLibrary(), offMain), FilesCapability(FakePicker(), offMain),
            ShareCapability({ true }, offMain), PrintCapability({ _, _ -> true }, offMain),
            NotificationsCapability(NotificationCenter(FakePoster(), FakeAlarms(), MemoryStore()) { 0L }, FakePoster(), offMain),
        )
        assertEquals(listOf("photos", "files", "share", "print", "notifications"), caps.map { it.name() })
        val native = Contract.tools.filter { it["runs"] == "native" }
        val listed = caps.flatMap { cap -> (Json.parse(cap.toolsJson()) as List<*>).map { cap.name() to (it as Map<*, *>) } }
        assertEquals(native.map { it["name"] }.sortedBy { it as String }, listed.map { "${it.first}.${it.second["name"]}" }.sorted())
        for ((cap, tool) in listed) {
            val fixture = Contract.tool("$cap.${tool["name"]}")
            assertEquals(fixture["description"], tool["description"])
            assertEquals(fixture["inputSchema"], tool["inputSchema"])
        }
        // print.pdf runs in the Engine: not a native tool.
        assertFalse(listed.any { it.first == "print" && it.second["name"] == "pdf" })
    }

    @Test
    fun everyBridgeCaseNativeArgumentAnswersOnSchema() {
        val library = FakeLibrary(listOf(photo(1)), picked = listOf(photo(1).ref.id))
        val picker = FakePicker(listOf(PickedFile("a.csv", "text/csv", "a,b".toByteArray())))
        val store = MemoryStore()
        val poster = FakePoster()
        val caps = mapOf(
            "photos" to PhotosCapability(library, offMain),
            "files" to FilesCapability(picker, offMain),
            "share" to ShareCapability({ true }, offMain),
            "print" to PrintCapability({ _, _ -> true }, offMain),
            "notifications" to NotificationsCapability(NotificationCenter(poster, FakeAlarms(), store) { 1_793_527_200_000L }, poster, offMain),
        )
        var replayed = 0
        for (case in Contract.bridgeCases) {
            val native = case["native"] as Map<*, *>? ?: continue
            val tool = case["tool"] as String
            val (cap, name) = tool.split('.')
            val capability = caps[cap] ?: continue // another host's capability (#VAPP-273)
            val result = Result(capability.call(name, Json.write(native)))
            if (tool == "photos.get") result.assertError("notFound") else result.assertValid(tool)
            replayed++
        }
        assertTrue(replayed >= 15)
    }
}

class BaseTest {
    @Test
    fun mainThreadIsRefused() {
        val library = FakeLibrary(picked = listOf("x"))
        Result(PhotosCapability(library, onMain).call("pick", """{"limit":1}""")).assertError("failed: main thread")
        assertEquals(-1, library.lastPickLimit)
    }

    @Test
    fun unknownToolIsUnavailable() {
        Result(PhotosCapability(FakeLibrary(), offMain).call("delete", "{}")).assertError("unavailable")
        Result(PrintCapability({ _, _ -> true }, offMain).call("pdf", "{}")).assertError("unavailable")
    }

    @Test
    fun brokenArgumentsFail() {
        Result(PhotosCapability(FakeLibrary(), offMain).call("get", "{")).assertError("failed: arguments are not JSON")
        Result(PhotosCapability(FakeLibrary(), offMain).call("get", "[]")).assertError("failed: arguments are not an object")
        Result(PhotosCapability(FakeLibrary(), offMain).call("get", "{}")).assertError("failed: missing id")
    }

    @Test
    fun platformExceptionsBecomeFailures() {
        val share = ShareCapability({ throw SecurityException("nope") }, offMain)
        Result(share.call("send", """{"text":"x"}""")).assertError("failed: permission denied")
        val print = PrintCapability({ _, _ -> throw IllegalStateException("no printer\nstack") }, offMain)
        Result(print.call("print", """{"name":"a.pdf","mimeType":"application/pdf","data":"JVBERi0xLjQK"}""")).assertError("failed: no printer")
    }

    @Test
    fun base64IsStandardAndPadded() {
        assertEquals("aGk=", Bytes.encode("hi".toByteArray()))
        assertEquals("+/8=", Bytes.encode(byteArrayOf(0xfb.toByte(), 0xff.toByte())))
        assertArrayEquals("hello".toByteArray(), Bytes.decode("aGVsbG8="))
        try {
            Bytes.decode("a b")
            throw AssertionError("decoded")
        } catch (e: ToolFailure) {
            assertEquals("failed: data is not base64", e.reason)
        }
    }

    @Test
    fun jsonRoundTripsAndKeepsRawMembers() {
        val text = """{"a":1,"b":[true,null,"x\né"],"c":{"d":1.5,"e":-2}}"""
        assertEquals(text.replace("\\u00e9", "é"), Json.write(Json.parse(text)))
        val raw = Json.rawMembers("""{ "id" : "x", "tap" : {"params": {"n": 1.0, "z": "a\"b"}} }""")
        assertEquals("""{"params": {"n": 1.0, "z": "a\"b"}}""", raw["tap"])
        assertEquals("\"x\"", raw["id"])
    }

    @Test
    fun textClipsByCodePoint() {
        val s = "😀".repeat(300)
        assertEquals(255, Text.clip(s, 255).codePointCount(0, Text.clip(s, 255).length))
        assertEquals("file", Text.name("  ", "file"))
    }
}

class PhotosTest {
    private val library = FakeLibrary((1..5).map { photo(it) })
    private val photos = PhotosCapability(library, offMain)

    @Test
    fun listPagesNewestFirstWithAnOpaqueCursor() {
        val first = Result(photos.call("list", """{"limit":2}""")).assertValid("photos.list").structured
        assertEquals(listOf("IMG_1.jpg", "IMG_2.jpg"), (first["photos"] as List<*>).map { (it as Map<*, *>)["name"] })
        assertEquals("2026-11-01T09:59:59Z", ((first["photos"] as List<*>)[0] as Map<*, *>)["takenAt"])
        assertFalse((first["photos"] as List<*>).any { "data" in (it as Map<*, *>).keys })
        val cursor = first["cursor"] as String
        val second = Result(photos.call("list", """{"limit":2,"cursor":"$cursor"}""")).assertValid("photos.list").structured
        assertEquals(listOf("IMG_3.jpg", "IMG_4.jpg"), (second["photos"] as List<*>).map { (it as Map<*, *>)["name"] })
        val last = Result(photos.call("list", """{"limit":2,"cursor":"${second["cursor"]}"}""")).assertValid("photos.list").structured
        assertEquals(1, (last["photos"] as List<*>).size)
        assertNull(last["cursor"])
        assertTrue("cursor" in last.keys)
    }

    @Test
    fun listOfAnExactPageEndsWithNull() {
        val all = Result(photos.call("list", """{"limit":5}""")).assertValid("photos.list").structured
        assertNull(all["cursor"])
    }

    @Test
    fun cursorRoundTripsAndRejectsForeignTokens() {
        assertEquals(40, PhotoCursor.decode(PhotoCursor.encode(40)))
        assertEquals(0, PhotoCursor.decode(null))
        Result(photos.call("list", """{"limit":2,"cursor":"c1"}""")).assertError("failed: unknown cursor")
    }

    @Test
    fun listNeedsReadAccess() {
        library.access = false
        Result(photos.call("list", "{}")).assertError("failed: permission denied")
    }

    @Test
    fun getAnswersBytesOrNotFound() {
        val r = Result(photos.call("get", """{"id":"${photo(3).ref.id}"}""")).assertValid("photos.get").structured
        val p = r["photo"] as Map<*, *>
        assertEquals(Bytes.encode("photo 3".toByteArray()), p["data"])
        assertEquals(listOf("id", "name", "mimeType", "takenAt", "data"), p.keys.toList())
        Result(photos.call("get", """{"id":"content://media/external/images/media/99"}""")).assertError("notFound")
    }

    @Test
    fun unknownFieldsGetDefaults() {
        val lib = FakeLibrary(listOf(PhotoFile(PhotoRef("content://x/1", null, null, null), ByteArray(0)), PhotoFile(PhotoRef("content://x/2", "a.heic", "application/octet-stream", 0), ByteArray(1))))
        val list = Result(PhotosCapability(lib, offMain).call("list", "{}")).assertValid("photos.list").structured["photos"] as List<*>
        assertEquals(mapOf("id" to "content://x/1", "name" to "photo", "mimeType" to "image/jpeg", "takenAt" to null), list[0])
        assertEquals("image/heic", (list[1] as Map<*, *>)["mimeType"])
        assertNull((list[1] as Map<*, *>)["takenAt"])
    }

    @Test
    fun pickAnswersAtMostLimitWithBytesAndCancelIsEmpty() {
        library.picked = listOf(photo(2).ref.id, photo(4).ref.id, photo(5).ref.id)
        val r = Result(photos.call("pick", """{"limit":2}""")).assertValid("photos.pick").structured
        assertEquals(2, library.lastPickLimit)
        assertEquals(listOf("IMG_2.jpg", "IMG_4.jpg"), (r["photos"] as List<*>).map { (it as Map<*, *>)["name"] })
        library.picked = emptyList()
        assertEquals(mapOf("photos" to emptyList<Any>()), Result(photos.call("pick", "{}")).assertValid("photos.pick").structured)
        assertEquals(1, library.lastPickLimit)
        assertEquals(0, library.asked)
    }

    @Test
    fun takenAtPrefersDateTaken() {
        assertEquals(5000L, AndroidPhotoLibrary.takenAt(5000L, 9L))
        assertEquals(9000L, AndroidPhotoLibrary.takenAt(0L, 9L))
        assertNull(AndroidPhotoLibrary.takenAt(null, null))
        assertEquals("2026-11-01T10:00:00Z", Times.rfc3339(1_793_527_200_000L))
        assertEquals("2026-11-01T10:00:00.250Z", Times.rfc3339(1_793_527_200_250L))
    }
}

class FilesTest {
    @Test
    fun acceptBecomesMimeTypes() {
        assertEquals(listOf("*/*"), Mime.acceptToMimeTypes(emptyList()))
        assertEquals(listOf("image/*", "text/csv"), Mime.acceptToMimeTypes(listOf("image/*", ".CSV", "text/csv")))
        assertEquals(listOf("application/vnd.google-earth.kml+xml"), Mime.acceptToMimeTypes(listOf(".kml"), FakePicker()::mimeForExtension))
        assertEquals(listOf("*/*"), Mime.acceptToMimeTypes(listOf("image/png", ".weird")))
    }

    @Test
    fun openShapesFilesAndCancelIsEmpty() {
        val picker = FakePicker(listOf(PickedFile("a.csv", null, "a,b".toByteArray()), PickedFile(null, "application/octet-stream", ByteArray(0)), PickedFile("m.md", "text/markdown", ByteArray(2))))
        val files = FilesCapability(picker, offMain)
        val one = Result(files.call("open", """{"accept":[".csv"],"multiple":false}""")).assertValid("files.open").structured["files"] as List<*>
        assertEquals(listOf("text/csv") to false, picker.openedWith)
        assertEquals(listOf(mapOf("name" to "a.csv", "mimeType" to "text/csv", "data" to "YSxi")), one)
        val all = Result(files.call("open", """{"multiple":true}""")).assertValid("files.open").structured["files"] as List<*>
        assertEquals(3, all.size)
        assertEquals(mapOf("name" to "file", "mimeType" to "application/octet-stream", "data" to ""), all[1])
        picker.opened = emptyList()
        assertEquals(mapOf("files" to emptyList<Any>()), Result(files.call("open", "{}")).assertValid("files.open").structured)
    }

    @Test
    fun saveHandsTheBytesAndCancelIsFalse() {
        val picker = FakePicker()
        val files = FilesCapability(picker, offMain)
        assertEquals(mapOf("saved" to true), Result(files.call("save", """{"name":"notes.txt","mimeType":"text/plain","data":"aGVsbG8="}""")).assertValid("files.save").structured)
        assertEquals("notes.txt", picker.saved!!.first)
        assertEquals("text/plain", picker.saved!!.second)
        assertArrayEquals("hello".toByteArray(), picker.saved!!.third)
        picker.saveAnswer = false
        assertEquals(mapOf("saved" to false), Result(files.call("save", """{"name":"x","mimeType":"text/plain","data":""}""")).assertValid("files.save").structured)
        Result(files.call("save", """{"name":"x","mimeType":"text/plain","data":"a b"}""")).assertError("failed: data is not base64")
    }
}

class SharePrintTest {
    @Test
    fun shareCarriesTextLinkAndFile() {
        var seen: ShareContent? = null
        val share = ShareCapability({ seen = it; false }, offMain)
        assertEquals(mapOf("shared" to false), Result(share.call("send", """{"text":"Our album","url":"https://example.com/a"}""")).assertValid("share.send").structured)
        assertEquals("Our album\nhttps://example.com/a", seen?.text)
        assertNull(seen?.file)
        val sent = ShareCapability({ seen = it; true }, offMain)
        assertEquals(mapOf("shared" to true), Result(sent.call("send", """{"title":"Hi","file":{"name":"hello.txt","mimeType":"text/plain","data":"aGVsbG8="}}""")).assertValid("share.send").structured)
        assertEquals("Hi", seen?.title)
        assertNull(seen?.text)
        assertArrayEquals("hello".toByteArray(), seen?.file?.bytes)
        assertEquals("https://x.y", ShareCapability.sharedText(null, "https://x.y"))
        Result(sent.call("send", "{}")).assertError("failed: nothing to share")
    }

    @Test
    fun shareNamesStayOneSegment() {
        assertEquals("a_b.txt", ShareNames.safe("a/b.txt"))
        assertEquals("file", ShareNames.safe(".."))
        assertEquals("file", ShareNames.safe(".type"))
    }

    @Test
    fun printTakesPdfPngJpegOnly() {
        var job: Pair<String, NativeFile>? = null
        val print = PrintCapability({ name, file -> job = name to file; true }, offMain)
        assertEquals(mapOf("printed" to true), Result(print.call("print", """{"name":"document.pdf","mimeType":"application/pdf","data":"JVBERi0xLjQK"}""")).assertValid("print.print").structured)
        assertEquals("document.pdf", job?.first)
        assertArrayEquals("%PDF-1.4\n".toByteArray(), job?.second?.bytes)
        Result(print.call("print", """{"name":"a.gif","mimeType":"image/gif","data":"R0lGOA=="}""")).assertError("failed: unsupported file")
        val cancelled = PrintCapability({ _, _ -> false }, offMain)
        assertEquals(mapOf("printed" to false), Result(cancelled.call("print", """{"name":"s.png","mimeType":"image/png","data":"iVBORw=="}""")).assertValid("print.print").structured)
    }
}

class NotificationsTest {
    private var now = 1_793_527_200_000L // 2026-11-01T10:00:00Z
    private val poster = FakePoster()
    private val alarms = FakeAlarms()
    private val store = MemoryStore()
    private val center = NotificationCenter(poster, alarms, store) { now }
    private val notifications = NotificationsCapability(center, poster, offMain)

    private val tap = """{"type":"notificationTap","app":"acme.album","screen":"page","params":{"id":"p2","n":1.0}}"""

    private fun post(json: String) = Result(notifications.call("post", json)).assertValid("notifications.post")
    private fun cancel(id: String) = Result(notifications.call("cancel", """{"id":"$id"}""")).assertValid("notifications.cancel").structured["cancelled"]

    @Test
    fun aPastOrMissingAtPostsNow() {
        assertEquals(emptyMap<String, Any>(), post("""{"id":"acme.album/n1","title":"Hi","body":null,"at":null,"tap":null}""").structured)
        assertEquals("Hi", poster.shown["acme.album/n1"]!!.title)
        assertNull(poster.shown["acme.album/n1"]!!.body)
        post("""{"id":"acme.album/n2","title":"Late","body":"b","at":"2026-11-01T09:00:00Z","tap":null}""")
        assertTrue("acme.album/n2" in poster.shown)
        assertTrue(alarms.scheduled.isEmpty())
    }

    @Test
    fun aFutureAtSchedulesAndFiresWithItsTapVerbatim() {
        post("""{"id":"acme.album/due-1","title":"Page due","body":"The lake page is due.","at":"2026-11-07T10:00:00+01:00","tap":$tap}""")
        assertEquals(mapOf("acme.album/due-1" to 1_794_042_000_000L), alarms.scheduled)
        assertTrue(poster.shown.isEmpty())
        // Early alarm (inexact): re-armed, not shown.
        center.fire("acme.album/due-1")
        assertTrue(poster.shown.isEmpty())
        now = 1_794_042_000_000L
        center.fire("acme.album/due-1")
        val shown = poster.shown.getValue("acme.album/due-1")
        assertEquals(tap, shown.tapJson)
        // The tap round trip: the intent's extras hand the exact JSON back.
        val extras = NotificationTaps.extras(shown)
        assertEquals(tap, NotificationTaps.fromExtras { extras[it] })
        assertEquals("acme.album/due-1", extras[NotificationTaps.EXTRA_ID])
        // A second fire (a duplicate alarm) shows nothing again.
        poster.log.clear()
        center.fire("acme.album/due-1")
        assertTrue(poster.log.isEmpty())
    }

    @Test
    fun noTapMeansNoTapExtra() {
        post("""{"id":"acme.album/n1","title":"Hi","body":null,"at":null,"tap":null}""")
        assertNull(NotificationTaps.fromExtras { NotificationTaps.extras(poster.shown.getValue("acme.album/n1"))[it] })
    }

    @Test
    fun postingAnIdAgainReplacesIt() {
        post("""{"id":"a/x","title":"One","body":null,"at":null,"tap":null}""")
        post("""{"id":"a/x","title":"Later","body":null,"at":"2026-11-02T10:00:00Z","tap":null}""")
        assertTrue(poster.shown.isEmpty())
        assertEquals(setOf("a/x"), alarms.scheduled.keys)
        post("""{"id":"a/x","title":"Now","body":null,"at":null,"tap":null}""")
        assertTrue(alarms.scheduled.isEmpty())
        assertEquals("Now", poster.shown.getValue("a/x").title)
        assertEquals(1, store.records.size)
    }

    @Test
    fun cancelSaysWhetherOneExisted() {
        post("""{"id":"a/p","title":"P","body":null,"at":"2026-11-02T10:00:00Z","tap":null}""")
        post("""{"id":"a/s","title":"S","body":null,"at":null,"tap":null}""")
        post("""{"id":"a/d","title":"D","body":null,"at":null,"tap":null}""")
        poster.dismiss("a/d")
        assertEquals(true, cancel("a/p"))
        assertTrue(alarms.scheduled.isEmpty())
        assertEquals(true, cancel("a/s"))
        assertFalse("a/s" in poster.shown)
        assertEquals(false, cancel("a/d"))
        assertEquals(false, cancel("a/p"))
        assertEquals(false, cancel("a/never"))
        assertTrue(store.records.isEmpty())
    }

    @Test
    fun rescheduleAfterRebootArmsPendingAndShowsOverdue() {
        post("""{"id":"a/soon","title":"S","body":null,"at":"2026-11-01T11:00:00Z","tap":null}""")
        post("""{"id":"a/later","title":"L","body":null,"at":"2026-11-03T10:00:00Z","tap":null}""")
        alarms.scheduled.clear()
        now += 2 * 60 * 60 * 1000
        center.reschedule()
        assertTrue("a/soon" in poster.shown)
        assertEquals(setOf("a/later"), alarms.scheduled.keys)
    }

    @Test
    fun deniedPermissionFailsAndStoresNothing() {
        poster.permitted = false
        Result(notifications.call("post", """{"id":"a/x","title":"T","body":null,"at":null,"tap":null}""")).assertError("failed: permission denied")
        assertTrue(store.records.isEmpty())
        Result(notifications.call("post", """{"id":"a/x","title":"T","at":"tomorrow"}""")).assertError("failed: permission denied")
        poster.permitted = true
        Result(notifications.call("post", """{"id":"a/x","title":"T","at":"tomorrow"}""")).assertError("failed: at is not an RFC 3339 time")
    }

    @Test
    fun idsHashStably() {
        assertEquals(-2128831035, NotificationIds.of(""))
        assertEquals(-468965076, NotificationIds.of("a"))
        assertEquals(1611862729, NotificationIds.of("acme.album/due-1"))
    }

    @Test
    fun recordsRoundTrip() {
        val r = NotificationRecord("a/x", "T \"q\"", null, 5L, tap, NotificationRecord.State.PENDING)
        assertEquals(r, NotificationRecord.fromJson(r.toJson()))
        assertNull(NotificationRecord.fromJson("{"))
    }
}
