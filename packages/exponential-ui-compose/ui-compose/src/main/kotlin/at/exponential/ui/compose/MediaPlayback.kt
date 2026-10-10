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
import androidx.media3.exoplayer.ExoPlayer
import at.exponential.ui.host.MediaLimits
import at.exponential.ui.host.MediaRequest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.util.UUID

/**
 * How a policed Video / AudioPlayer request plays (React's `<video src>`,
 * Swift's `MediaLoader.Playback`): a request without headers streams
 * straight into the player; one that carries headers (`/api/attachments`
 * auth) or a `data:` url is fetched under `media.limits` into a temporary
 * file first (React's blob url).
 */
sealed interface PlaybackSource {
    data class Stream(val url: String) : PlaybackSource

    data class Fetch(val request: MediaRequest) : PlaybackSource

    companion object {
        fun of(request: MediaRequest): PlaybackSource? {
            val scheme = request.url.substringBefore(':', "").lowercase()
            if (scheme.isEmpty()) return null
            return if (scheme != "data" && request.headers.isEmpty()) Stream(request.url) else Fetch(request)
        }
    }
}

/**
 * The files fetched requests play from: [LeafImages.fetch] (the limits, a
 * redirect re-policed per hop) written under the app's cache dir, one per
 * url + headers.
 */
object MediaFiles {
    private val files = HashMap<String, File>()

    fun key(request: MediaRequest): String =
        request.url + "|" + request.headers.toSortedMap().entries.joinToString("&") { "${it.key}=${it.value}" }

    /**
     * The uri a player opens for [request]: the stream itself, or the
     * fetched file; null when the fetch failed (over a limit, HTTP error,
     * a redirect the policy denies).
     */
    suspend fun playableUri(
        context: Context,
        request: MediaRequest,
        limits: MediaLimits = MediaLimits.contract,
        police: (String) -> MediaRequest? = { null },
    ): Uri? = when (val source = PlaybackSource.of(request)) {
        is PlaybackSource.Stream -> Uri.parse(source.url)
        is PlaybackSource.Fetch -> file(context, source.request, limits, police)?.let(Uri::fromFile)
        null -> null
    }

    private suspend fun file(context: Context, request: MediaRequest, limits: MediaLimits, police: (String) -> MediaRequest?): File? {
        val key = key(request)
        synchronized(files) { files[key]?.takeIf { it.exists() } }?.let { return it }
        val file = withContext(Dispatchers.IO) {
            runCatching {
                val bytes = LeafImages.fetch(request, limits, police)
                val dir = File(context.cacheDir, "exponential-ui-media").apply { mkdirs() }
                File(dir, "${UUID.randomUUID()}.media").apply { writeBytes(bytes) }
            }.getOrNull()
        } ?: return null
        synchronized(files) { files[key] = file }
        return file
    }
}

/**
 * One Video / AudioPlayer's player (VAPP-103, Swift's `MediaPlayback`): its
 * `src` plays ONLY through the surface's policed media request
 * (`SurfaceModel.mediaRequest`: the host's resolveUrl, then the media
 * policy); a denied src never reaches a player and the leaf stays the
 * static poster / controls. Nothing loads before playback starts (a press,
 * or `autoplay`); a request with headers is fetched under `media.limits`
 * first ([MediaFiles]). Compose state: the leaves recompose on it.
 */
@OptIn(UnstableApi::class)
class MediaPlayback(private val context: Context) {
    /** The player once a policed source opened (null = not started, denied or failed). */
    var player by mutableStateOf<ExoPlayer?>(null)
        private set
    var playing by mutableStateOf(false)
        private set

    /** Milliseconds played and the item's length (null until it is known). */
    var positionMs by mutableStateOf(0L)
        private set
    var durationMs by mutableStateOf<Long?>(null)
        private set

    /** The request playback opened. */
    var opened: MediaRequest? = null
        private set

    /** The uri the player was given (tests: the stream, or the fetched file). */
    var openedUri: Uri? = null
        private set

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
        stop()
        opened = request
        val uri = MediaFiles.playableUri(context, request, limits, police) ?: return
        if (opened != request) return
        val exo = withContext(Dispatchers.Main.immediate) {
            ExoPlayer.Builder(context).build().apply {
                if (muted) volume = 0f
                addListener(listener)
                setMediaItem(MediaItem.fromUri(uri))
                prepare()
                play()
            }
        }
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

    /** Drop the player (the leaf left the screen or its src changed). */
    fun stop() {
        player?.let {
            it.removeListener(listener)
            it.release()
        }
        player = null
        opened = null
        openedUri = null
        playing = false
        positionMs = 0
        durationMs = null
    }
}
