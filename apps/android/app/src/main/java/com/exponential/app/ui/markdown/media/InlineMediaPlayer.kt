@file:OptIn(UnstableApi::class)

package com.exponential.app.ui.markdown.media

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.ui.AspectRatioFrameLayout
import androidx.media3.ui.PlayerView
import coil3.compose.AsyncImage
import com.exponential.app.data.media.dataSourceFactory
import com.exponential.app.data.media.mediaEntryPoint
import com.exponential.app.data.media.mediaRequest
import com.exponential.app.domain.formatDuration
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.AttachmentInfo
import com.exponential.app.ui.markdown.DEFAULT_VIDEO_ASPECT_RATIO
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.markdown.attachmentPosterUrl
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

// EXP-824: the inline players every read surface shares — issue descriptions,
// comment bodies and the comment attachment strips. A video block is the
// photo treatment: poster in its aspect box, centred play glyph, duration
// chip; a tap starts ExoPlayer in place, the controller's fullscreen button
// (or a tap on a comment tile) opens the same player in a fullscreen dialog.
// Audio is a compact play/pause + progress row. Players are created on the
// first play and released when the block leaves the composition.

/**
 * A video attachment block (`video/` types; the glob spelling would open a
 * nested Kotlin block comment). [info] is the synced row (poster presence,
 * probed size, duration); a missing probe reserves 16:9.
 */
@Composable
fun VideoBlockView(
    url: String,
    label: String,
    info: AttachmentInfo?,
    modifier: Modifier = Modifier,
) {
    var playing by rememberSaveable(url) { mutableStateOf(false) }
    var fullscreen by rememberSaveable(url) { mutableStateOf(false) }
    val aspect = info?.aspectRatio ?: DEFAULT_VIDEO_ASPECT_RATIO
    Box(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .aspectRatio(aspect)
            .clip(RoundedCornerShape(8.dp))
            .background(Color.Black),
    ) {
        if (!playing) {
            MediaPoster(url, hasPoster = info?.hasPoster == true, label = label, Modifier.matchParentSize())
            Box(
                Modifier
                    .matchParentSize()
                    .clickable(onClickLabel = "Play $label") { playing = true },
            )
            PlayGlyph(Modifier.align(Alignment.Center))
            DurationChip(info?.durationMs, Modifier.align(Alignment.BottomEnd).padding(8.dp))
        } else {
            val player = rememberAttachmentPlayer(url, autoPlay = true)
            InlinePlayerSurface(
                player = player,
                active = !fullscreen,
                onFullscreen = { fullscreen = true },
                modifier = Modifier.matchParentSize(),
            )
            if (fullscreen) {
                FullscreenVideoDialog(player = player, onExit = { fullscreen = false })
            }
        }
    }
}

/**
 * An audio attachment block (`audio/` types): play/pause, a seekable progress bar and the
 * position / duration readout.
 */
