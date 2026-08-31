package be.edwin.fabatag

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.nfc.NdefMessage
import android.nfc.NdefRecord
import android.nfc.NfcAdapter
import android.nfc.Tag
import android.nfc.tech.Ndef
import android.nfc.tech.NdefFormatable
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.IOException
import java.util.concurrent.atomic.AtomicBoolean

class MainActivity : ComponentActivity() {
    private val api = ApiClient()
    private lateinit var tokenStore: TokenStore
    private var session by mutableStateOf<AccountSession?>(null)
    private var library by mutableStateOf<CloudLibrary?>(null)
    private var loading by mutableStateOf(false)
    private var statusMessage by mutableStateOf<String?>(null)
    private var importDraft by mutableStateOf<ImportDraft?>(null)
    private var renameTarget by mutableStateOf<CloudPlaylist?>(null)
    private var deleteTarget by mutableStateOf<CloudPlaylist?>(null)
    private var pendingNfc by mutableStateOf<CloudPlaylist?>(null)
    private var nfcResult by mutableStateOf<String?>(null)
    private var pickerTarget: CloudPlaylist? = null
    private var nfcAdapter: NfcAdapter? = null
    private val nfcWriting = AtomicBoolean(false)

    private val audioPicker = registerForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isEmpty()) return@registerForActivityResult
        if (uris.size > 99) {
            statusMessage = "Une playlist est limitée à 99 pistes."
            return@registerForActivityResult
        }
        val target = pickerTarget
        val suggestedId = target?.figureId ?: nextAvailableId()
        if (suggestedId == null) {
            statusMessage = "Aucun identifiant personnalisé libre entre 2000 et 8999."
            return@registerForActivityResult
        }
        importDraft = ImportDraft(
            figureId = suggestedId,
            name = target?.name ?: "Ma playlist",
            uris = uris,
            labels = uris.map(::displayName),
            replacing = target != null,
        )
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        tokenStore = TokenStore(this)
        session = tokenStore.load()
        nfcAdapter = NfcAdapter.getDefaultAdapter(this)
        setContent {
            FabaTheme {
                FabaApp(
                    session = session,
                    library = library,
                    loading = loading,
                    statusMessage = statusMessage,
                    importDraft = importDraft,
                    renameTarget = renameTarget,
                    deleteTarget = deleteTarget,
                    pendingNfc = pendingNfc,
                    nfcResult = nfcResult,
                    onAuthenticate = ::authenticate,
                    onRefresh = ::refreshLibrary,
                    onLogout = ::logout,
                    onImport = ::pickAudio,
                    onImportDraftChange = { importDraft = it },
                    onImportConfirm = ::uploadDraft,
                    onRename = { renameTarget = it },
                    onRenameDismiss = { renameTarget = null },
                    onRenameConfirm = ::renamePlaylist,
                    onDelete = { deleteTarget = it },
                    onDeleteDismiss = { deleteTarget = null },
                    onDeleteConfirm = ::deletePlaylist,
                    onArmNfc = ::armNfc,
                    onCancelNfc = ::cancelNfc,
                    onDismissResult = { nfcResult = null },
                    onDismissStatus = { statusMessage = null },
                )
            }
        }
        if (session != null) refreshLibrary()
    }

    override fun onResume() {
        super.onResume()
        if (pendingNfc != null) enableNfcReader()
    }

    override fun onPause() {
        nfcAdapter?.disableReaderMode(this)
        super.onPause()
    }

    private fun authenticate(register: Boolean, email: String, password: String, name: String) {
        if (password.length < 10) {
            statusMessage = "Le mot de passe doit contenir au moins 10 caractères."
            return
        }
        loading = true
        statusMessage = null
        lifecycleScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    if (register) api.register(email, password, name) else api.login(email, password)
                }
            }.onSuccess { next ->
                tokenStore.save(next)
                session = next
                refreshLibrary()
            }.onFailure(::showError)
            loading = false
        }
    }

    private fun refreshLibrary() {
        val current = session ?: return
        loading = true
        lifecycleScope.launch {
            runCatching { withContext(Dispatchers.IO) { api.library(current.token) } }
                .onSuccess { library = it }
                .onFailure { error ->
                    if (error is ApiException && error.status == 401) clearSession()
                    showError(error)
                }
            loading = false
        }
    }

    private fun logout() {
        val current = session
        clearSession()
        if (current != null) lifecycleScope.launch(Dispatchers.IO) { runCatching { api.logout(current.token) } }
    }

    private fun clearSession() {
        tokenStore.clear()
        session = null
        library = null
        cancelNfc()
    }

    private fun pickAudio(target: CloudPlaylist?) {
        pickerTarget = target
        audioPicker.launch(arrayOf("audio/mpeg", "audio/mp3"))
    }

    private fun uploadDraft(draft: ImportDraft) {
        val current = session ?: return
        if (!Regex("^[2-8][0-9]{3}$").matches(draft.figureId)) {
            statusMessage = "L'identifiant doit être compris entre 2000 et 8999."
            return
        }
        if (draft.name.isBlank()) {
            statusMessage = "Donnez un nom à la playlist."
            return
        }
        loading = true
        statusMessage = "Envoi de ${draft.uris.size} piste(s)…"
        lifecycleScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    api.savePlaylist(current.token, draft.figureId, draft.name, draft.labels, resetAudio = true)
                    draft.uris.forEachIndexed { position, uri ->
                        val input = contentResolver.openInputStream(uri)
                            ?: throw IOException("Impossible de lire ${draft.labels[position]}.")
                        api.uploadAudio(current.token, draft.figureId, position, input)
                    }
                    api.library(current.token)
                }
            }.onSuccess {
                library = it
                importDraft = null
                statusMessage = "Playlist et fichiers audio synchronisés."
            }.onFailure { error ->
                showError(error)
                runCatching { withContext(Dispatchers.IO) { api.library(current.token) } }
                    .onSuccess { library = it }
            }
            loading = false
        }
    }

    private fun renamePlaylist(playlist: CloudPlaylist, name: String) {
        val current = session ?: return
        if (name.isBlank()) return
        loading = true
        lifecycleScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    api.savePlaylist(current.token, playlist.figureId, name, playlist.tracks.map { it.label }, resetAudio = false)
                }
            }.onSuccess {
                library = it
                renameTarget = null
            }.onFailure(::showError)
            loading = false
        }
    }

    private fun deletePlaylist(playlist: CloudPlaylist) {
        val current = session ?: return
        loading = true
        lifecycleScope.launch {
            runCatching { withContext(Dispatchers.IO) { api.deletePlaylist(current.token, playlist.figureId) } }
                .onSuccess {
                    library = it
                    deleteTarget = null
                }
                .onFailure(::showError)
            loading = false
        }
    }

    private fun armNfc(playlist: CloudPlaylist) {
        if (!PayloadValidator.isValid(playlist)) {
            statusMessage = "Cette playlist ne contient pas un identifiant FABA+ personnalisé valide."
            return
        }
        val adapter = nfcAdapter
        if (adapter == null) {
            statusMessage = "Ce téléphone ne possède pas de lecteur NFC."
            return
        }
        if (!adapter.isEnabled) {
            statusMessage = "Activez le NFC dans les réglages du téléphone puis réessayez."
            return
        }
        pendingNfc = playlist
        statusMessage = null
        enableNfcReader()
    }

    private fun enableNfcReader() {
        val adapter = nfcAdapter ?: return
        adapter.enableReaderMode(
            this,
            { tag -> handleTag(tag) },
            NfcAdapter.FLAG_READER_NFC_A or NfcAdapter.FLAG_READER_NFC_B or
                NfcAdapter.FLAG_READER_NFC_F or NfcAdapter.FLAG_READER_NFC_V,
            null,
        )
    }

    private fun handleTag(tag: Tag) {
        val playlist = pendingNfc ?: return
        if (!nfcWriting.compareAndSet(false, true)) return
        val result = writeNdef(tag, playlist.nfcPayload)
        runOnUiThread {
            nfcAdapter?.disableReaderMode(this)
            pendingNfc = null
            nfcWriting.set(false)
            nfcResult = if (result.success) {
                "Tag prêt pour « ${playlist.name} » (K${playlist.figureId})."
            } else {
                result.message
            }
        }
    }

    private fun cancelNfc() {
        pendingNfc = null
        nfcAdapter?.disableReaderMode(this)
    }

    private fun writeNdef(tag: Tag, payload: String): NfcWriteResult {
        val message = NdefMessage(arrayOf(NdefRecord.createTextRecord("fr", payload)))
        return try {
            val ndef = Ndef.get(tag)
            if (ndef != null) {
                ndef.connect()
                try {
                    if (!ndef.isWritable) return NfcWriteResult(false, "Ce tag NFC est verrouillé en lecture seule.")
                    if (ndef.maxSize < message.toByteArray().size) return NfcWriteResult(false, "Ce tag NFC n'a pas assez de mémoire.")
                    ndef.writeNdefMessage(message)
                    val verified = ndef.ndefMessage?.toByteArray()?.contentEquals(message.toByteArray()) == true
                    if (!verified) return NfcWriteResult(false, "Le tag a été écrit, mais la vérification a échoué. Réessayez avec un autre tag.")
                } finally {
                    ndef.close()
                }
            } else {
                val formatable = NdefFormatable.get(tag)
                    ?: return NfcWriteResult(false, "Ce tag NFC n'est pas compatible NDEF.")
                formatable.connect()
                try {
                    formatable.format(message)
                } finally {
                    formatable.close()
                }
            }
            NfcWriteResult(true, "Tag NFC écrit et vérifié.")
        } catch (error: Exception) {
            NfcWriteResult(false, "Écriture NFC impossible : ${error.message ?: "tag incompatible ou retiré trop tôt"}.")
        }
    }

    private fun displayName(uri: Uri): String {
        val raw = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
            ?: uri.lastPathSegment
            ?: "Piste"
        return raw.substringBeforeLast('.').ifBlank { "Piste" }.take(200)
    }

    private fun nextAvailableId(): String? {
        val used = library?.playlists?.mapTo(mutableSetOf()) { it.figureId }.orEmpty()
        return (2000..8999).firstOrNull { it.toString() !in used }?.toString()
    }

    private fun showError(error: Throwable) {
        statusMessage = error.message ?: "Une erreur inattendue est survenue."
    }
}

