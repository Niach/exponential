package com.exponential.app.data.auth

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.Json

private const val KEY_ACCOUNTS = "accounts"
private const val KEY_ACTIVE_ACCOUNT_ID = "active_account_id"
// One-shot re-key of URL-only account ids to per-user ids (see migratePerUserIdsLocked).
private const val KEY_PERUSER_MIGRATION = "peruser_migration_v1"

// Legacy single-account keys migrated on first launch.
private const val LEGACY_KEY_INSTANCE_URL = "instance_url"
private const val LEGACY_KEY_TOKEN = "session_token"
private const val LEGACY_KEY_USER_EMAIL = "user_email"
private const val LEGACY_KEY_USER_ID = "user_id"
private const val LEGACY_KEY_IS_ADMIN = "is_admin"

@Singleton
class AccountStore @Inject constructor(
    private val store: SecureStore,
) {
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
    private val lock = Any()

    private val _accounts = MutableStateFlow<List<ServerAccount>>(emptyList())
    val accounts: StateFlow<List<ServerAccount>> = _accounts.asStateFlow()

    private val _activeAccountId = MutableStateFlow<String?>(null)
    val activeAccountId: StateFlow<String?> = _activeAccountId.asStateFlow()

    init {
        synchronized(lock) {
            val loaded = loadAccounts()
            val activeId = store.get(KEY_ACTIVE_ACCOUNT_ID)
            val (migratedAccounts, migratedActive) = migrateLegacyIfNeeded(loaded, activeId)
            _accounts.value = migratedAccounts
            _activeAccountId.value = migratedActive
            migratePerUserIdsLocked()
            persistLocked()
        }
    }

    val activeAccount: ServerAccount?
        get() = _activeAccountId.value?.let { id -> _accounts.value.firstOrNull { it.id == id } }

    fun upsertAndActivate(instanceUrl: String): ServerAccount {
        synchronized(lock) {
            val id = ServerAccount.makeId(instanceUrl)
            val now = System.currentTimeMillis()
            val existing = _accounts.value.firstOrNull { it.id == id }
            val updated = if (existing != null) {
                _accounts.value.map { if (it.id == id) it.copy(instanceUrl = instanceUrl, lastUsedAt = now) else it }
            } else {
                _accounts.value + ServerAccount(id = id, instanceUrl = instanceUrl, lastUsedAt = now)
            }
            _accounts.value = updated
            _activeAccountId.value = id
            persistLocked()
            return updated.first { it.id == id }
        }
    }

    // Persist a resolved login. `userId` is REQUIRED (the login flow fails
    // rather than call this with an unknown user): it drives the per-user
    // account identity via [resolveAccounts], which re-keys the active pending
    // account to the per-user id (fresh DB) or merges into the existing per-user
    // account (its cached DB resumes) — never sharing a DB across users.
    fun resolveActiveAccount(
        token: String,
        email: String?,
        name: String?,
        userId: String,
        isAdmin: Boolean,
        onboardingCompletedAt: String?,
        onboardingKnown: Boolean,
    ) {
        synchronized(lock) {
            val active = _activeAccountId.value?.let { id -> _accounts.value.firstOrNull { it.id == id } }
            val instanceUrl = active?.instanceUrl ?: return
            val result = resolveAccounts(
                accounts = _accounts.value,
                instanceUrl = instanceUrl,
                userId = userId,
                token = token,
                email = email,
                name = name,
                isAdmin = isAdmin,
                onboardingCompletedAt = onboardingCompletedAt,
                onboardingKnown = onboardingKnown,
                now = System.currentTimeMillis(),
            )
            _accounts.value = result.accounts
            _activeAccountId.value = result.activeId
            persistLocked()
        }
    }

    // Flip just the onboarding flag on a given account (after onboarding.complete),
    // leaving the token/session fields intact.
    fun setOnboardingCompletedAt(id: String, value: String?) {
        synchronized(lock) {
            _accounts.value = _accounts.value.map {
                if (it.id == id) it.copy(onboardingCompletedAt = value, onboardingKnown = true) else it
            }
            persistLocked()
        }
    }

    fun clearActiveToken() {
        synchronized(lock) {
            val id = _activeAccountId.value ?: return
            _accounts.value = _accounts.value.map {
                if (it.id == id) it.copy(token = null) else it
            }
            persistLocked()
        }
    }

    fun setActive(id: String) {
        synchronized(lock) {
            if (_accounts.value.none { it.id == id }) return
            val now = System.currentTimeMillis()
            _accounts.value = _accounts.value.map {
                if (it.id == id) it.copy(lastUsedAt = now) else it
            }
            _activeAccountId.value = id
            persistLocked()
        }
    }

    fun remove(id: String) {
        synchronized(lock) {
            _accounts.value = _accounts.value.filterNot { it.id == id }
            if (_activeAccountId.value == id) {
                _activeAccountId.value = _accounts.value.maxByOrNull { it.lastUsedAt }?.id
            }
            persistLocked()
        }
    }

    private fun persistLocked() {
        store.set(KEY_ACCOUNTS, json.encodeToString(_accounts.value))
        store.set(KEY_ACTIVE_ACCOUNT_ID, _activeAccountId.value)
    }

    private fun loadAccounts(): List<ServerAccount> {
        val raw = store.get(KEY_ACCOUNTS) ?: return emptyList()
        return runCatching {
            json.decodeFromString<List<ServerAccount>>(raw)
        }.getOrElse { emptyList() }
    }

    // One-shot: re-key accounts stored under the old URL-only id to the per-user
    // id, so each user gets a distinct Room DB file. A token without a userId
    // (older build that never captured one) is cleared to force a clean re-login
    // rather than guess an identity. The legacy URL-keyed DB files are wiped
    // app-side (see the per-user DB cleanup in ExponentialApp).
    private fun migratePerUserIdsLocked() {
        if (store.get(KEY_PERUSER_MIGRATION) == "done") return
        var activeId = _activeAccountId.value
        _accounts.value = _accounts.value.map { account ->
            val uid = account.userId
            when {
                account.token != null && uid != null &&
                    account.id == ServerAccount.makeId(account.instanceUrl) -> {
                    val newId = ServerAccount.makeId(account.instanceUrl, uid)
                    if (activeId == account.id) activeId = newId
                    account.copy(id = newId)
                }
                account.token != null && uid == null -> account.copy(token = null)
                else -> account
            }
        }
        _activeAccountId.value = activeId
        store.set(KEY_PERUSER_MIGRATION, "done")
    }

    private fun migrateLegacyIfNeeded(
        loaded: List<ServerAccount>,
        activeId: String?,
    ): Pair<List<ServerAccount>, String?> {
        if (loaded.isNotEmpty()) return loaded to activeId
        val legacyUrl = store.get(LEGACY_KEY_INSTANCE_URL) ?: return emptyList<ServerAccount>() to null
        val id = ServerAccount.makeId(legacyUrl)
        val account = ServerAccount(
            id = id,
            instanceUrl = legacyUrl,
            token = store.get(LEGACY_KEY_TOKEN),
            userEmail = store.get(LEGACY_KEY_USER_EMAIL),
            userId = store.get(LEGACY_KEY_USER_ID),
            isAdmin = store.get(LEGACY_KEY_IS_ADMIN) == "true",
        )
        // Wipe legacy keys.
        store.set(LEGACY_KEY_INSTANCE_URL, null)
        store.set(LEGACY_KEY_TOKEN, null)
        store.set(LEGACY_KEY_USER_EMAIL, null)
        store.set(LEGACY_KEY_USER_ID, null)
        store.set(LEGACY_KEY_IS_ADMIN, null)
        return listOf(account) to id
    }
}