@Composable
fun AudioBlockView(
    url: String,
    label: String,
    info: AttachmentInfo?,
    modifier: Modifier = Modifier,
) {
    var started by remember(url) { mutableStateOf(false) }
    var isPlaying by remember(url) { mutableStateOf(false) }
    var positionMs by remember(url) { mutableLongStateOf(0L) }
    var durationMs by remember(url) { mutableStateOf(info?.durationMs) }
    val player: ExoPlayer? = if (started) rememberAttachmentPlayer(url, autoPlay = true) else null

    DisposableEffect(player) {
        if (player == null) return@DisposableEffect onDispose { }
        val listener = object : Player.Listener {
            override fun onIsPlayingChanged(playing: Boolean) {
                isPlaying = playing
                positionMs = player.currentPosition
            }

            override fun onPlaybackStateChanged(playbackState: Int) {
                if (playbackState == Player.STATE_READY) {
                    player.duration.takeIf { it > 0 }?.let { durationMs = it }
                }
                if (playbackState == Player.STATE_ENDED) positionMs = player.duration.coerceAtLeast(0)
            }
        }
        player.addListener(listener)
        onDispose { player.removeListener(listener) }
    }
    LaunchedEffect(player, isPlaying) {
        while (isActive && player != null && isPlaying) {
            positionMs = player.currentPosition
            delay(250)
        }
    }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(Color.White.copy(alpha = 0.06f))
            .padding(horizontal = 10.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(36.dp)
                .clip(CircleShape)
                .background(Color.White.copy(alpha = 0.12f))
                .clickable(onClickLabel = if (isPlaying) "Pause $label" else "Play $label") {
                    when {
                        player == null -> started = true
                        player.isPlaying -> player.pause()
                        else -> {
                            if (player.playbackState == Player.STATE_ENDED) player.seekTo(0)
                            player.play()
                        }
                    }
                },
            contentAlignment = Alignment.Center,
        ) {
            if (isPlaying) PauseGlyph(Modifier.size(14.dp)) else PlayTriangle(Modifier.size(14.dp))
        }
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(
                label,
                style = MdStyle.body,
                color = MdStyle.Text,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(6.dp))
            val total = durationMs ?: 0L
            val fraction = if (total > 0) (positionMs.toFloat() / total.toFloat()).coerceIn(0f, 1f) else 0f
            LinearProgressIndicator(
                progress = { fraction },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(4.dp)
                    .pointerInput(player, total) {
                        detectTapGestures { offset ->
                            val p = player ?: return@detectTapGestures
                            if (total <= 0 || size.width <= 0) return@detectTapGestures
                            val target = (offset.x / size.width * total).toLong().coerceIn(0L, total)
                            p.seekTo(target)
                            positionMs = target
                        }
                    },
                color = Color.White.copy(alpha = 0.85f),
                trackColor = Color.White.copy(alpha = 0.15f),
            )
        }
        Spacer(Modifier.width(10.dp))
        Text(
            if (started) "${formatDuration(positionMs)} / ${formatDuration(durationMs)}" else formatDuration(durationMs),
            style = MdStyle.body.copy(fontSize = MdStyle.bodySize * 0.8f),
            color = MdStyle.Dim,
        )
    }
}

/**
 * An ExoPlayer bound to one attachment URL through the authenticated data
 * source (bearer only to the owning instance), prepared once and released
 * when the caller leaves the composition.
 */
@Composable
fun rememberAttachmentPlayer(url: String, autoPlay: Boolean): ExoPlayer {
    val context = LocalContext.current
    val player = remember(url) {
        val request = mediaEntryPoint(context).authRepository().mediaRequest(url)
        ExoPlayer.Builder(context)
            .setMediaSourceFactory(DefaultMediaSourceFactory(request.dataSourceFactory()))
            .build()
            .apply {
                setMediaItem(MediaItem.fromUri(request.url))
                prepare()
                playWhenReady = autoPlay
            }
    }
    DisposableEffect(player) {
        onDispose { player.release() }
    }
    return player
}

@Composable
private fun InlinePlayerSurface(
    player: Player,
    active: Boolean,
    onFullscreen: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val currentOnFullscreen by rememberUpdatedState(onFullscreen)
    AndroidView(
        factory = { ctx ->
            PlayerView(ctx).apply {
                useController = true
                setShowNextButton(false)
                setShowPreviousButton(false)
                resizeMode = AspectRatioFrameLayout.RESIZE_MODE_FIT
                setShutterBackgroundColor(android.graphics.Color.BLACK)
                // The controller's own fullscreen button; the dialog below
                // takes the player over while it is open.
                setFullscreenButtonClickListener { currentOnFullscreen() }
            }
        },
        // Only one PlayerView may own the surface at a time.
        update = { view -> view.player = if (active) player else null },
        onRelease = { view -> view.player = null },
        modifier = modifier,
    )
}

/**
 * The fullscreen twin: the SAME player in a full-window dialog with the
 * controller's exit-fullscreen button and a close glyph. Orientation changes
 * re-lay the dialog out; the player keeps running.
 */
