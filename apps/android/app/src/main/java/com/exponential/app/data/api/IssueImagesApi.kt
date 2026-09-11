package com.exponential.app.data.api

import android.util.Log
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.domain.PreparedMedia
import io.ktor.client.HttpClient
import io.ktor.client.plugins.timeout
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.ByteArrayContent
import io.ktor.http.isSuccess
import java.io.ByteArrayOutputStream
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable
data class UploadedImage(
    val id: String,
    val url: String,
    val filename: String,
    val contentType: String,
    val sizeBytes: Long,
    // EXP-824: a media upload's probe + poster route (`/api/attachments/{id}?poster=1`).
    // All nullable — the injected Json is ignoreUnknownKeys/explicitNulls=false.
    val width: Int? = null,
    val height: Int? = null,
    val durationMs: Long? = null,
    val posterUrl: String? = null,
    val videoCodec: String? = null,
    val audioCodec: String? = null,
)

/**
 * Build the multipart/form-data body for the single image part BY HAND.
 *
 * Ktor's MultiPartFormDataContent renders the part as
 * `Content-Disposition: form-data; name=file; ...` — the name UNQUOTED
 * (RFC-legal token form). Bun's `request.formData()` on the server mis-parses
 * that: the part key comes out as `file; filename=` and `formData.get("file")`
 * returns null → HTTP 400 "Missing image file" on EVERY Android upload
 * (EXP-61). Hand-rolling the body lets us emit the browser-canonical quoted
 * form that every server parser accepts.
 */
internal fun buildImageUploadBody(
    bytes: ByteArray,
    filename: String,
    contentType: String,
    // EXP-824: a media upload's OPTIONAL `poster` part (image/jpeg, ≤ 2 MB)
    // and its plain string fields (`width`/`height`/`durationMs`), every one
    // in the same quoted-disposition form. Absent = the one-part body.
    poster: ByteArray? = null,
    fields: Map<String, String> = emptyMap(),
): Pair<ByteArray, String> {
    val boundary = "exp-${UUID.randomUUID()}"
    // Header values must stay single-line and quote-safe; the server re-derives
    // its own stored name via sanitizeUploadFilename anyway.
    val safeName = filename.replace(Regex("[\"\\\\\r\n]"), "_")
    val out = ByteArrayOutputStream(bytes.size + (poster?.size ?: 0) + 512)
    fun ascii(text: String) = out.write(text.toByteArray(Charsets.UTF_8))
    ascii(
        "--$boundary\r\n" +
            "Content-Disposition: form-data; name=\"file\"; filename=\"$safeName\"\r\n" +
            "Content-Type: $contentType\r\n" +
            "\r\n",
    )
    out.write(bytes)
    if (poster != null) {
        ascii(
            "\r\n--$boundary\r\n" +
                "Content-Disposition: form-data; name=\"poster\"; filename=\"poster.jpg\"\r\n" +
                "Content-Type: image/jpeg\r\n" +
                "\r\n",
        )
        out.write(poster)
    }
    for ((name, value) in fields) {
        ascii(
            "\r\n--$boundary\r\n" +
                "Content-Disposition: form-data; name=\"$name\"\r\n" +
                "\r\n" +
                value,
        )
    }
    ascii("\r\n--$boundary--\r\n")
    return out.toByteArray() to boundary
}

/**
 * The string fields a media upload sends beside its bytes (EXP-824): only
 * POSITIVE integers, in a fixed order, so a missing probe sends nothing rather
 * than a `0` or `null` the server would reject.
 */
internal fun mediaUploadFields(width: Int?, height: Int?, durationMs: Long?): Map<String, String> {
    val out = LinkedHashMap<String, String>()
    if (width != null && width > 0) out["width"] = width.toString()
    if (height != null && height > 0) out["height"] = height.toString()
    if (durationMs != null && durationMs > 0) out["durationMs"] = durationMs.toString()
    return out
}

