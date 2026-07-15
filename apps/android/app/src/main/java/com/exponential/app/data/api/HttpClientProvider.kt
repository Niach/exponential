package com.exponential.app.data.api

import com.exponential.app.BuildConfig
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.DefaultRequest
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.plugins.logging.LogLevel
import io.ktor.client.plugins.logging.Logger
import io.ktor.client.plugins.logging.Logging
import io.ktor.client.plugins.websocket.WebSockets
import io.ktor.client.request.header
import io.ktor.serialization.kotlinx.json.json
import javax.inject.Singleton
import kotlinx.serialization.json.Json

@Module
@InstallIn(SingletonComponent::class)
object HttpClientModule {

    @Provides
    @Singleton
    fun provideJson(): Json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Provides
    @Singleton
    fun provideHttpClient(json: Json): HttpClient =
        HttpClient(CIO) {
            expectSuccess = false
            // Without this plugin, ktor CIO enforces its engine-level default
            // requestTimeout of 15s — BELOW the Electric live long-poll hold
            // window (~20s on prod, up to ~60s per long-poll-canary.md), so
            // every idle shape poll died with "Request timeout has expired"
            // (EXP-61: errors across all 14 shapes, sync frozen). Worse, the
            // engine enforces it by CANCELLING the request job, which can kill
            // the shape run-loop outright. Installing HttpTimeout replaces that
            // path with a plugin-level typed exception; ShapeClient raises the
            // per-request budget above the hold window (iOS/desktop parity:
            // both use 90s for shape reads, 30s for everything else).
            install(HttpTimeout) {
                requestTimeoutMillis = 30_000
                connectTimeoutMillis = 10_000
                socketTimeoutMillis = 30_000
            }
            install(ContentNegotiation) { json(json) }
            // Steer viewer sockets (relay PTY mirror, masterplan §5c) ride the
            // same client; the plugin is inert for plain HTTP calls.
            install(WebSockets)
            if (BuildConfig.DEBUG) {
                install(Logging) {
                    level = LogLevel.INFO
                    logger = object : Logger {
                        override fun log(message: String) {
                            android.util.Log.d("ktor", message)
                        }
                    }
                }
            }
            install(DefaultRequest) {
                header("Accept", "application/json")
            }
        }
}
