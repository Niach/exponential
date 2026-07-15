package com.exponential.app.data.api

import com.exponential.app.AppConstants
import com.exponential.app.BuildConfig
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.DefaultRequest
import io.ktor.client.plugins.HttpResponseValidator
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.plugins.logging.LogLevel
import io.ktor.client.plugins.logging.Logger
import io.ktor.client.plugins.logging.Logging
import io.ktor.client.plugins.websocket.WebSockets
import io.ktor.client.request.header
import io.ktor.client.statement.bodyAsText
import io.ktor.serialization.kotlinx.json.json
import javax.inject.Singleton
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

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
    fun provideHttpClient(json: Json, updateGate: UpdateGate): HttpClient =
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
                // Client versioning + min-version gate contract (EXP-104). Every
                // request (tRPC AND Electric shape polls — they share this client)
                // carries the version so the server can 426 an under-min build.
                header("x-client-version", AppConstants.CLIENT_VERSION_HEADER_VALUE)
            }
            // A custom validator runs even with expectSuccess = false, so this is
            // the single choke point that catches the server's HTTP 426
            // ("client_upgrade_required") across every tRPC and shape response and
            // latches the app-wide update gate. Parsing is fully defensive — the
            // min/latest fields may be absent, and a body that won't decode must
            // never mask the 426 signal.
            HttpResponseValidator {
                validateResponse { response ->
                    if (response.status.value == 426) {
                        val info = runCatching {
                            val obj = json.parseToJsonElement(response.bodyAsText()).jsonObject
                            UpdateGate.UpgradeInfo(
                                min = obj["min"]?.jsonPrimitive?.contentOrNull,
                                latest = obj["latest"]?.jsonPrimitive?.contentOrNull,
                            )
                        }.getOrDefault(UpdateGate.UpgradeInfo(min = null, latest = null))
                        updateGate.trigger(info)
                    }
                }
            }
        }
}
