@file:OptIn(UnstableApi::class)

package com.exponential.app.data.media

import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DefaultHttpDataSource
import com.exponential.app.AppConstants
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.images.resolveAccountForUrl

/**
 * Where an inline player fetches its bytes from (EXP-824): the stored
 * relative `/api/attachments/{id}` resolved against the active instance, plus
 * the request headers that may ride along.
 */
class MediaRequest(
    val url: String,
    /** `Authorization` + `x-client-version`, or empty for a foreign host. */
    val headers: Map<String, String>,
)

/**
 * The byte route is member-only and Android is bearer-only, so the player's
 * HTTP data source has to carry the session token — but ONLY to the instance
 * that owns it. Mirrors the Coil [com.exponential.app.data.images.InstanceUrlInterceptor]
 * and `AttachmentsApi.download`: a relative URL resolves against the active
 * instance, the token is matched by instance-URL prefix (active account
 * first), and an absolute URL on any other host gets no headers at all.
 */
internal fun resolveMediaRequest(
    url: String,
    accounts: List<ServerAccount>,
    activeAccountId: String?,
    activeInstanceUrl: String?,
): MediaRequest {
    val activeBase = activeInstanceUrl?.trimEnd('/')
    val absolute = if (url.startsWith("/") && activeBase != null) "$activeBase$url" else url
    val token = resolveAccountForUrl(accounts, activeAccountId, absolute)?.token
    val headers = if (token != null) {
        mapOf(
            "Authorization" to "Bearer $token",
            "x-client-version" to AppConstants.CLIENT_VERSION_HEADER_VALUE,
        )
    } else {
        emptyMap()
    }
    return MediaRequest(absolute, headers)
}

fun AuthRepository.mediaRequest(url: String): MediaRequest =
    resolveMediaRequest(url, accounts.value, activeAccountId.value, instanceUrl.value)

/**
 * A plain `DefaultHttpDataSource` (HttpURLConnection; HTTP Range works
 * against the server) with the resolved headers as defaults. Deliberately
 * not the OkHttp data source: it would pull its own okhttp line next to the
 * ktor-pinned one (see libs.versions.toml).
 */
fun MediaRequest.dataSourceFactory(): DataSource.Factory =
    DefaultHttpDataSource.Factory()
        .setAllowCrossProtocolRedirects(false)
        .setDefaultRequestProperties(headers)