data class ImportDraft(
    val figureId: String,
    val name: String,
    val uris: List<Uri>,
    val labels: List<String>,
    val replacing: Boolean,
)

private data class NfcWriteResult(val success: Boolean, val message: String)

@Composable
private fun FabaApp(
    session: AccountSession?,
    library: CloudLibrary?,
    loading: Boolean,
    statusMessage: String?,
    importDraft: ImportDraft?,
    renameTarget: CloudPlaylist?,
    deleteTarget: CloudPlaylist?,
    pendingNfc: CloudPlaylist?,
    nfcResult: String?,
    onAuthenticate: (Boolean, String, String, String) -> Unit,
    onRefresh: () -> Unit,
    onLogout: () -> Unit,
    onImport: (CloudPlaylist?) -> Unit,
    onImportDraftChange: (ImportDraft?) -> Unit,
    onImportConfirm: (ImportDraft) -> Unit,
    onRename: (CloudPlaylist) -> Unit,
    onRenameDismiss: () -> Unit,
    onRenameConfirm: (CloudPlaylist, String) -> Unit,
    onDelete: (CloudPlaylist) -> Unit,
    onDeleteDismiss: () -> Unit,
    onDeleteConfirm: (CloudPlaylist) -> Unit,
    onArmNfc: (CloudPlaylist) -> Unit,
    onCancelNfc: () -> Unit,
    onDismissResult: () -> Unit,
    onDismissStatus: () -> Unit,
) {
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        if (session == null) {
            AuthScreen(loading, statusMessage, onAuthenticate, onDismissStatus)
        } else {
            LibraryScreen(session, library, loading, statusMessage, onRefresh, onLogout, onImport, onRename, onDelete, onArmNfc, onDismissStatus)
        }
    }
    importDraft?.let { draft ->
        ImportDialog(draft, loading, onImportDraftChange, onImportConfirm)
    }
    renameTarget?.let { playlist ->
        NameDialog("Renommer la playlist", playlist.name, onRenameDismiss) { onRenameConfirm(playlist, it) }
    }
    deleteTarget?.let { playlist ->
        AlertDialog(
            onDismissRequest = onDeleteDismiss,
            title = { Text("Supprimer « ${playlist.name} » ?") },
            text = { Text("La playlist et tous ses fichiers audio seront supprimés du cloud. Cette action n'efface pas les cartes SD déjà préparées.") },
            confirmButton = { TextButton(onClick = { onDeleteConfirm(playlist) }) { Text("Supprimer", color = MaterialTheme.colorScheme.error) } },
            dismissButton = { TextButton(onClick = onDeleteDismiss) { Text("Annuler") } },
        )
    }
    pendingNfc?.let { playlist ->
        AlertDialog(
            onDismissRequest = onCancelNfc,
            icon = { Text("◉", color = MaterialTheme.colorScheme.primary, fontSize = 34.sp) },
            title = { Text("Approchez le tag NFC") },
            text = { Text("Maintenez le tag contre le téléphone pour préparer K${playlist.figureId} — ${playlist.name}. Le code est verrouillé par l'application.") },
            confirmButton = {},
            dismissButton = { TextButton(onClick = onCancelNfc) { Text("Annuler") } },
        )
    }
    nfcResult?.let { message ->
        AlertDialog(
            onDismissRequest = onDismissResult,
            title = { Text(if (message.startsWith("Tag prêt")) "Tag NFC prêt" else "Résultat NFC") },
            text = { Text(message) },
            confirmButton = { Button(onClick = onDismissResult) { Text("Terminé") } },
        )
    }
}

