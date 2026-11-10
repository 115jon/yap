package dev.yap.coreproto

import android.app.Application
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Badge
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import uniffi.yap_mobile.Message
import uniffi.yap_mobile.Phase
import uniffi.yap_mobile.TreeItem
import uniffi.yap_mobile.TreeItemKind
import uniffi.yap_mobile.flushCache

/** One core per process, started with the app. */
class YapApp : Application() {
    lateinit var store: CoreStore
        private set

    override fun onCreate() {
        super.onCreate()
        store = CoreStore(this)
    }
}

private enum class Screen { Servers, Channels, Messages }

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val store = (application as YapApp).store
        lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) {
                store.core.setVisible(true)
            }

            override fun onStop(owner: LifecycleOwner) {
                store.core.setVisible(false)
                flushCache()
            }
        })
        setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                Surface(Modifier.fillMaxSize()) {
                    App(store, openUrl = { url ->
                        startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
                    })
                }
            }
        }
    }
}

@Composable
private fun App(store: CoreStore, openUrl: (String) -> Unit) {
    when (store.phase) {
        Phase.BOOT, Phase.LOADING -> Centered {
            CircularProgressIndicator()
            Spacer(Modifier.size(16.dp))
            Text(store.status.ifEmpty { "Loading" }, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Phase.SIGNED_OUT -> Centered {
            Text("Yap", fontSize = 40.sp, fontWeight = FontWeight.Bold)
            Text("Rust core prototype", color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.size(32.dp))
            Button(onClick = { store.core.startSignIn() }) { Text("Sign in") }
            if (store.status.isNotEmpty()) {
                Spacer(Modifier.size(16.dp))
                Text(store.status, color = MaterialTheme.colorScheme.error)
            }
        }
        Phase.DEVICE_CODE -> Centered {
            Text("Approve this sign-in in your browser", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.size(24.dp))
            Text(
                store.userCode,
                fontSize = 36.sp,
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.size(24.dp))
            Button(onClick = { openUrl(store.verifyUrl) }, enabled = store.verifyUrl.isNotEmpty()) {
                Text("Open browser")
            }
            TextButton(onClick = { store.core.cancelSignIn() }) { Text("Cancel") }
        }
        Phase.READY -> Ready(store)
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Ready(store: CoreStore) {
    var screen by rememberSaveable { mutableStateOf(Screen.Servers) }
    BackHandler(enabled = screen != Screen.Servers) {
        screen = if (screen == Screen.Messages) Screen.Channels else Screen.Servers
    }
    val title = when (screen) {
        Screen.Servers -> store.meName.ifEmpty { "Servers" }
        Screen.Channels -> store.serverName
        Screen.Messages -> "# " + store.channelName
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                actions = {
                    ConnectionDot(store.connected)
                    if (screen == Screen.Servers) {
                        TextButton(onClick = { store.core.signOut() }) { Text("Sign out") }
                    }
                },
            )
        },
    ) { padding ->
        Box(Modifier.padding(padding).fillMaxSize()) {
            when (screen) {
                Screen.Servers -> ServerList(store) { id ->
                    store.core.selectServer(id)
                    screen = Screen.Channels
                }
                Screen.Channels -> ChannelTree(store) { id ->
                    store.core.selectChannel(id)
                    screen = Screen.Messages
                }
                Screen.Messages -> MessageList(store)
            }
        }
    }
}

@Composable
private fun ConnectionDot(connected: Boolean) {
    Box(
        Modifier
            .padding(horizontal = 8.dp)
            .size(10.dp)
            .background(if (connected) Color(0xFF3BA55C) else Color(0xFF747F8D), CircleShape),
    )
}

@Composable
private fun ServerList(store: CoreStore, open: (String) -> Unit) {
    LazyColumn(Modifier.fillMaxSize()) {
        itemsIndexed(store.servers, key = { _, s -> s.id }) { _, s ->
            Row(
                Modifier
                    .fillMaxWidth()
                    .clickable { open(s.id) }
                    .padding(horizontal = 16.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Initials(s.initials)
                Spacer(Modifier.width(16.dp))
                Text(
                    s.name,
                    Modifier.weight(1f),
                    fontWeight = if (s.unread) FontWeight.Bold else FontWeight.Normal,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (s.mentions > 0) Badge { Text(s.mentions.toString()) }
            }
        }
    }
}

@Composable
private fun ChannelTree(store: CoreStore, open: (String) -> Unit) {
    if (store.tree.isEmpty()) {
        Centered { CircularProgressIndicator() }
        return
    }
    LazyColumn(Modifier.fillMaxSize()) {
        itemsIndexed(store.tree, key = { _, t -> "${t.kind}/${t.id}" }) { _, item ->
            TreeRow(item, open)
        }
    }
}

@Composable
private fun TreeRow(item: TreeItem, open: (String) -> Unit) {
    when (item.kind) {
        TreeItemKind.CATEGORY -> Text(
            item.name.uppercase(),
            Modifier.padding(start = 16.dp, top = 20.dp, bottom = 4.dp),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        TreeItemKind.CHANNEL -> if (!item.folded) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .clickable(enabled = !item.voice) { open(item.id) }
                    .padding(horizontal = 16.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    if (item.voice) "🔊" else "#",
                    Modifier.width(28.dp),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text(
                    item.name,
                    Modifier.weight(1f),
                    fontWeight = if (item.unread) FontWeight.Bold else FontWeight.Normal,
                    color = if (item.unread || item.voice) {
                        MaterialTheme.colorScheme.onSurface
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
                )
                if (item.mentions > 0) Badge { Text(item.mentions.toString()) }
            }
        }
        TreeItemKind.MEMBER -> if (!item.folded) {
            Text(
                item.name,
                Modifier.padding(start = 60.dp, top = 2.dp, bottom = 2.dp),
                style = MaterialTheme.typography.bodySmall,
                color = if (item.speaking) Color(0xFF3BA55C) else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun MessageList(store: CoreStore) {
    val messages = store.messages
    if (messages.isEmpty()) {
        Centered {
            if (store.pageLoading) CircularProgressIndicator() else Text("No messages yet")
        }
        return
    }
    // Newest at the bottom: reverse layout over the core's oldest-first rows.
    val state = rememberLazyListState()
    val nearTop by remember {
        derivedStateOf {
            val last = state.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0
            last >= state.layoutInfo.totalItemsCount - 5
        }
    }
    // A new message while reading the newest ones: stay at the bottom (the
    // list keeps its anchor row in view otherwise).
    val newest = messages.lastOrNull()?.id
    LaunchedEffect(newest) {
        if (state.firstVisibleItemIndex <= 1) state.animateScrollToItem(0)
    }
    LaunchedEffect(nearTop, store.hasOlder, store.olderLoading) {
        if (nearTop && store.hasOlder && !store.olderLoading) store.core.loadOlder()
    }
    Column(Modifier.fillMaxSize()) {
        LazyColumn(
            Modifier.weight(1f).fillMaxWidth(),
            state = state,
            reverseLayout = true,
            verticalArrangement = Arrangement.Top,
        ) {
            items(messages.size, key = { i -> messages[messages.size - 1 - i].id }) { i ->
                MessageRow(messages[messages.size - 1 - i])
            }
            if (store.olderLoading) {
                item(key = "older") {
                    Box(Modifier.fillMaxWidth().padding(16.dp), Alignment.Center) {
                        CircularProgressIndicator(Modifier.size(24.dp))
                    }
                }
            }
        }
        if (store.typing.isNotEmpty()) {
            Text(
                store.typing,
                Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun MessageRow(m: Message) {
    Column(Modifier.fillMaxWidth()) {
        if (m.divider) {
            Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                HorizontalDivider(Modifier.weight(1f), color = MaterialTheme.colorScheme.error)
                Text("  NEW", color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelSmall)
            }
        }
        Row(
            Modifier
                .fillMaxWidth()
                .background(if (m.mentioned) Color(0x22FAA61A) else Color.Transparent)
                .padding(start = 16.dp, end = 16.dp, top = if (m.grouped) 2.dp else 12.dp),
        ) {
            if (m.grouped) {
                Spacer(Modifier.width(56.dp))
            } else {
                Initials(m.initials)
                Spacer(Modifier.width(16.dp))
            }
            Column(Modifier.weight(1f)) {
                if (!m.grouped) {
                    Row(verticalAlignment = Alignment.Bottom) {
                        Text(
                            m.author,
                            fontWeight = FontWeight.SemiBold,
                            color = parseColor(m.authorColor) ?: MaterialTheme.colorScheme.onSurface,
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            "${m.date} ${m.time}",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                if (m.replyAuthor.isNotEmpty()) {
                    Text(
                        "↪ ${m.replyAuthor}: ${m.replyText}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                val body = m.body + if (m.edited) " (edited)" else ""
                if (body.isNotBlank()) {
                    Text(
                        body,
                        color = if (m.pending) {
                            MaterialTheme.colorScheme.onSurfaceVariant
                        } else {
                            MaterialTheme.colorScheme.onSurface
                        },
                    )
                }
                if (m.attachments.length > 2) {
                    Text(
                        "📎 attachment",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
            }
        }
    }
}

@Composable
private fun Initials(text: String) {
    Box(
        Modifier.size(40.dp).background(MaterialTheme.colorScheme.primaryContainer, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, color = MaterialTheme.colorScheme.onPrimaryContainer, fontWeight = FontWeight.Bold, fontSize = 14.sp)
    }
}

@Composable
private fun Centered(content: @Composable () -> Unit) {
    Column(
        Modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) { content() }
}

private fun parseColor(hex: String): Color? =
    if (hex.length == 7 && hex[0] == '#') {
        hex.substring(1).toLongOrNull(16)?.let { Color(0xFF000000 or it) }
    } else {
        null
    }
