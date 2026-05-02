package com.exponential.app.data.images

import coil3.intercept.Interceptor
import coil3.request.ImageRequest
import coil3.request.ImageResult
import com.exponential.app.data.auth.AuthRepository

class InstanceUrlInterceptor(private val auth: AuthRepository) : Interceptor {
    override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
        val request = chain.request
        val data = request.data
        if (data is String && data.startsWith("/")) {
            val base = auth.instanceUrl.value?.trimEnd('/')
            if (base != null) {
                val rewritten = ImageRequest.Builder(request)
                    .data("$base$data")
                    .build()
                return chain.withRequest(rewritten).proceed()
            }
        }
        return chain.proceed()
    }
}
