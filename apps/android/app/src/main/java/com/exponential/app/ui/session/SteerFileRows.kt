package com.exponential.app.ui.session

import android.content.Context
import android.webkit.MimeTypeMap
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.AttachmentsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.domain.SteerFile
import com.exponential.app.domain.sanitizeFilename
import com.exponential.app.ui.components.FileTile
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.issue.openFile
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import java.io.File
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@EntryPoint
@InstallIn(SingletonComponent::class)
interface SteerFileEntryPoint {
    fun attachmentsApi(): AttachmentsApi
    fun authRepository(): AuthRepository
}

/**
 * Wave D ×4: a user message's FILE lines (`[name](/api/attachments/<id>)`),
 * drawn as the file tile the comment thread uses for a non-image attachment
 * (glyph + name). A tap downloads the bytes into the per-id attachment cache
 * and hands them to whatever app opens the type, like a comment's file —
 * never a broken image.
 */
@Composable
internal fun SteerFileRows(files: List<SteerFile>, modifier: Modifier = Modifier) {
    if (files.isEmpty()) return
    val context = LocalContext.current
    val toaster = LocalToaster.current
    val scope = rememberCoroutineScope()
    Row(
        modifier = modifier
            .horizontalScroll(rememberScrollState())
            .padding(top = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        files.forEach { file ->
            FileTile(
                filename = file.name,
                subtitle = null,
                modifier = Modifier
                    .testTag("steer-file-${file.id}")
                    .clickable {
                        scope.launch {
                            // A failed download (401, offline) says so
                            // instead of a silent no-op tap.
                            val local = downloadSteerFile(context, file) ?: run {
                                toaster.error("Could not download the file")
                                return@launch
                            }
                            openFile(context, local, steerFileContentType(file.name))
                        }
                    },
            )
        }
    }
}

/** The MIME type a file line's extension names, else the generic octet one. */
internal fun steerFileContentType(name: String): String {
    val ext = name.substringAfterLast('.', "").lowercase()
    if (ext.isEmpty()) return "application/octet-stream"
    return MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "application/octet-stream"
}

private suspend fun downloadSteerFile(context: Context, file: SteerFile): File? {
    val points = EntryPointAccessors.fromApplication(context.applicationContext, SteerFileEntryPoint::class.java)
    val accountId = points.authRepository().activeAccountId.value ?: return null
    val target = File(File(File(context.cacheDir, "attachments"), file.id), sanitizeFilename(file.name))
    return try {
        withContext(Dispatchers.IO) { if (target.isFile && target.length() > 0) return@withContext target else null }
            ?: run {
                val bytes = points.attachmentsApi().download(accountId, "/api/attachments/${file.id}")
                withContext(Dispatchers.IO) {
                    target.parentFile?.mkdirs()
                    target.writeBytes(bytes)
                }
                target
            }
    } catch (cancel: CancellationException) {
        throw cancel
    } catch (_: Throwable) {
        null
    }
}
