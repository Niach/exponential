package com.exponential.app.data.push

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.RemoteInput
import com.exponential.app.ExponentialApp
import com.exponential.app.R
import com.exponential.app.data.api.AnswerInput
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.electric.MessageSnapshot
import com.exponential.app.data.sealed.SealedKeys
import com.exponential.app.data.sealed.TreeKeyStore
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/**
 * #EXP-1641 (notification-actions.json WHAT THEY DO): a notification action,
 * run in the app's process without opening it. Every mutation goes through
 * THE app's answer / send code ([SessionsApi.answer] / [SessionsApi.send]),
 * which re-seals a sealed card and seals into an encrypted chat with the tree
 * key exactly like the session page. A `sealed=1` push never leaves in
 * clear ([NotificationActions.route]): a reply is sealed for the push's
 * `rootId` without the session row, a card not synced here is read first
 * ([MessageSnapshot]) so the answer path can re-seal it. Success removes the
 * notification; a failure replaces it with [NotificationActions.FAILED],
 * opening the session.
 */
class NotificationActionReceiver : BroadcastReceiver() {

    @EntryPoint
    @InstallIn(SingletonComponent::class)
    interface Deps {
        fun sessionsApi(): SessionsApi
        fun databaseHolder(): DatabaseHolder
        fun treeKeyStore(): TreeKeyStore
        fun messageSnapshot(): MessageSnapshot
    }

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION) return
        val actionId = intent.getStringExtra(EXTRA_ACTION) ?: return
        val accountId = intent.getStringExtra(EXTRA_ACCOUNT) ?: return
        val sessionId = intent.getStringExtra(EXTRA_SESSION) ?: return
        val notificationId = intent.getIntExtra(EXTRA_NOTIFICATION, 0)
        // #EXP-1815: a push's notification is tagged by its thread.
        val notificationTag = intent.getStringExtra(EXTRA_TAG)
        val text = RemoteInput.getResultsFromIntent(intent)?.getCharSequence(REMOTE_INPUT_KEY)?.toString()?.trim()
        val app = context.applicationContext
        val deps = EntryPointAccessors.fromApplication(app, Deps::class.java)
        val pending = goAsync()
        scope.launch {
            val ok = try {
                withTimeoutOrNull(TIMEOUT_MS) { perform(deps, intent, actionId, accountId, sessionId, text) } ?: false
            } catch (err: CancellationException) {
                throw err
            } catch (err: Throwable) {
                // Offline, CONFLICT (answered elsewhere), a locked sealed card.
                Log.w(TAG, "notification action failed: ${err.javaClass.simpleName}")
                false
            }
            try {
                if (ok) {
                    NotificationManagerCompat.from(app).cancel(notificationTag, notificationId)
                } else {
                    showFailed(app, notificationTag, notificationId, sessionId, intent.getStringExtra(EXTRA_USER))
                }
            } finally {
                pending.finish()
            }
        }
    }

    /** True = the server took it; false = refused here (nothing was sent). */
    private suspend fun perform(
        deps: Deps,
        intent: Intent,
        actionId: String,
        accountId: String,
        sessionId: String,
        text: String?,
    ): Boolean {
        val sealed = intent.getBooleanExtra(EXTRA_SEALED, false)
        // A cold process: this account's cached tree keys, as the UI's cold start.
        SealedKeys.preload(deps.treeKeyStore().vault, listOf(accountId))
        val db = deps.databaseHolder().database(accountId)
        val api = deps.sessionsApi()
        val messageId = intent.getStringExtra(EXTRA_MESSAGE)
        val cardLocal = messageId != null && db.messageDao().getById(messageId)?.card != null
        val route = NotificationActions.route(actionId, sealed, intent.getStringExtra(EXTRA_ROOT), cardLocal)
        when (route) {
            NotificationActions.Route.Refuse -> return false
            NotificationActions.Route.Send, is NotificationActions.Route.SendSealed -> {
                if (text.isNullOrEmpty()) return false
                val input = SessionsSendInput.message(sessionId, text)
                if (route is NotificationActions.Route.SendSealed) {
                    api.sendSealed(accountId, input, route.rootId)
                } else {
                    api.send(accountId, input)
                }
                return true
            }
            // The card off the shape on demand; still none = nothing to re-seal.
            NotificationActions.Route.FetchThenAnswer ->
                if (messageId == null || deps.messageSnapshot().fetch(accountId, messageId)?.card == null) return false
            NotificationActions.Route.Answer -> Unit
        }
        messageId ?: return false
        val input = when {
            actionId == NotificationActions.APPROVE -> AnswerInput(messageId = messageId, decision = DECISION_APPROVED)
            actionId == NotificationActions.REQUEST_CHANGES -> {
                if (text.isNullOrEmpty()) return false
                AnswerInput(messageId = messageId, decision = DECISION_REJECTED, feedback = text)
            }
            actionId == NotificationActions.ANSWER -> {
                if (text.isNullOrEmpty()) return false
                AnswerInput(messageId = messageId, answer = text)
            }
            actionId.startsWith(NotificationActions.OPTION_PREFIX) -> {
                val label = intent.getStringExtra(EXTRA_LABEL) ?: return false
                AnswerInput(messageId = messageId, answer = label, answers = AnswerInput.questionAnswers(listOf(label)))
            }
            else -> return false
        }
        api.answer(accountId, input)
        return true
    }

    private fun showFailed(context: Context, notificationTag: String?, notificationId: Int, sessionId: String, userId: String?) {
        val open = PendingIntent.getActivity(
            context,
            notificationId,
            openIntent(context, sessionId, userId),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val failed = context.getString(R.string.notification_action_failed)
        val notification = NotificationCompat.Builder(context, ExponentialApp.ISSUE_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_notification)
            .setContentTitle(failed)
            .setAutoCancel(true)
            .setContentIntent(open)
            .build()
        try {
            NotificationManagerCompat.from(context).notify(notificationTag, notificationId, notification)
        } catch (err: SecurityException) {
            Log.w(TAG, "notify denied: ${err.message}")
        }
    }

    companion object {
        private const val TAG = "NotificationAction"
        private const val ACTION = "com.exponential.app.action.NOTIFICATION_ACTION"
        private const val EXTRA_ACTION = "actionId"
        private const val EXTRA_ACCOUNT = "accountId"
        private const val EXTRA_USER = "userId"
        private const val EXTRA_SESSION = "sessionId"
        private const val EXTRA_MESSAGE = "messageId"
        private const val EXTRA_ROOT = "rootId"
        private const val EXTRA_LABEL = "label"
        private const val EXTRA_SEALED = "sealed"
        private const val EXTRA_NOTIFICATION = "notificationId"
        private const val EXTRA_TAG = "notificationTag"
        private const val REMOTE_INPUT_KEY = "text"
        private const val DECISION_APPROVED = "approved"
        private const val DECISION_REJECTED = "rejected"
        // A notification action's broadcast must finish well inside the
        // receiver's ANR window; a slower call counts as failed.
        private const val TIMEOUT_MS = 9_000L

        private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

        /** The session a tap opens (inbox-route.json), under the push's user. */
        fun openIntent(context: Context, sessionId: String, userId: String?): Intent =
            Intent(Intent.ACTION_VIEW, Uri.parse(PushDeepLinks.uri(PushDeepLinks.Target.Session(sessionId), userId))).apply {
                setPackage(context.packageName)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            }

        /**
         * The OS-drawn actions of one push ([NotificationActions.notificationActions]):
         * buttons, a [RemoteInput] for a text one, `open` = [contentIntent]
         * (the tap's). [accountId] = the account the push's `userId` picks;
         * null = none here, no actions.
         */
        fun build(
            context: Context,
            actions: List<NotificationActions.Action>,
            data: Map<String, String>,
            accountId: String?,
            notificationId: Int,
            contentIntent: PendingIntent,
            /** #EXP-1815: the notification's tag (its thread). */
            notificationTag: String? = null,
        ): List<NotificationCompat.Action> {
            val sessionId = data["sessionId"]?.takeIf { it.isNotEmpty() }
            if (accountId == null || sessionId == null) return emptyList()
            return actions.mapIndexed { index, action ->
                if (action.id == NotificationActions.OPEN) {
                    return@mapIndexed NotificationCompat.Action.Builder(0, context.getString(R.string.notification_action_open), contentIntent).build()
                }
                val intent = Intent(context, NotificationActionReceiver::class.java).apply {
                    setAction(ACTION)
                    // Unique per notification + action, so PendingIntents never merge.
                    setData(Uri.parse("exponential-action://$notificationId/${Uri.encode(action.id)}"))
                    putExtra(EXTRA_ACTION, action.id)
                    putExtra(EXTRA_ACCOUNT, accountId)
                    putExtra(EXTRA_USER, data["userId"])
                    putExtra(EXTRA_SESSION, sessionId)
                    putExtra(EXTRA_MESSAGE, data[NotificationActions.KEY_MESSAGE_ID])
                    putExtra(EXTRA_ROOT, data[QuestionPush.ROOT_ID_KEY])
                    putExtra(EXTRA_SEALED, data[NotificationActions.KEY_SEALED] == "1")
                    putExtra(EXTRA_NOTIFICATION, notificationId)
                    putExtra(EXTRA_TAG, notificationTag)
                    if (action.id.startsWith(NotificationActions.OPTION_PREFIX)) putExtra(EXTRA_LABEL, action.title)
                }
                val text = action.kind == NotificationActions.Kind.Text
                // A RemoteInput fills the intent in: mutable; a button: immutable.
                val mutability = if (text && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    PendingIntent.FLAG_MUTABLE
                } else if (text) {
                    0
                } else {
                    PendingIntent.FLAG_IMMUTABLE
                }
                val pending = PendingIntent.getBroadcast(
                    context,
                    notificationId * 31 + index,
                    intent,
                    PendingIntent.FLAG_UPDATE_CURRENT or mutability,
                )
                val builder = NotificationCompat.Action.Builder(0, label(context, action), pending)
                    .setAuthenticationRequired(true)
                if (text) {
                    builder.addRemoteInput(
                        RemoteInput.Builder(REMOTE_INPUT_KEY).setLabel(placeholder(context, action.id)).build(),
                    )
                    builder.setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_REPLY)
                    builder.setShowsUserInterface(false)
                    builder.setAllowGeneratedReplies(false)
                }
                builder.build()
            }
        }

        /** The fixture's copy from resources; an option button wears its label. */
        private fun label(context: Context, action: NotificationActions.Action): String = when (action.id) {
            NotificationActions.APPROVE -> context.getString(R.string.notification_action_approve)
            NotificationActions.REQUEST_CHANGES -> context.getString(R.string.notification_action_request_changes)
            NotificationActions.ANSWER -> context.getString(R.string.notification_action_answer)
            NotificationActions.REPLY -> context.getString(R.string.notification_action_reply)
            else -> action.title
        }

        private fun placeholder(context: Context, id: String): String = when (id) {
            NotificationActions.REQUEST_CHANGES -> context.getString(R.string.notification_action_request_changes_placeholder)
            NotificationActions.ANSWER -> context.getString(R.string.notification_action_answer_placeholder)
            else -> context.getString(R.string.notification_action_reply_placeholder)
        }
    }
}