@Composable
fun FullscreenVideoDialog(player: Player, onExit: () -> Unit) {
    val currentOnExit by rememberUpdatedState(onExit)
    Dialog(
        onDismissRequest = onExit,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        Box(Modifier.fillMaxSize().background(Color.Black)) {
            AndroidView(
                factory = { ctx ->
                    PlayerView(ctx).apply {
                        useController = true
                        setShowNextButton(false)
                        setShowPreviousButton(false)
                        resizeMode = AspectRatioFrameLayout.RESIZE_MODE_FIT
                        setShutterBackgroundColor(android.graphics.Color.BLACK)
                        setFullscreenButtonClickListener { currentOnExit() }
                        setFullscreenButtonState(true)
                    }
                },
                update = { view -> view.player = player },
                onRelease = { view -> view.player = null },
                modifier = Modifier.fillMaxSize(),
            )
            IconButton(
                onClick = onExit,
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .statusBarsPadding()
                    .padding(8.dp),
            ) {
                Icon(
                    ExpIcons.uiClose,
                    contentDescription = "Exit fullscreen",
                    tint = Color.White.copy(alpha = 0.9f),
                )
            }
        }
    }
}

/**
 * A stand-alone fullscreen player for one attachment (the comment strip's
 * tap target): creates its own player, plays at once, releases on close.
 */
@Composable
fun MediaPlayerDialog(url: String, onDismiss: () -> Unit) {
    val player = rememberAttachmentPlayer(url, autoPlay = true)
    FullscreenVideoDialog(player = player, onExit = onDismiss)
}

/** The poster frame behind a not-yet-started video, or a neutral wash. */
@Composable
fun MediaPoster(url: String, hasPoster: Boolean, label: String, modifier: Modifier = Modifier) {
    if (hasPoster) {
        AsyncImage(
            model = attachmentPosterUrl(url),
            contentDescription = label,
            contentScale = ContentScale.Fit,
            modifier = modifier,
        )
    } else {
        Box(modifier.background(Color.White.copy(alpha = 0.06f)))
    }
}

/** The centred play affordance: a white triangle on a translucent disc. */
@Composable
fun PlayGlyph(modifier: Modifier = Modifier, size: Dp = 56.dp) {
    Box(
        modifier = modifier
            .size(size)
            .clip(CircleShape)
            .background(Color.Black.copy(alpha = 0.55f)),
        contentAlignment = Alignment.Center,
    ) {
        PlayTriangle(Modifier.size(size * 0.36f).padding(start = size * 0.04f))
    }
}

@Composable
private fun PlayTriangle(modifier: Modifier) {
    Canvas(modifier) {
        val w = this.size.width
        val h = this.size.height
        val path = Path().apply {
            moveTo(w * 0.1f, 0f)
            lineTo(w, h / 2f)
            lineTo(w * 0.1f, h)
            close()
        }
        drawPath(path, Color.White)
    }
}

@Composable
private fun PauseGlyph(modifier: Modifier) {
    Canvas(modifier) {
        val w = this.size.width
        val h = this.size.height
        val bar = w * 0.32f
        drawRect(Color.White, topLeft = androidx.compose.ui.geometry.Offset(w * 0.1f, 0f), size = androidx.compose.ui.geometry.Size(bar, h))
        drawRect(Color.White, topLeft = androidx.compose.ui.geometry.Offset(w * 0.58f, 0f), size = androidx.compose.ui.geometry.Size(bar, h))
    }
}

/** `0:07` / `2:34` / `1:02:03` in a translucent pill; nothing when unknown. */
@Composable
fun DurationChip(durationMs: Long?, modifier: Modifier = Modifier) {
    if (durationMs == null || durationMs <= 0) return
    Text(
        formatDuration(durationMs),
        color = Color.White,
        style = MdStyle.body.copy(fontSize = MdStyle.bodySize * 0.7f),
        modifier = modifier
            .clip(RoundedCornerShape(4.dp))
            .background(Color.Black.copy(alpha = 0.6f))
            .padding(horizontal = 6.dp, vertical = 2.dp),
    )
}
