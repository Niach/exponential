package com.exponential.app.data.api

import android.util.Log
import com.exponential.app.data.auth.AuthRepository
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
): Pair<ByteArray, String> {
    val boundary = "exp-${UUID.randomUUID()}"
    // Header values must stay single-line and quote-safe; the server re-derives
    // its own stored name via sanitizeUploadFilename anyway.
    val safeName = filename.replace(Regex("[\"\\\\\r\n]"), "_")
    val head = (
        "--$boundary\r\n" +
            "Content-Disposition: form-data; name=\"file\"; filename=\"$safeName\"\r\n" +
            "Content-Type: $contentType\r\n" +
            "\r\n"
        ).toByteArray(Charsets.UTF_8)
    val tail = "\r\n--$boundary--\r\n".toByteArray(Charsets.UTF_8)
    return (head + bytes + tail) to boundary
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
    ): UploadedImage {
        val account = auth.accounts.value.firstOrNull { it.id == accountId }
        val baseUrl = account?.instanceUrl
            ?: throw TrpcException("No instance URL for account $accountId")
        val token = account.token
        val (body, boundary) = buildImageUploadBody(bytes, filename, contentType)
        val response = client.post("$baseUrl/api/issues/$issueId/images") {
            // A multi-MB photo on a slow uplink can legitimately take longer
            // than the client-wide 30s request budget.
            timeout { requestTimeoutMillis = 120_000 }
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
