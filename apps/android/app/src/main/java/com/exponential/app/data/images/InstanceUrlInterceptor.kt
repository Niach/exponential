package com.exponential.app.data.images

import coil3.intercept.Interceptor
import coil3.network.NetworkHeaders
import coil3.network.httpHeaders
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

        // Attach the Bearer token of the account that owns this instance, so
        // private-workspace attachments authenticate. Match by instance URL
        // prefix rather than assuming the active account.
        val token = auth.accounts.value
            .firstOrNull { acct ->
                val acctBase = acct.instanceUrl.trimEnd('/')
                absolute == acctBase || absolute.startsWith("$acctBase/")
            }
            ?.token

        val builder = ImageRequest.Builder(request).data(absolute)
        if (token != null) {
            builder.httpHeaders(
                NetworkHeaders.Builder()
                    .add("Authorization", "Bearer $token")
                    .build(),
            )
        }
        return chain.withRequest(builder.build()).proceed()
    }
}