@Composable
private fun AuthScreen(
    loading: Boolean,
    statusMessage: String?,
    onAuthenticate: (Boolean, String, String, String) -> Unit,
    onDismissStatus: () -> Unit,
) {
    var register by remember { mutableStateOf(false) }
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var name by remember { mutableStateOf("") }
    Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        Column(verticalArrangement = Arrangement.spacedBy(14.dp), modifier = Modifier.fillMaxWidth()) {
            Box(Modifier.size(64.dp).background(MaterialTheme.colorScheme.primary, RoundedCornerShape(20.dp)), contentAlignment = Alignment.Center) {
                Text("F+", color = Color.White, fontWeight = FontWeight.Black, fontSize = 22.sp)
            }
            Text("FABA Tag", style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Black)
            Text("Votre bibliothèque audio et vos tags NFC, sans configuration.", color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (register) OutlinedTextField(name, { name = it }, label = { Text("Nom affiché") }, modifier = Modifier.fillMaxWidth(), singleLine = true)
            OutlinedTextField(email, { email = it }, label = { Text("Adresse e-mail") }, modifier = Modifier.fillMaxWidth(), singleLine = true)
            OutlinedTextField(password, { password = it }, label = { Text("Mot de passe") }, modifier = Modifier.fillMaxWidth(), singleLine = true, visualTransformation = PasswordVisualTransformation())
            statusMessage?.let { StatusCard(it, onDismissStatus) }
            Button(
                onClick = { onAuthenticate(register, email, password, name) },
                enabled = !loading && email.isNotBlank() && password.isNotBlank() && (!register || name.isNotBlank()),
                modifier = Modifier.fillMaxWidth().height(52.dp),
            ) {
                if (loading) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = Color.White) else Text(if (register) "Créer mon compte" else "Se connecter")
            }
            TextButton(onClick = { register = !register }, modifier = Modifier.align(Alignment.CenterHorizontally)) {
                Text(if (register) "J'ai déjà un compte" else "Créer un compte")
            }
        }
    }
}

