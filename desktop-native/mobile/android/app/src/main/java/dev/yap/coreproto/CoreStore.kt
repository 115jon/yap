package dev.yap.coreproto

import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.SnapshotStateList
import java.util.TimeZone
import uniffi.yap_mobile.Core
import uniffi.yap_mobile.Listener
import uniffi.yap_mobile.LocalTime
import uniffi.yap_mobile.Message
import uniffi.yap_mobile.MessageDiff
import uniffi.yap_mobile.Phase
import uniffi.yap_mobile.Server
import uniffi.yap_mobile.ServerDiff
import uniffi.yap_mobile.TokenVault
import uniffi.yap_mobile.TreeDiff
import uniffi.yap_mobile.TreeItem
import uniffi.yap_mobile.Update

/**
 * The view side of the Rust store: the same role as the desktop's QML list
 * models. The core sends batches of [Update]s on its own thread; each batch
 * is applied on the main thread, in order, so Compose sees one consistent
 * state per batch. Nothing here decides anything: every rule (ordering,
 * unread, grouping, the device sign-in, the cache) is in yap-core.
 */
class CoreStore(context: Context) : Listener {
    private val main = Handler(Looper.getMainLooper())

    var phase by mutableStateOf(Phase.BOOT)
        private set
    var userCode by mutableStateOf("")
        private set
    var verifyUrl by mutableStateOf("")
        private set
    var status by mutableStateOf("")
        private set
    var connected by mutableStateOf(false)
        private set
    var meName by mutableStateOf("")
        private set
    var serverId by mutableStateOf("")
        private set
    var serverName by mutableStateOf("")
        private set
    var channelId by mutableStateOf("")
        private set
    var channelName by mutableStateOf("")
        private set
    var pageLoading by mutableStateOf(false)
        private set
    var olderLoading by mutableStateOf(false)
        private set
    var hasOlder by mutableStateOf(false)
        private set
    var typing by mutableStateOf("")
        private set

    val servers: SnapshotStateList<Server> = mutableStateListOf()
    val tree: SnapshotStateList<TreeItem> = mutableStateListOf()
    val messages: SnapshotStateList<Message> = mutableStateListOf()

    /** Diffs that did not fit: the two sides drifted (should stay 0). */
    var drift by mutableStateOf(0)
        private set

    val core: Core = Core.start(
        context.filesDir.absolutePath,
        android.os.Build.MODEL,
        DeviceTime,
        this,
        PrefsVault(context),
    )

    override fun onUpdates(updates: List<Update>) {
        main.post { updates.forEach(::apply) }
    }

    private fun apply(update: Update) {
        when (update) {
            is Update.PhaseChanged -> phase = update.phase
            is Update.DeviceCode -> {
                userCode = update.userCode
                verifyUrl = update.url
            }
            is Update.Status -> status = update.text
            is Update.Connected -> connected = update.connected
            is Update.Me -> meName = update.name.ifEmpty { update.username }
            is Update.ActiveServer -> {
                serverId = update.id
                serverName = update.name
            }
            is Update.ActiveChannel -> {
                channelId = update.id
                channelName = update.name
            }
            is Update.Servers -> applyServers(update.diff)
            is Update.Channels -> applyTree(update.diff)
            is Update.Messages -> applyMessages(update.diff)
            is Update.PageLoading -> pageLoading = update.loading
            is Update.History -> {
                olderLoading = update.loading
                hasOlder = update.more
            }
            is Update.Typing -> typing = update.text
        }
    }

    // One function per row type: UniFFI has no generic enums.
    private fun applyServers(d: ServerDiff) = when (d) {
        is ServerDiff.Reset -> reset(servers, d.rows)
        is ServerDiff.Insert -> insert(servers, d.at, d.rows)
        is ServerDiff.Update -> update(servers, d.at, d.row)
        is ServerDiff.Remove -> remove(servers, d.at, d.count)
    }

    private fun applyTree(d: TreeDiff) = when (d) {
        is TreeDiff.Reset -> reset(tree, d.rows)
        is TreeDiff.Insert -> insert(tree, d.at, d.rows)
        is TreeDiff.Update -> update(tree, d.at, d.row)
        is TreeDiff.Remove -> remove(tree, d.at, d.count)
    }

    private fun applyMessages(d: MessageDiff) = when (d) {
        is MessageDiff.Reset -> reset(messages, d.rows)
        is MessageDiff.Insert -> insert(messages, d.at, d.rows)
        is MessageDiff.Update -> update(messages, d.at, d.row)
        is MessageDiff.Remove -> remove(messages, d.at, d.count)
    }

    private fun <T> reset(list: SnapshotStateList<T>, rows: List<T>) {
        list.clear()
        list.addAll(rows)
    }

    // Indices as Long: the core's u32 does not fit an Int at the top end.
    private fun <T> insert(list: SnapshotStateList<T>, at: UInt, rows: List<T>) {
        val i = at.toLong()
        if (i > list.size) drift++ else list.addAll(i.toInt(), rows)
    }

    private fun <T> update(list: SnapshotStateList<T>, at: UInt, row: T) {
        val i = at.toLong()
        if (i >= list.size) drift++ else list[i.toInt()] = row
    }

    private fun <T> remove(list: SnapshotStateList<T>, at: UInt, count: UInt) {
        val i = at.toLong()
        val n = count.toLong()
        if (i + n > list.size) drift++ else list.subList(i.toInt(), (i + n).toInt()).clear()
    }
}

/** The device's time zone, per instant (DST applies per message date). */
private object DeviceTime : LocalTime {
    override fun offsetSecs(utcMs: Long): Int = TimeZone.getDefault().getOffset(utcMs) / 1000
}

/**
 * The session token between launches, in app-private preferences (kept out
 * of Auto Backup by the manifest).
 * Prototype only: a release build keeps it under an Android Keystore key
 * (the desktop uses Windows Credential Manager).
 */
private class PrefsVault(context: Context) : TokenVault {
    private val prefs = context.getSharedPreferences("session", Context.MODE_PRIVATE)

    override fun load(): String? = prefs.getString("token", null)

    override fun store(token: String) {
        prefs.edit().putString("token", token).apply()
    }

    override fun clear() {
        prefs.edit().remove("token").apply()
    }
}
