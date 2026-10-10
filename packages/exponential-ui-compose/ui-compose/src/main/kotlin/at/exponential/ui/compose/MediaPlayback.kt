package at.exponential.ui.compose

import android.content.Context
import android.net.Uri
import androidx.annotation.OptIn
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import at.exponential.ui.host.MediaLimits
import at.exponential.ui.host.MediaRequest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.File
import java.io.IOException
import java.util.UUID

/**
 * How a policed Video / AudioPlayer request plays (React's `<video src>`,
 * Swift's `MediaLoader.Playback`): an http(s) request STREAMS into the
 * player with its headers (`/api/attachments` auth; catalog/host.json: the
 * media byte limits do not apply to Video / Audio); a `data:` url is
 * written to a temporary [File] first (bounded by the message itself, not
 * `maxBytes`).
 */
sealed interface PlaybackSource {
    data class Stream(val url: String, val headers: Map<String, String> = emptyMap()) : PlaybackSource

    data class File(val request: MediaRequest) : PlaybackSource

    companion object {
        fun of(request: MediaRequest): PlaybackSource? = when (LeafImages.schemeOf(request.url)) {
            "http", "https" -> Stream(request.url, request.headers)
            "data" -> File(request)
            else -> null
        }

        /**
         * [request] ready to open: a stream's redirect chain resolved and
         * policed per hop ([LeafImages.resolveStream]: the final url + that
         * hop's headers); a `data:` request as is. null = denied or failed.
         */
        suspend fun resolve(request: MediaRequest, limits: MediaLimits = MediaLimits.contract, police: (String) -> MediaRequest? = { null }): PlaybackSource? =
            when (val source = of(request)) {
                is Stream -> withContext(Dispatchers.IO) {
                    runCatching { LeafImages.resolveStream(request, limits, police) }.getOrNull()
                }?.let { Stream(it.url, it.headers) }
                else -> source
            }
    }
}

/**
 * The temporary files `data:` players open, under the cache dir's
 * `exponential-ui-media` (emptied on the first use per process): one per
 * request, refcounted by the live playbacks (the last [release] deletes
 * it) and bounded to [CAPACITY] files (an eviction deletes the oldest; its
 * player keeps the open descriptor).
 */
object MediaFiles {
    const val CAPACITY = 8

    private class Entry(val file: File, var users: Int)

    private val entries = LinkedHashMap<String, Entry>(16, 0.75f, true)
    private var swept = false

    fun key(request: MediaRequest): String =
        request.url + "|" + request.headers.toSortedMap().entries.joinToString("&") { "${it.key}=${it.value}" }

    fun dir(context: Context): File = File(context.cacheDir, "exponential-ui-media")

    /**
     * The file [request] plays from, held for the caller until [release]
     * (fetched once while any playback holds it); null when the fetch
     * failed. A redirect is re-policed per hop.
     */
    suspend fun acquire(context: Context, request: MediaRequest, police: (String) -> MediaRequest? = { null }): File? {
        val key = key(request)
        synchronized(this) {
            entries[key]?.takeIf { it.file.exists() }?.let {
                it.users += 1
                return it.file
            }
        }
        // NonCancellable: a written file is always registered (else it would leak).
        return withContext(Dispatchers.IO + NonCancellable) {
            val bytes = runCatching { LeafImages.fetch(request, MediaLimits(Long.MAX_VALUE, MediaLimits.contract.timeoutMs, 0), police) }.getOrNull()
                ?: return@withContext null
            sweep(context)
            val file = runCatching { File(dir(context).apply { mkdirs() }, "${UUID.randomUUID()}.media").apply { writeBytes(bytes) } }.getOrNull()
                ?: return@withContext null
            synchronized(this@MediaFiles) {
                entries[key]?.takeIf { it.file.exists() }?.let {
                    it.users += 1
                    file.delete()
                    return@withContext it.file
                }
                entries[key] = Entry(file, 1)
                while (entries.size > CAPACITY) {
                    val eldest = entries.entries.iterator()
                    eldest.next().value.file.delete()
                    eldest.remove()
                }
            }
            file
        }
    }