@Composable
private fun LibraryScreen(
    session: AccountSession,
    library: CloudLibrary?,
    loading: Boolean,
    statusMessage: String?,
    onRefresh: () -> Unit,
    onLogout: () -> Unit,
    onImport: (CloudPlaylist?) -> Unit,
    onRename: (CloudPlaylist) -> Unit,
    onDelete: (CloudPlaylist) -> Unit,
    onArmNfc: (CloudPlaylist) -> Unit,
    onDismissStatus: () -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        Surface(shadowElevation = 2.dp) {
            Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(42.dp).background(MaterialTheme.colorScheme.primary, RoundedCornerShape(13.dp)), contentAlignment = Alignment.Center) {
                        Text("F+", color = Color.White, fontWeight = FontWeight.Black)
                    }
                    Column(Modifier.padding(start = 12.dp).weight(1f)) {
                        Text("Bonjour ${session.displayName}", fontWeight = FontWeight.ExtraBold, fontSize = 19.sp)
                        Text(session.email, color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp)
                    }
                    TextButton(onClick = onRefresh, enabled = !loading) { Text("Actualiser") }
                }
                library?.let {
                    val percent = if (it.storageLimitBytes > 0) (it.storageUsedBytes * 100 / it.storageLimitBytes) else 0
                    Text("${it.playlists.size} playlists · ${formatBytes(it.storageUsedBytes)} utilisés · $percent % du quota", modifier = Modifier.padding(top = 12.dp), color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp)
                }
            }
        }
        if (statusMessage != null) Box(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) { StatusCard(statusMessage, onDismissStatus) }
        if (library == null && loading) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        } else {
            LazyColumn(
                contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 110.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
                modifier = Modifier.weight(1f),
            ) {
                if (library?.playlists.isNullOrEmpty()) {
                    item { EmptyLibrary() }
                } else {
                    items(library.playlists, key = { it.figureId }) { playlist ->
                        PlaylistCard(playlist, loading, onImport, onRename, onDelete, onArmNfc)
                    }
                }
            }
        }
        Surface(shadowElevation = 8.dp) {
            Row(Modifier.fillMaxWidth().padding(14.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Button(onClick = { onImport(null) }, enabled = !loading, modifier = Modifier.weight(1f).height(48.dp)) { Text("＋ Importer des MP3") }
                OutlinedButton(onClick = onLogout, modifier = Modifier.height(48.dp)) { Text("Déconnexion") }
            }
        }
    }
}

