package com.exponential.app.ui.agent

import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import androidx.core.net.toUri
import com.exponential.app.domain.ComposerAddFileChoice
import com.exponential.app.domain.MAX_STEER_IMAGES
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.icons.ExpIcons
import java.io.File
import java.util.UUID
import kotlinx.coroutines.launch

/**
 * EXP-1635 (composer-menu.json `addFile`): what the + menu's add-file row opens
 * on a phone, in EVERY composer. The launchers live here, at the HOST (a
 * composer that folds would lose a result the camera delivers later); each
 * pick lands in [readComposerPick], so the host's caps + toasts apply.
 */
@Stable
class ComposerAttachPicker(
    val choices: List<ComposerAddFileChoice>,
    private val launchChoice: (ComposerAddFileChoice) -> Unit,
) {
    /** The glass menu ([ComposerAttachMenu]) at the "+". */
    var open by mutableStateOf(false)

    fun launch(choice: ComposerAddFileChoice) = launchChoice(choice)
}

@Composable
internal fun rememberComposerAttachPicker(onPick: (ComposerPick) -> Unit): ComposerAttachPicker {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val latestOnPick by rememberUpdatedState(onPick)
    // ContentResolver reads can stream from a cloud-backed provider:
    // [readComposerPick] stays off the main thread. One pick at a time through
    // the VM, so its caps (4 images, 4 files) + toasts apply.
    fun read(uris: List<Uri>) {
        if (uris.isEmpty()) return
        scope.launch { for (uri in uris) latestOnPick(readComposerPick(context, uri)) }
    }
    val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { read(it) }
    val photoPicker = rememberLauncherForActivityResult(
        ActivityResultContracts.PickMultipleVisualMedia(MAX_STEER_IMAGES),
    ) { read(it) }
    // Saved: the camera can outlive this activity's process.
    var shot by rememberSaveable { mutableStateOf<String?>(null) }
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { saved ->
        val uri = shot?.toUri()
        shot = null
        if (saved && uri != null) read(listOf(uri))
    }
    return remember(context) {
        val hasCamera = context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)
        ComposerAttachPicker(ComposerAddFileChoice.visible(hasCamera)) { choice ->
            when (choice) {
                ComposerAddFileChoice.Camera -> {
                    val uri = newCameraShot(context)
                    shot = uri.toString()
                    camera.launch(uri)
                }
                ComposerAddFileChoice.Photos -> photoPicker.launch(
                    PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly),
                )
                ComposerAddFileChoice.File -> filePicker.launch(arrayOf("*/*"))
            }
        }
    }
}

/** A fresh FileProvider target for one shot; shots older than a day go. */
private fun newCameraShot(context: Context): Uri {
    val dir = File(context.cacheDir, CAMERA_DIR).apply { mkdirs() }
    val cutoff = System.currentTimeMillis() - CAMERA_STALE_MS
    dir.listFiles()?.forEach { if (it.lastModified() < cutoff) it.delete() }
    val file = File(dir, "${UUID.randomUUID()}.jpg")
    return FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
}

/**
 * The Camera | Photos | File glass menu. A [GlassDropdownMenu], so it needs
 * the "+" as its anchor: the host's tool slot draws it beside the button.
 */
@Composable
internal fun ComposerAttachMenu(picker: ComposerAttachPicker, onClose: () -> Unit = {}) {
    GlassDropdownMenu(
        expanded = picker.open,
        onDismissRequest = {
            picker.open = false
            onClose()
        },
        modifier = Modifier.testTag(ATTACH_MENU_TEST_ID),
    ) {
        picker.choices.forEach { choice ->
            GlassMenuItem(
                text = { Text(choice.label) },
                leadingIcon = {
                    ExpIcons.byConcept(choice.icon)?.let { Icon(it, contentDescription = null, modifier = Modifier.size(16.dp)) }
                },
                onClick = {
                    picker.open = false
                    onClose()
                    picker.launch(choice)
                },
                modifier = Modifier.testTag("$ATTACH_ROW_TEST_ID-${choice.wire}"),
            )
        }
    }
}

internal const val ATTACH_MENU_TEST_ID = "composer-attach-menu"
internal const val ATTACH_ROW_TEST_ID = "composer-attach"
private const val CAMERA_DIR = "composer-camera"
private const val CAMERA_STALE_MS = 24L * 60 * 60 * 1000

/**
 * #EXP-1841 (composer-attachments.json): THE `[Image #N]` placeholders of
 * every composer (a new chat's, a session's, an agent chat's, the
 * Assistant's, an app editor's). Each newly picked IMAGE drops its own marker
 * at the caret (`insert`), numbered on from the images already pending, so
 * the writer can say "crop [Image #2]"; a file tile carries none.
 */
@Composable
internal fun ComposerImageMarkers(
    pending: List<com.exponential.app.domain.PendingAttachment>,
    value: androidx.compose.ui.text.input.TextFieldValue,
    onValueRewrite: (androidx.compose.ui.text.input.TextFieldValue) -> Unit,
) {
    val imageCount = pending.count { it.isImage }
    var markedImages by remember { androidx.compose.runtime.mutableIntStateOf(imageCount) }
    androidx.compose.runtime.LaunchedEffect(imageCount) {
        if (imageCount > markedImages) {
            var next = value
            for (k in (markedImages + 1)..imageCount) {
                val placed = com.exponential.app.domain.ComposerAttachments.insert(next.text, next.selection.end, k)
                next = androidx.compose.ui.text.input.TextFieldValue(placed.text, androidx.compose.ui.text.TextRange(placed.caret))
            }
            onValueRewrite(next)
        }
        markedImages = imageCount
    }
}

/**
 * #EXP-1841: the strip's X in every composer (composer-attachments.json
 * `drop`): an image's markers leave the draft and every higher one slides
 * down BEFORE the list shrinks; a file's X leaves the text alone.
 */
internal fun removeComposerAttachment(
    pending: List<com.exponential.app.domain.PendingAttachment>,
    index: Int,
    value: androidx.compose.ui.text.input.TextFieldValue,
    onValueRewrite: (androidx.compose.ui.text.input.TextFieldValue) -> Unit,
    onRemove: (Int) -> Unit,
) {
    val kinds = pending.map {
        if (it.isImage) com.exponential.app.domain.ComposerAttachments.Kind.Image
        else com.exponential.app.domain.ComposerAttachments.Kind.File
    }
    // composer-attachments.json `drop`: the caret stays where it was.
    val dropped = com.exponential.app.domain.ComposerAttachments.drop(kinds, index, value.text, value.selection.end)
    if (dropped.text != value.text) {
        onValueRewrite(
            androidx.compose.ui.text.input.TextFieldValue(
                dropped.text,
                androidx.compose.ui.text.TextRange(dropped.caret),
            ),
        )
    }
    onRemove(index)
}
