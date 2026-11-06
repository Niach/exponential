// #VAPP-265: fakes behind the platform seams.
package at.exponential.engine.capabilities

class FakeLibrary(
    val photos: List<PhotoFile> = emptyList(),
    var access: Boolean = true,
    var picked: List<String> = emptyList(),
) : PhotoLibrary {
    var asked = 0
    var lastPickLimit = -1
    override fun ensureReadAccess(): Boolean { asked++; return access }
    override fun query(offset: Int, limit: Int) = photos.drop(offset).take(limit).map { it.ref }
    override fun read(id: String) = photos.firstOrNull { it.ref.id == id }
    override fun pick(limit: Int): List<String> { lastPickLimit = limit; return picked }
}

fun photo(n: Int, name: String? = "IMG_$n.jpg", mime: String? = "image/jpeg", taken: Long? = 1_793_527_200_000L - n * 1000L) =
    PhotoFile(PhotoRef("content://media/external/images/media/$n", name, mime, taken), "photo $n".toByteArray())

class FakePicker(var opened: List<PickedFile> = emptyList(), var saveAnswer: Boolean = true) : DocumentPicker {
    var openedWith: Pair<List<String>, Boolean>? = null
    var saved: Triple<String, String, ByteArray>? = null
    override fun open(mimeTypes: List<String>, multiple: Boolean): List<PickedFile> { openedWith = mimeTypes to multiple; return opened }
    override fun save(name: String, mimeType: String, bytes: ByteArray): Boolean { saved = Triple(name, mimeType, bytes); return saveAnswer }
    override fun mimeForExtension(extension: String): String? = if (extension == "kml") "application/vnd.google-earth.kml+xml" else null
}

class FakePoster(var permitted: Boolean = true) : NotificationPoster {
    val shown = LinkedHashMap<String, NotificationRecord>()
    val log = ArrayList<String>()
    override fun ensurePermission() = permitted
    override fun show(record: NotificationRecord) { shown[record.id] = record; log.add("show ${record.id}") }
    override fun cancel(id: String) { shown.remove(id); log.add("cancel $id") }
    override fun isActive(id: String) = id in shown
    fun dismiss(id: String) { shown.remove(id) }
}

class FakeAlarms : AlarmScheduler {
    val scheduled = LinkedHashMap<String, Long>()
    override fun schedule(id: String, atMillis: Long) { scheduled[id] = atMillis }
    override fun cancel(id: String) { scheduled.remove(id) }
}

class MemoryStore : NotificationStore {
    val records = LinkedHashMap<String, String>()
    override fun get(id: String) = records[id]?.let(NotificationRecord::fromJson)
    override fun put(record: NotificationRecord) { records[record.id] = record.toJson() }
    override fun remove(id: String) { records.remove(id) }
    override fun all() = records.values.mapNotNull(NotificationRecord::fromJson)
}
