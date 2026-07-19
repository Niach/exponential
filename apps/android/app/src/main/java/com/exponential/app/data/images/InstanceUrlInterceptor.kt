package com.exponential.app.data.images

import coil3.intercept.Interceptor
import coil3.network.HttpException
import coil3.network.NetworkHeaders
import coil3.network.httpHeaders
import coil3.request.ErrorResult
import coil3.request.ImageRequest
import coil3.request.ImageResult
import com.exponential.app.data.auth.AuthRepository

/**
 * Resolves relative attachment URLs (`/api/attachments/{id}`) against the active
 * instance URL AND attaches that instance's Bearer token. The shared Ktor
 * HttpClient's DefaultRequest only sets `Accept` (every real call attaches auth
 * per-request), so without this Coil fetches attachments anonymously and the
 * server 401s every attachment — broken images everywhere. The token is matched
 * to the account whose instance URL serves the image (multi-account safe),
 * mirroring how TrpcClient / IssueImagesApi pick the per-account token.
 *
 * On a 401 the request is retried once with a freshly-read token from the auth
 * repository: the token captured at request-build time can be stale when the
 * account just re-authenticated (sign-out/sign-in rotates the session token).
 */
class InstanceUrlInterceptor(private val auth: AuthRepository) : Interceptor {
    override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
        val request = chain.request
        val data = request.data

        // Resolve a relative attachment path to absolute against the active
        // instance. External/absolute URLs (e.g. third-party avatars) pass through.
        val activeBase = auth.instanceUrl.value?.trimEnd('/')
        val absolute = when {
            data is String && data.startsWith("/") && activeBase != null -> "$activeBase$data"
            data is String -> data
            else -> return chain.proceed()
        }

        val token = resolveToken(absolute)
        val result = chain.withRequest(buildRequest(request, absolute, token)).proceed()

        // 401 → the token we attached was missing or revoked. Re-read the
        // account's current token (it may have rotated since the request was
        // built) and retry exactly once.
        if (result.isUnauthorized()) {
            val freshToken = resolveToken(absolute)
            if (freshToken != null && freshToken != token) {
                return chain.withRequest(buildRequest(request, absolute, freshToken)).proceed()
            }
        }
        return result
    }

    // The Bearer token of the account that owns this instance, so
    // private-team attachments authenticate. Match by instance URL
    // prefix rather than assuming the active account.
    private fun resolveToken(absoluteUrl: String): String? =
        auth.accounts.value
            .firstOrNull { acct ->
                val acctBase = acct.instanceUrl.trimEnd('/')
                absoluteUrl == acctBase || absoluteUrl.startsWith("$acctBase/")
            }
            ?.token

    private fun buildRequest(base: ImageRequest, absoluteUrl: String, token: String?): ImageRequest {
        val builder = ImageRequest.Builder(base).data(absoluteUrl)
        if (token != null) {
            builder.httpHeaders(
                NetworkHeaders.Builder()
                    .add("Authorization", "Bearer $token")
                    .build(),
            )
        }
        return builder.build()
    }
}

private fun ImageResult.isUnauthorized(): Boolean {
    val throwable = (this as? ErrorResult)?.throwable
    return throwable is HttpException && throwable.response.code == 401
}