    /** Drop one hold on [file]: the last one deletes it. */
    fun release(file: File) {
        synchronized(this) {
            val it = entries.entries.iterator()
            while (it.hasNext()) {
                val e = it.next()
                if (e.value.file != file) continue
                e.value.users -= 1
                if (e.value.users <= 0) {
                    it.remove()
                    file.delete()
                }
                return
            }
        }
    }

    /** The files currently tracked (tests). */
    internal fun tracked(): List<File> = synchronized(this) { entries.values.map { it.file } }

    /** Forget every entry and sweep again on the next use (tests). */
    internal fun reset() = synchronized(this) {
        entries.clear()
        swept = false
    }

    /** The first use per process empties the directory (files a previous process left). */
    private fun sweep(context: Context) = synchronized(this) {
        if (swept) return@synchronized
        swept = true
        dir(context).listFiles()?.forEach { it.delete() }
    }
}

/**
 * A stream's data source pinned to the origin the probe resolved: after
 * every open the source's uri must keep that scheme + host + port, else it
 * closes and fails. ExoPlayer's HTTP source follows same-protocol
 * redirects itself (with the headers), so a later hop to another origin
 * fails the stream instead of being re-policed.
 */
@OptIn(UnstableApi::class)
internal class PinnedDataSource(private val inner: DataSource, private val origin: Uri) : DataSource by inner {
    override fun open(dataSpec: DataSpec): Long {
        val length = inner.open(dataSpec)
        val uri = inner.uri
        if (uri == null || !sameOrigin(uri, origin)) {
            inner.close()
            throw IOException("media moved off ${origin.scheme}://${origin.authority} to $uri")
        }
        return length
    }

    companion object {
        fun sameOrigin(a: Uri, b: Uri): Boolean =
            a.scheme.equals(b.scheme, ignoreCase = true) && a.host.equals(b.host, ignoreCase = true) && port(a) == port(b)

        private fun port(u: Uri): Int = if (u.port != -1) u.port else if (u.scheme.equals("https", ignoreCase = true)) 443 else 80
    }
}

/**
 * One Video / AudioPlayer's player (VAPP-103, Swift's `MediaPlayback`): its
 * `src` plays ONLY through the surface's policed media request
 * (`SurfaceModel.mediaRequest`: the host's resolveUrl, then the media
 * policy); a denied src never reaches a player and the leaf stays the
 * static poster / controls. Nothing loads before playback starts (a press,
 * or `autoplay`); an http(s) src streams with its headers after its
 * redirects are policed ([PlaybackSource.resolve]), a `data:` src plays
 * from a [MediaFiles] file. ONE open at a time: a second press while it
 * opens waits for it, another src cancels it. Compose state: the leaves
 * recompose on it.
 */
@OptIn(UnstableApi::class)
class MediaPlayback(private val context: Context) {
    /** The player once a policed source opened (null = not started, denied or failed). */
    var player by mutableStateOf<ExoPlayer?>(null)
        private set
    var playing by mutableStateOf(false)
        private set

    /** True while a source opens (the probe or the file fetch): the play controls wait. */
    var loading by mutableStateOf(false)
        private set

    /** Milliseconds played and the item's length (null until it is known). */
    var positionMs by mutableStateOf(0L)
        private set
    var durationMs by mutableStateOf<Long?>(null)
        private set

    /** The request playback opened. */
    var opened: MediaRequest? = null
        private set

    /** The uri the player was given (tests: the resolved stream, or the file). */
    var openedUri: Uri? = null
        private set

    /** The stream the player opened (the final hop's url + headers); null for a file. */
    var openedStream: PlaybackSource.Stream? = null
        private set

    /** Players built and released (tests). */
    internal var playersBuilt = 0
        private set
    internal var playersReleased = 0
        private set

    private var opening: Deferred<Unit>? = null
    private var generation = 0
    private var file: File? = null

    private val listener = object : Player.Listener {
        override fun onIsPlayingChanged(isPlaying: Boolean) {
            playing = isPlaying
        }

        override fun onPlaybackStateChanged(state: Int) {
            tick()
            if (state == Player.STATE_ENDED) {
                player?.pause()
                player?.seekTo(0)
                playing = false
            }
        }

        override fun onPlayerError(error: PlaybackException) {
            playing = false
        }
    }