@Singleton
class IssueImagesApi @Inject constructor(
    private val client: HttpClient,
    private val auth: AuthRepository,
    private val json: Json,
) {
    suspend fun upload(
        accountId: String,
        issueId: String,
        bytes: ByteArray,
        filename: String,
        contentType: String,
    ): UploadedImage =
        // EXP-613: inline images ride the general /files route; the old
        // image-only /images route is gone (PR #584).
        post(accountId, "api/issues/$issueId/files", bytes, filename, contentType)

    /**
     * EXP-824: an inline video / audio upload against the issue — the
     * normalised bytes plus the poster part and probed fields. Same route,
     * same response shape (now carrying the media columns).
     */
    suspend fun uploadMedia(
        accountId: String,
        issueId: String,
        media: PreparedMedia,
    ): UploadedImage =
        post(
            accountId,
            "api/issues/$issueId/files",
            media.bytes,
            media.filename,
            media.contentType,
            poster = media.poster,
            fields = mediaUploadFields(media.width, media.height, media.durationMs),
            // A 50 MB clip needs the file route's budget, not the image one.
            timeoutMs = MEDIA_UPLOAD_TIMEOUT_MS,
        )

    /**
     * A steer image (EXP-511) uploads against the SESSION, not against an
     * issue: a batch, action or chat run has no issue to hang an attachment
     * on, and gating the attach button on `issueId` meant most runs simply
     * could not be shown a screenshot. Same multipart shape, same response.
     */
    suspend fun uploadSessionImage(
        accountId: String,
        sessionId: String,
        bytes: ByteArray,
        filename: String,
        contentType: String,
    ): UploadedImage =
        post(accountId, "api/sessions/$sessionId/files", bytes, filename, contentType)

    /**
     * EXP-825: an image attached BEFORE a session exists — the Agent page
     * composer uploads against the TEAM (`session_attachments` row with a NULL
     * session id) and the start binds it to the run the desktop creates
     * (`codingSessions.start` `attachmentIds`). Same multipart part, same
     * response, same size/type rules as the session route; unbound rows are
     * swept after seven days.
     */
    suspend fun uploadTeamSessionImage(
        accountId: String,
        teamId: String,
        bytes: ByteArray,
        filename: String,
        contentType: String,
    ): UploadedImage =
        post(accountId, "api/teams/$teamId/session-files", bytes, filename, contentType)

    private suspend fun post(
        accountId: String,
        path: String,
        bytes: ByteArray,
        filename: String,
        contentType: String,
        poster: ByteArray? = null,
        fields: Map<String, String> = emptyMap(),
        timeoutMs: Long = 120_000,
    ): UploadedImage {
        val account = auth.accounts.value.firstOrNull { it.id == accountId }
        val baseUrl = account?.instanceUrl
            ?: throw TrpcException("No instance URL for account $accountId")
        val token = account.token
        val (body, boundary) = buildImageUploadBody(bytes, filename, contentType, poster, fields)
        val response = client.post("${baseUrl.trimEnd('/')}/$path") {
            // A multi-MB photo on a slow uplink can legitimately take longer
            // than the client-wide 30s request budget.
            timeout { requestTimeoutMillis = timeoutMs }
            if (token != null) header("Authorization", "Bearer $token")
            setBody(
                ByteArrayContent(
                    bytes = body,
                    contentType = ContentType.MultiPart.FormData.withParameter("boundary", boundary),
                )
            )
        }
        val text = response.bodyAsText()
        if (!response.status.isSuccess()) {
            // 412 = the server's storage cap. Its body carries billing copy
            // ("Upgrade to upload more.") the app must not render (EXP-216) —
            // log it for diagnostics, surface neutral copy. Other statuses keep
            // the raw body: the retry badge showing WHY is the EXP-61 contract.
            if (response.status == HttpStatusCode.PreconditionFailed) {
                Log.w("IssueImagesApi", "Storage-cap upload rejection: $text")
                throw TrpcException("Team storage is full.", response.status)
            }
            throw TrpcException("Image upload failed: HTTP ${response.status.value}: $text", response.status)
        }
        return json.decodeFromString(UploadedImage.serializer(), text)
    }
}

// 5 minutes: a 50 MB clip over a slow mobile uplink (the file route's budget).
private const val MEDIA_UPLOAD_TIMEOUT_MS = 300_000L
