package com.exponential.app.data.api

import android.util.Log
import com.exponential.app.data.auth.AuthRepository
import io.ktor.client.HttpClient
import io.ktor.client.plugins.timeout
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsBytes
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.ByteArrayContent
import io.ktor.http.isSuccess
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * Response of `POST /api/issues/{id}/files` (EXP-297). Identical shape to the
 * image route's; `width`/`height` are image-only and simply not modelled here
 * (the injected Json is `ignoreUnknownKeys`).
 */
@Serializable
data class UploadedAttachment(
    val id: String,
    val url: String,
    val filename: String,
    val contentType: String,
    val sizeBytes: Long,
)

@Serializable
private data class AttachmentIdInput(@SerialName("id") val id: String)

/**
 * Any-content-type issue attachments (EXP-297): upload to the `/files` route,
 * authenticated download of the stored bytes, and the member-level
 * `attachments.delete` mutation.
 *
 * The image pipeline keeps its own [IssueImagesApi] (frozen `/images`
 * contract); only the hand-rolled multipart body is shared — see
 * [buildImageUploadBody] for why Ktor's own encoder can't be used.
 */
@Singleton
class AttachmentsApi @Inject constructor(
    private val client: HttpClient,
    private val auth: AuthRepository,
    private val json: Json,
    private val trpc: TrpcClient,
) {
    suspend fun upload(
        accountId: String,
        issueId: String,
        bytes: ByteArray,
        filename: String,
        contentType: String,
    ): UploadedAttachment {
        val account = auth.accounts.value.firstOrNull { it.id == accountId }
        val baseUrl = account?.instanceUrl
            ?: throw TrpcException("No instance URL for account $accountId")
        val token = account.token
        val (body, boundary) = buildImageUploadBody(bytes, filename, contentType)
        val response = client.post("$baseUrl/api/issues/$issueId/files") {
            // 50 MB on a mobile uplink needs far more than the client-wide 30s
            // request budget (and more than the image route's 120s).
            timeout { requestTimeoutMillis = FILE_TRANSFER_TIMEOUT_MS }
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
            // log it for diagnostics, surface neutral copy.
            if (response.status == HttpStatusCode.PreconditionFailed) {
                Log.w("AttachmentsApi", "Storage-cap upload rejection: $text")
                throw TrpcException("Team storage is full.", response.status)
            }
            throw TrpcException(
                trpcUserMessageFromBody(text)
                    ?: "File upload failed: HTTP ${response.status.value}",
                response.status,
            )
        }
        return json.decodeFromString(UploadedAttachment.serializer(), text)
    }

    /**
     * Fetch an attachment's bytes. [relativeUrl] is the stored
     * `/api/attachments/{id}` form; an absolute URL is used as-is. The bearer
     * token rides ONLY same-instance requests — like the Coil
     * InstanceUrlInterceptor and iOS's `isSameOrigin` guard, a foreign host
     * must never see the session token.
     */
    suspend fun download(accountId: String, relativeUrl: String): ByteArray {
        val account = auth.accounts.value.firstOrNull { it.id == accountId }
        val baseUrl = account?.instanceUrl
            ?: throw TrpcException("No instance URL for account $accountId")
        val token = account.token
        val base = baseUrl.trimEnd('/')
        val url = if (relativeUrl.startsWith("/")) "$base$relativeUrl" else relativeUrl
        val sameInstance = url == base || url.startsWith("$base/")
        val response = client.get(url) {
            timeout { requestTimeoutMillis = FILE_TRANSFER_TIMEOUT_MS }
            if (token != null && sameInstance) header("Authorization", "Bearer $token")
        }
        if (!response.status.isSuccess()) {
            Log.w("AttachmentsApi", "Attachment download HTTP ${response.status.value} for $url")
            throw TrpcException(
                "The file could not be downloaded (HTTP ${response.status.value})",
                response.status,
            )
        }
        return response.bodyAsBytes()
    }

    /** `attachments.delete` — member-level; rewrites markdown references server-side. */
    suspend fun delete(accountId: String, attachmentId: String) {
        trpc.mutationUnit(
            accountId,
            path = "attachments.delete",
            input = AttachmentIdInput(attachmentId),
            inputSerializer = AttachmentIdInput.serializer(),
        )
    }
}

// 5 minutes: the 50 MB cap over a slow mobile uplink/downlink.
private const val FILE_TRANSFER_TIMEOUT_MS = 300_000L