@Composable
private fun PlaylistCard(
    playlist: CloudPlaylist,
    loading: Boolean,
    onImport: (CloudPlaylist?) -> Unit,
    onRename: (CloudPlaylist) -> Unit,
    onDelete: (CloudPlaylist) -> Unit,
    onArmNfc: (CloudPlaylist) -> Unit,
) {
    val complete = playlist.tracks.isNotEmpty() && playlist.tracks.all { it.audioAvailable }
    Surface(shape = RoundedCornerShape(20.dp), color = Color.White, tonalElevation = 1.dp, shadowElevation = 1.dp) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(52.dp).background(Color(0xFFF0ECFF), RoundedCornerShape(16.dp)), contentAlignment = Alignment.Center) {
                    Text("K${playlist.figureId}", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Black, fontSize = 12.sp)
                }
                Column(Modifier.padding(start = 12.dp).weight(1f)) {
                    Text(playlist.name, fontWeight = FontWeight.ExtraBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text("${playlist.trackCount} piste(s) · ${if (complete) "audio complet" else "audio à compléter"}", color = if (complete) Color(0xFF247A58) else Color(0xFFA26816), fontSize = 12.sp)
                }
                TextButton(onClick = { onRename(playlist) }, enabled = !loading) { Text("Renommer") }
            }
            playlist.tracks.take(3).forEach { track ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(String.format("%02d", track.position + 1), color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold, fontSize = 11.sp)
                    Text(track.label, modifier = Modifier.padding(start = 10.dp).weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis, fontSize = 13.sp)
                    Text(if (track.audioAvailable) "✓" else "!", color = if (track.audioAvailable) Color(0xFF247A58) else Color(0xFFA26816))
                }
            }
            if (playlist.tracks.size > 3) Text("+ ${playlist.tracks.size - 3} autre(s) piste(s)", color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 11.sp)
            HorizontalDivider(color = Color(0xFFEEEAF3))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { onArmNfc(playlist) }, enabled = complete && !loading, modifier = Modifier.weight(1f)) { Text("Écrire le tag NFC") }
                OutlinedButton(onClick = { onImport(playlist) }, enabled = !loading) { Text("Remplacer") }
                TextButton(onClick = { onDelete(playlist) }, enabled = !loading) { Text("Supprimer", color = MaterialTheme.colorScheme.error) }
            }
        }
    }
}