    /**
     * Start playing [request] (muted for `autoplay`); a later request
     * replaces the player. null (a denied src) stops and loads nothing.
     */
    suspend fun play(
        request: MediaRequest?,
        muted: Boolean = false,
        limits: MediaLimits = MediaLimits.contract,
        police: (String) -> MediaRequest? = { null },
    ) {
        if (request == null) return stop()
        val current = player
        if (request == opened && current != null) {
            current.play()
            playing = true
            return
        }
        val inFlight = opening
        if (inFlight != null && request == opened) {
            awaitQuietly(inFlight)
            return
        }
        stop()
        opened = request
        loading = true
        val gen = generation
        coroutineScope {
            val task = async { open(gen, request, muted, limits, police) }
            opening = task
            try {
                awaitQuietly(task)
            } finally {
                if (opening === task) {
                    opening = null
                    loading = false
                }
            }
        }
    }

    /** Await an open; ITS cancellation (another src replaced it) is no error of the caller's. */
    private suspend fun awaitQuietly(task: Deferred<Unit>) {
        try {
            task.await()
        } catch (e: CancellationException) {
            currentCoroutineContext().ensureActive()
        }
    }

    private suspend fun open(gen: Int, request: MediaRequest, muted: Boolean, limits: MediaLimits, police: (String) -> MediaRequest?) {
        when (val source = PlaybackSource.resolve(request, limits, police)) {
            is PlaybackSource.Stream -> withContext(Dispatchers.Main.immediate) {
                if (gen != generation) return@withContext
                val uri = Uri.parse(source.url)
                val http = DefaultHttpDataSource.Factory()
                    .setDefaultRequestProperties(source.headers)
                    .setAllowCrossProtocolRedirects(false)
                    .setConnectTimeoutMs(limits.timeoutMs.toInt())
                    .setReadTimeoutMs(limits.timeoutMs.toInt())
                val pinned = DataSource.Factory { PinnedDataSource(http.createDataSource(), uri) }
                install(ExoPlayer.Builder(context).setMediaSourceFactory(DefaultMediaSourceFactory(pinned)), uri, muted)
                openedStream = source
            }
            is PlaybackSource.File -> {
                val f = MediaFiles.acquire(context, source.request, police) ?: return
                var held = false
                try {
                    withContext(Dispatchers.Main.immediate) {
                        if (gen != generation) return@withContext
                        install(ExoPlayer.Builder(context), Uri.fromFile(f), muted)
                        file = f
                        held = true
                    }
                } finally {
                    if (!held) MediaFiles.release(f)
                }
            }
            null -> {}
        }
    }

    private fun install(builder: ExoPlayer.Builder, uri: Uri, muted: Boolean) {
        val exo = builder.build().apply {
            if (muted) volume = 0f
            addListener(listener)
            setMediaItem(MediaItem.fromUri(uri))
            prepare()
            play()
        }
        playersBuilt += 1
        openedUri = uri
        player = exo
        playing = true
    }

    fun pause() {
        player?.pause()
        playing = false
    }

    /** Seek to [fraction] (0..1) of the known length ([fallbackMs] = `durationMs`). */
    fun seek(fraction: Float, fallbackMs: Long?) {
        val p = player ?: return
        val total = durationMs ?: fallbackMs ?: return
        if (total <= 0) return
        val to = (fraction.coerceIn(0f, 1f) * total).toLong()
        positionMs = to
        p.seekTo(to)
    }

    /** Read the player's position + length (the leaves poll it while playing). */
    fun tick() {
        val p = player ?: return
        positionMs = p.currentPosition.coerceAtLeast(0)
        val d = p.duration
        if (d != C.TIME_UNSET && d > 0) durationMs = d
    }

    /** Drop the player, any open in flight and the file it held (the leaf left the screen or its src changed). */
    fun stop() {
        generation += 1
        opening?.cancel()
        opening = null
        loading = false
        player?.let {
            it.removeListener(listener)
            it.release()
            playersReleased += 1
        }
        file?.let(MediaFiles::release)
        file = null
        player = null
        opened = null
        openedUri = null
        openedStream = null
        playing = false
        positionMs = 0
        durationMs = null
    }
}
