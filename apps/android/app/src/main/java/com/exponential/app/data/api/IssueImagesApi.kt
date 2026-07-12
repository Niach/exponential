package com.exponential.app.data.api

import com.exponential.app.data.auth.AuthRepository
import io.ktor.client.HttpClient
import io.ktor.client.plugins.timeout
import io.ktor.client.request.forms.MultiPartFormDataContent
import io.ktor.client.request.forms.formData
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsText
import io.ktor.http.Headers
import io.ktor.http.HttpHeaders
import io.ktor.http.isSuccess
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
        val response = client.post("$baseUrl/api/issues/$issueId/images") {
            // A multi-MB photo on a slow uplink can legitimately take longer
            // than the client-wide 30s request budget.
            timeout { requestTimeoutMillis = 120_000 }
            if (token != null) header("Authorization", "Bearer $token")
            setBody(
                MultiPartFormDataContent(
                    formData {
                        append(
                            key = "file",
                            value = bytes,
                            headers = Headers.build {
                                append(HttpHeaders.ContentType, contentType)
                                append(HttpHeaders.ContentDisposition, "filename=\"$filename\"")
                            },
                        )
                    },
                )
            )
        }
        val text = response.bodyAsText()
        if (!response.status.isSuccess()) {
            throw TrpcException("Image upload failed: HTTP ${response.status.value}: $text")
        }
        return json.decodeFromString(UploadedImage.serializer(), text)
    }
}