@Composable
private fun EmptyLibrary() {
    Column(Modifier.fillMaxWidth().padding(vertical = 70.dp, horizontal = 25.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Text("♫", color = MaterialTheme.colorScheme.primary, fontSize = 42.sp)
        Text("Votre bibliothèque est vide", fontWeight = FontWeight.ExtraBold, fontSize = 18.sp)
        Text("Importez des MP3 ici ou synchronisez une carte depuis l'application PC.", modifier = Modifier.padding(top = 8.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun ImportDialog(
    draft: ImportDraft,
    loading: Boolean,
    onChange: (ImportDraft?) -> Unit,
    onConfirm: (ImportDraft) -> Unit,
) {
    AlertDialog(
        onDismissRequest = { if (!loading) onChange(null) },
        title = { Text(if (draft.replacing) "Remplacer les musiques" else "Nouvelle playlist") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(draft.name, { onChange(draft.copy(name = it)) }, label = { Text("Nom") }, singleLine = true)
                OutlinedTextField(draft.figureId, { if (!draft.replacing) onChange(draft.copy(figureId = it.filter(Char::isDigit).take(4))) }, label = { Text("ID FABA+ (2000–8999)") }, enabled = !draft.replacing, singleLine = true)
                Text("${draft.uris.size} MP3 sélectionné(s). L'ordre choisi par Android sera l'ordre de lecture.", color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp)
                draft.labels.take(5).forEachIndexed { index, label -> Text("${index + 1}. $label", maxLines = 1, overflow = TextOverflow.Ellipsis, fontSize = 12.sp) }
            }
        },
        confirmButton = { Button(onClick = { onConfirm(draft) }, enabled = !loading) { Text(if (loading) "Envoi…" else "Synchroniser") } },
        dismissButton = { TextButton(onClick = { onChange(null) }, enabled = !loading) { Text("Annuler") } },
    )
}

@Composable
private fun NameDialog(title: String, initial: String, onDismiss: () -> Unit, onConfirm: (String) -> Unit) {
    var value by remember(initial) { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { OutlinedTextField(value, { value = it }, label = { Text("Nom") }, singleLine = true) },
        confirmButton = { Button(onClick = { onConfirm(value) }, enabled = value.isNotBlank()) { Text("Enregistrer") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Annuler") } },
    )
}

@Composable
private fun StatusCard(message: String, onDismiss: () -> Unit) {
    Surface(color = Color(0xFFFFF4DC), shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(message, modifier = Modifier.weight(1f), color = Color(0xFF765313), fontSize = 12.sp)
            TextButton(onClick = onDismiss) { Text("Fermer") }
        }
    }
}

@Composable
private fun FabaTheme(content: @Composable () -> Unit) {
    val colors = androidx.compose.material3.lightColorScheme(
        primary = Color(0xFF6847C7),
        onPrimary = Color.White,
        secondary = Color(0xFFF4A641),
        background = Color(0xFFF7F7FC),
        surface = Color(0xFFFFFFFF),
        onSurface = Color(0xFF292535),
        onSurfaceVariant = Color(0xFF777181),
        error = Color(0xFFB22D3B),
    )
    MaterialTheme(colorScheme = colors, typography = MaterialTheme.typography, content = content)
}

private fun formatBytes(bytes: Long): String = when {
    bytes >= 1024L * 1024 * 1024 -> String.format("%.1f Go", bytes / (1024.0 * 1024 * 1024))
    bytes >= 1024L * 1024 -> String.format("%.1f Mo", bytes / (1024.0 * 1024))
    else -> String.format("%.0f Ko", bytes / 1024.0)
}
