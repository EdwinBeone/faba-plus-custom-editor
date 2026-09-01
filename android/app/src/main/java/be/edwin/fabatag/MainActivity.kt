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
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import android.provider.Settings
import android.util.Log
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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
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
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.io.IOException

private const val NFC_LOG_TAG = "FabaNfc"

class MainActivity : ComponentActivity() {
    private val api = ApiClient()
    private val updateClient = GitHubUpdateClient()
    private lateinit var tokenStore: TokenStore
    private var session by mutableStateOf<AccountSession?>(null)
    private var library by mutableStateOf<CloudLibrary?>(null)
    private var loading by mutableStateOf(false)
    private var statusMessage by mutableStateOf<String?>(null)
    private var importDraft by mutableStateOf<ImportDraft?>(null)
    private var renameTarget by mutableStateOf<CloudPlaylist?>(null)
    private var deleteTarget by mutableStateOf<CloudPlaylist?>(null)
    private var pendingNfc by mutableStateOf<CloudPlaylist?>(null)
    private var nfcProgress by mutableStateOf<NfcProgress?>(null)
    private var nfcResult by mutableStateOf<String?>(null)
    private var availableUpdate by mutableStateOf<AppUpdate?>(null)
    private var updateDialogVisible by mutableStateOf(false)
    private var updateChecking by mutableStateOf(false)
    private var updateDownloading by mutableStateOf(false)
    private var updateProgress by mutableStateOf<Int?>(null)
    private var updateError by mutableStateOf<String?>(null)
    private var pendingInstallApk: File? = null
    private var pickerTarget: CloudPlaylist? = null
    private var nfcAdapter: NfcAdapter? = null
    @Volatile
    private var nfcSession: NfcWriteSession? = null

    private val unknownSourcesLauncher = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        val apk = pendingInstallApk ?: return@registerForActivityResult
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O || packageManager.canRequestPackageInstalls()) {
            pendingInstallApk = null
            launchPackageInstaller(apk)
        } else {
            pendingInstallApk = null
            statusMessage = "Autorisez FABA Tag à installer des applications, puis relancez la mise à jour."
        }
    }

    private val audioPicker = registerForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isEmpty()) return@registerForActivityResult
        val target = pickerTarget
        val existingTracks = target?.tracks?.map { track ->
            DraftTrack(
                label = track.label,
                sourcePosition = track.position,
                uri = null,
                audioAvailable = track.audioAvailable,
            )
        }.orEmpty()
        if (existingTracks.size + uris.size > 99) {
            statusMessage = "Une playlist est limitée à 99 pistes."
            return@registerForActivityResult
        }
        val suggestedId = target?.figureId ?: nextAvailableId()
        if (suggestedId == null) {
            statusMessage = "Aucun identifiant personnalisé libre entre 2000 et 8999."
            return@registerForActivityResult
        }
        importDraft = ImportDraft(
            figureId = suggestedId,
            name = target?.name ?: "Ma playlist",
            tracks = existingTracks + uris.map { uri ->
                DraftTrack(displayName(uri), sourcePosition = null, uri = uri, audioAvailable = true)
            },
            editing = target != null,
        )
    }

    private val additionalAudioPicker = registerForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        val draft = importDraft ?: return@registerForActivityResult
        if (uris.isEmpty()) return@registerForActivityResult
        if (draft.tracks.size + uris.size > 99) {
            statusMessage = "Une playlist est limitée à 99 pistes."
            return@registerForActivityResult
        }
        importDraft = draft.copy(
            tracks = draft.tracks + uris.map { uri ->
                DraftTrack(displayName(uri), sourcePosition = null, uri = uri, audioAvailable = true)
            },
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
                    nfcProgress = nfcProgress,
                    nfcResult = nfcResult,
                    availableUpdate = availableUpdate,
                    updateDialogVisible = updateDialogVisible,
                    updateChecking = updateChecking,
                    updateDownloading = updateDownloading,
                    updateProgress = updateProgress,
                    updateError = updateError,
                    onAuthenticate = ::authenticate,
                    onRefresh = ::refreshLibrary,
                    onLogout = ::logout,
                    onImport = ::pickAudio,
                    onImportDraftChange = { importDraft = it },
                    onImportConfirm = ::uploadDraft,
                    onEdit = ::editPlaylist,
                    onAddToDraft = ::pickAdditionalAudio,
                    onRename = { renameTarget = it },
                    onRenameDismiss = { renameTarget = null },
                    onRenameConfirm = ::renamePlaylist,
                    onDelete = { deleteTarget = it },
                    onDeleteDismiss = { deleteTarget = null },
                    onDeleteConfirm = ::deletePlaylist,
                    onArmNfc = ::armNfc,
                    onCancelNfc = ::cancelNfc,
                    onDismissResult = ::dismissNfcResult,
                    onDismissStatus = { statusMessage = null },
                    onCheckUpdate = ::openOrCheckUpdate,
                    onInstallUpdate = ::downloadAndInstallUpdate,
                    onDismissUpdate = { if (!updateDownloading) updateDialogVisible = false },
                )
            }
        }
        if (session != null) refreshLibrary()
        checkForAppUpdate(manual = false)
    }

    override fun onResume() {
        super.onResume()
        val currentSession = nfcSession
        val playlist = pendingNfc
        if (currentSession != null && playlist != null && currentSession.acceptsReaderCallbacks()) {
            val purpose = when (currentSession.currentPhase()) {
                NfcSessionPhase.WAITING_FOR_TAG -> NfcReaderPurpose.INITIAL
                NfcSessionPhase.WAITING_FOR_VERIFICATION -> NfcReaderPurpose.VERIFICATION
                else -> null
            }
            if (purpose != null) enableNfcReader(currentSession, playlist, purpose)
        }
    }

    override fun onPause() {
        nfcAdapter?.disableReaderMode(this)
        super.onPause()
    }

    private fun openOrCheckUpdate() {
        if (availableUpdate != null) {
            updateError = null
            updateDialogVisible = true
        } else {
            checkForAppUpdate(manual = true)
        }
    }

    private fun checkForAppUpdate(manual: Boolean) {
        if (updateChecking || updateDownloading) return
        updateChecking = true
        if (manual) statusMessage = "Recherche de la dernière release GitHub…"
        lifecycleScope.launch {
            runCatching { withContext(Dispatchers.IO) { updateClient.check() } }
                .onSuccess { update ->
                    availableUpdate = update
                    if (update != null) {
                        updateError = null
                        updateDialogVisible = true
                        statusMessage = null
                    } else if (manual) {
                        statusMessage = "FABA Tag ${BuildConfig.VERSION_NAME} est déjà à jour."
                    }
                }
                .onFailure { error ->
                    if (manual) statusMessage = error.message ?: "Vérification de la mise à jour impossible."
                }
            updateChecking = false
        }
    }

    private fun downloadAndInstallUpdate() {
        val update = availableUpdate ?: return
        if (updateDownloading) return
        updateDownloading = true
        updateProgress = 0
        updateError = null
        lifecycleScope.launch {
            val destination = File(File(cacheDir, "updates"), "FABA-Tag-${update.version}.apk")
            runCatching {
                withContext(Dispatchers.IO) {
                    updateClient.download(update, destination) { progress ->
                        runOnUiThread { updateProgress = progress }
                    }
                }
            }.onSuccess {
                updateDownloading = false
                updateProgress = 100
                updateDialogVisible = false
                installDownloadedApk(destination)
            }.onFailure { error ->
                updateDownloading = false
                updateProgress = null
                updateError = error.message ?: "Téléchargement de la mise à jour impossible."
            }
        }
    }

    private fun installDownloadedApk(apk: File) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !packageManager.canRequestPackageInstalls()) {
            pendingInstallApk = apk
            statusMessage = "Android doit autoriser FABA Tag comme source de mise à jour. Activez l'autorisation sur l'écran suivant."
            unknownSourcesLauncher.launch(
                Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:$packageName")),
            )
            return
        }
        launchPackageInstaller(apk)
    }

    private fun launchPackageInstaller(apk: File) {
        runCatching {
            val uri = FileProvider.getUriForFile(this, "$packageName.fileprovider", apk)
            startActivity(
                Intent(Intent.ACTION_VIEW)
                    .setDataAndType(uri, "application/vnd.android.package-archive")
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
            )
        }.onSuccess {
            statusMessage = "Mise à jour vérifiée. Confirmez maintenant l'installation dans Android."
        }.onFailure { error ->
            statusMessage = error.message ?: "Android n'a pas pu ouvrir l'installateur de mise à jour."
        }
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

    private fun editPlaylist(playlist: CloudPlaylist) {
        importDraft = ImportDraft(
            figureId = playlist.figureId,
            name = playlist.name,
            tracks = playlist.tracks.map { track ->
                DraftTrack(
                    label = track.label,
                    sourcePosition = track.position,
                    uri = null,
                    audioAvailable = track.audioAvailable,
                )
            },
            editing = true,
        )
    }

    private fun pickAdditionalAudio() {
        additionalAudioPicker.launch(arrayOf("audio/mpeg", "audio/mp3"))
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
        if (draft.tracks.isEmpty() || draft.tracks.size > 99) {
            statusMessage = "Une playlist doit contenir entre 1 et 99 pistes."
            return
        }
        if (draft.tracks.any { it.uri == null && !it.audioAvailable }) {
            statusMessage = "Une piste conservée n'a pas de fichier audio dans le cloud. Supprimez-la ou ajoutez son MP3."
            return
        }
        loading = true
        statusMessage = "Synchronisation de ${draft.tracks.size} piste(s)…"
        lifecycleScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    val temporaryDirectory = File(cacheDir, "playlist-edit-${System.nanoTime()}")
                    try {
                        temporaryDirectory.mkdirs()
                        val downloaded = mutableMapOf<Int, File>()
                        draft.tracks.mapNotNull { it.sourcePosition }.distinct().forEach { sourcePosition ->
                            val destination = File(temporaryDirectory, "$sourcePosition.mp3")
                            api.downloadAudio(current.token, draft.figureId, sourcePosition, destination)
                            downloaded[sourcePosition] = destination
                        }
                        api.savePlaylist(
                            current.token,
                            draft.figureId,
                            draft.name,
                            draft.tracks.map { it.label },
                            resetAudio = true,
                        )
                        draft.tracks.forEachIndexed { position, track ->
                            val input = track.uri?.let(contentResolver::openInputStream)
                                ?: track.sourcePosition?.let { downloaded[it]?.inputStream() }
                                ?: throw IOException("Impossible de lire ${track.label}.")
                            api.uploadAudio(current.token, draft.figureId, position, input)
                        }
                    } finally {
                        temporaryDirectory.deleteRecursively()
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
        nfcSession?.cancel()
        adapter.disableReaderMode(this)
        val newSession = NfcWriteSession()
        nfcSession = newSession
        pendingNfc = playlist
        nfcProgress = NfcProgress(
            currentStep = 0,
            message = "Approchez et maintenez le tag contre le téléphone.",
        )
        nfcResult = null
        statusMessage = null
        Log.i(NFC_LOG_TAG, "session=${newSession.id} armed figure=K${playlist.figureId}")
        enableNfcReader(newSession, playlist, NfcReaderPurpose.INITIAL)
    }

    private fun enableNfcReader(
        nfcSession: NfcWriteSession,
        playlist: CloudPlaylist,
        purpose: NfcReaderPurpose,
    ) {
        val adapter = nfcAdapter ?: return
        if (this.nfcSession !== nfcSession || !nfcSession.acceptsReaderCallbacks()) return
        Log.i(
            NFC_LOG_TAG,
            "session=${nfcSession.id} reader enabled phase=${nfcSession.currentPhase()} purpose=$purpose",
        )
        adapter.enableReaderMode(
            this,
            { tag -> handleTag(tag, nfcSession, playlist, purpose) },
            NfcAdapter.FLAG_READER_NFC_A or NfcAdapter.FLAG_READER_NFC_B or
                NfcAdapter.FLAG_READER_NFC_F or NfcAdapter.FLAG_READER_NFC_V,
            null,
        )
    }

    private fun handleTag(
        tag: Tag,
        nfcSession: NfcWriteSession,
        playlist: CloudPlaylist,
        purpose: NfcReaderPurpose,
    ) {
        if (this.nfcSession !== nfcSession) {
            Log.i(NFC_LOG_TAG, "session=${nfcSession.id} ignored callback from an obsolete reader")
            return
        }
        val tagId = tag.id.joinToString("") { "%02X".format(it.toInt() and 0xff) }
        when (purpose) {
            NfcReaderPurpose.INITIAL -> if (nfcSession.beginInspection()) {
                Log.i(NFC_LOG_TAG, "session=${nfcSession.id} initial tag claimed uid=$tagId")
                when (val outcome = inspectClearAndWrite(tag, playlist.nfcPayload, nfcSession)) {
                    is NfcInitialOutcome.Complete -> finishNfc(nfcSession, playlist, outcome.result)
                    NfcInitialOutcome.NeedsFreshVerification -> restartReaderForVerification(nfcSession, playlist)
                }
            } else {
                Log.i(
                    NFC_LOG_TAG,
                    "session=${nfcSession.id} initial callback ignored phase=${nfcSession.currentPhase()} uid=$tagId",
                )
            }

            NfcReaderPurpose.VERIFICATION -> if (nfcSession.beginVerification()) {
                Log.i(NFC_LOG_TAG, "session=${nfcSession.id} verification tag claimed uid=$tagId")
                updateNfcProgress(nfcSession, 4, "Vérification des données écrites…")
                finishNfc(nfcSession, playlist, verifyNdef(tag, playlist.nfcPayload, nfcSession))
            } else {
                Log.i(
                    NFC_LOG_TAG,
                    "session=${nfcSession.id} verification callback ignored phase=${nfcSession.currentPhase()} uid=$tagId",
                )
            }
        }
    }

    private fun cancelNfc() {
        val currentSession = nfcSession
        currentSession?.cancel()
        if (currentSession != null) Log.i(NFC_LOG_TAG, "session=${currentSession.id} cancelled")
        nfcSession = null
        pendingNfc = null
        nfcProgress = null
        nfcAdapter?.disableReaderMode(this)
    }

    private fun dismissNfcResult() {
        val currentSession = nfcSession
        if (currentSession?.currentPhase() == NfcSessionPhase.FINISHED) nfcSession = null
        nfcResult = null
    }

    private fun inspectClearAndWrite(
        tag: Tag,
        payload: String,
        nfcSession: NfcWriteSession,
    ): NfcInitialOutcome {
        val expectedMessage = ndefTextMessage(payload)
        var currentStep = 1
        return try {
            updateNfcProgress(nfcSession, 1, "Vérification de l’état actuel du tag…")
            ensureSessionPhase(nfcSession, NfcSessionPhase.INSPECTING)
            val ndef = Ndef.get(tag)
            if (ndef != null) {
                ndef.connect()
                try {
                    val existingMessage = ndef.ndefMessage
                    Log.i(
                        NFC_LOG_TAG,
                        "session=${nfcSession.id} step=1 inspected ndef=true writable=${ndef.isWritable} " +
                            "maxSize=${ndef.maxSize} existingRecords=${existingMessage?.records?.size ?: 0}",
                    )
                    if (!ndef.isWritable) {
                        return NfcInitialOutcome.Complete(NfcWriteResult(false, "Ce tag NFC est verrouillé en lecture seule."))
                    }
                    if (ndef.maxSize < expectedMessage.toByteArray().size) {
                        return NfcInitialOutcome.Complete(NfcWriteResult(false, "Ce tag NFC n'a pas assez de mémoire."))
                    }

                    advanceNfcStep(nfcSession.beginClearing())
                    currentStep = 2
                    updateNfcProgress(nfcSession, 2, "Suppression des anciennes données…")
                    ensureSessionPhase(nfcSession, NfcSessionPhase.CLEARING)
                    if (isEmptyNdef(existingMessage)) {
                        Log.i(NFC_LOG_TAG, "session=${nfcSession.id} step=2 no existing data")
                    } else {
                        val emptyMessage = emptyNdefMessage()
                        ndef.writeNdefMessage(emptyMessage)
                        if (!isEmptyNdef(ndef.ndefMessage)) {
                            return NfcInitialOutcome.Complete(
                                NfcWriteResult(false, "Les anciennes données du tag n'ont pas pu être supprimées."),
                            )
                        }
                        Log.i(NFC_LOG_TAG, "session=${nfcSession.id} step=2 existing data cleared")
                    }

                    advanceNfcStep(nfcSession.beginWriting())
                    currentStep = 3
                    updateNfcProgress(nfcSession, 3, "Écriture du nouveau contenu…")
                    ensureSessionPhase(nfcSession, NfcSessionPhase.WRITING)
                    ndef.writeNdefMessage(expectedMessage)
                    Log.i(NFC_LOG_TAG, "session=${nfcSession.id} step=3 payload written")

                    advanceNfcStep(nfcSession.beginInlineVerification())
                    currentStep = 4
                    updateNfcProgress(nfcSession, 4, "Vérification des données écrites…")
                    ensureSessionPhase(nfcSession, NfcSessionPhase.VERIFYING)
                    val verified = ndef.ndefMessage?.toByteArray()?.contentEquals(expectedMessage.toByteArray()) == true
                    if (!verified) {
                        return NfcInitialOutcome.Complete(
                            NfcWriteResult(false, "Le tag a été écrit, mais la vérification a échoué. Réessayez avec un autre tag."),
                        )
                    }
                    Log.i(NFC_LOG_TAG, "session=${nfcSession.id} step=4 payload verified")
                } finally {
                    ndef.close()
                }
            } else {
                val formatable = NdefFormatable.get(tag)
                    ?: return NfcInitialOutcome.Complete(NfcWriteResult(false, "Ce tag NFC n'est pas compatible NDEF."))
                Log.i(NFC_LOG_TAG, "session=${nfcSession.id} step=1 inspected ndef=false formatable=true")

                advanceNfcStep(nfcSession.beginClearing())
                currentStep = 2
                updateNfcProgress(nfcSession, 2, "Aucune ancienne donnée à supprimer.")
                ensureSessionPhase(nfcSession, NfcSessionPhase.CLEARING)
                Log.i(NFC_LOG_TAG, "session=${nfcSession.id} step=2 unformatted tag has no existing NDEF data")

                advanceNfcStep(nfcSession.beginWriting())
                currentStep = 3
                updateNfcProgress(nfcSession, 3, "Formatage et écriture du nouveau contenu…")
                ensureSessionPhase(nfcSession, NfcSessionPhase.WRITING)
                formatable.connect()
                try {
                    formatable.format(expectedMessage)
                    Log.i(NFC_LOG_TAG, "session=${nfcSession.id} step=3 tag formatted and payload written")
                } finally {
                    formatable.close()
                }
                return NfcInitialOutcome.NeedsFreshVerification
            }
            NfcInitialOutcome.Complete(NfcWriteResult(true, "Tag NFC écrit et vérifié."))
        } catch (error: Exception) {
            Log.e(NFC_LOG_TAG, "session=${nfcSession.id} failed at step=$currentStep", error)
            NfcInitialOutcome.Complete(
                NfcWriteResult(
                    false,
                    "Étape $currentStep/4 impossible : ${error.message ?: "tag incompatible ou retiré trop tôt"}.",
                ),
            )
        }
    }

    private fun restartReaderForVerification(nfcSession: NfcWriteSession, playlist: CloudPlaylist) {
        runOnUiThread {
            if (this.nfcSession !== nfcSession) return@runOnUiThread
            nfcAdapter?.disableReaderMode(this)
            if (!nfcSession.awaitFreshVerification()) return@runOnUiThread
            Log.i(NFC_LOG_TAG, "session=${nfcSession.id} step=4 waiting for fresh NDEF discovery")
            nfcProgress = NfcProgress(
                currentStep = 4,
                message = "Vérification finale… Maintenez encore le tag contre le téléphone.",
            )
            enableNfcReader(nfcSession, playlist, NfcReaderPurpose.VERIFICATION)
        }
    }

    private fun verifyNdef(tag: Tag, payload: String, nfcSession: NfcWriteSession): NfcWriteResult {
        val expectedMessage = ndefTextMessage(payload)
        return try {
            val ndef = Ndef.get(tag)
                ?: return NfcWriteResult(false, "Le tag a été écrit, mais il n'est pas lisible en NDEF pour la vérification.")
            ndef.connect()
            try {
                if (nfcSession.currentPhase() != NfcSessionPhase.VERIFYING) throw IOException("Session NFC annulée")
                val verified = ndef.ndefMessage?.toByteArray()?.contentEquals(expectedMessage.toByteArray()) == true
                if (!verified) {
                    return NfcWriteResult(false, "Le tag a été écrit, mais la vérification a échoué. Réessayez avec un autre tag.")
                }
            } finally {
                ndef.close()
            }
            Log.i(NFC_LOG_TAG, "session=${nfcSession.id} step=4 payload verified after formatting")
            NfcWriteResult(true, "Tag NFC écrit et vérifié.")
        } catch (error: Exception) {
            Log.e(NFC_LOG_TAG, "session=${nfcSession.id} failed at step=4", error)
            NfcWriteResult(false, "Étape 4/4 impossible : ${error.message ?: "tag incompatible ou retiré trop tôt"}.")
        }
    }

    private fun finishNfc(nfcSession: NfcWriteSession, playlist: CloudPlaylist, result: NfcWriteResult) {
        if (!nfcSession.finish()) return
        Log.i(NFC_LOG_TAG, "session=${nfcSession.id} finished success=${result.success}; reader will be disabled")
        runOnUiThread {
            if (this.nfcSession !== nfcSession) return@runOnUiThread
            nfcAdapter?.disableReaderMode(this)
            pendingNfc = null
            nfcProgress = null
            nfcResult = if (result.success) {
                "Les 4 étapes sont terminées. Tag prêt pour « ${playlist.name} » (K${playlist.figureId})."
            } else {
                result.message
            }
        }
    }

    private fun updateNfcProgress(nfcSession: NfcWriteSession, currentStep: Int, message: String) {
        runOnUiThread {
            if (this.nfcSession === nfcSession && pendingNfc != null) {
                nfcProgress = NfcProgress(currentStep, message)
            }
        }
    }

    private fun advanceNfcStep(advanced: Boolean) {
        if (!advanced) throw IOException("Session NFC annulée")
    }

    private fun ensureSessionPhase(nfcSession: NfcWriteSession, expected: NfcSessionPhase) {
        if (nfcSession.currentPhase() != expected) throw IOException("Session NFC annulée")
    }

    private fun ndefTextMessage(payload: String): NdefMessage =
        NdefMessage(arrayOf(NdefRecord.createTextRecord("fr", payload)))

    private fun emptyNdefMessage(): NdefMessage = NdefMessage(
        arrayOf(NdefRecord(NdefRecord.TNF_EMPTY, byteArrayOf(), byteArrayOf(), byteArrayOf())),
    )

    private fun isEmptyNdef(message: NdefMessage?): Boolean = message == null || message.records.all { record ->
        record.tnf == NdefRecord.TNF_EMPTY && record.type.isEmpty() && record.id.isEmpty() && record.payload.isEmpty()
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
    val tracks: List<DraftTrack>,
    val editing: Boolean,
)

data class DraftTrack(
    val label: String,
    val sourcePosition: Int?,
    val uri: Uri?,
    val audioAvailable: Boolean,
)

private data class NfcWriteResult(val success: Boolean, val message: String)

private sealed interface NfcInitialOutcome {
    data class Complete(val result: NfcWriteResult) : NfcInitialOutcome
    data object NeedsFreshVerification : NfcInitialOutcome
}

private data class NfcProgress(
    val currentStep: Int,
    val message: String,
)

private enum class NfcReaderPurpose {
    INITIAL,
    VERIFICATION,
}

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
    nfcProgress: NfcProgress?,
    nfcResult: String?,
    availableUpdate: AppUpdate?,
    updateDialogVisible: Boolean,
    updateChecking: Boolean,
    updateDownloading: Boolean,
    updateProgress: Int?,
    updateError: String?,
    onAuthenticate: (Boolean, String, String, String) -> Unit,
    onRefresh: () -> Unit,
    onLogout: () -> Unit,
    onImport: (CloudPlaylist?) -> Unit,
    onImportDraftChange: (ImportDraft?) -> Unit,
    onImportConfirm: (ImportDraft) -> Unit,
    onEdit: (CloudPlaylist) -> Unit,
    onAddToDraft: () -> Unit,
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
    onCheckUpdate: () -> Unit,
    onInstallUpdate: () -> Unit,
    onDismissUpdate: () -> Unit,
) {
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        Box(Modifier.fillMaxSize().safeDrawingPadding()) {
            if (session == null) {
                AuthScreen(loading, statusMessage, updateChecking, availableUpdate, onAuthenticate, onDismissStatus, onCheckUpdate)
            } else {
                LibraryScreen(session, library, loading, statusMessage, updateChecking, availableUpdate, onRefresh, onLogout, onImport, onEdit, onRename, onDelete, onArmNfc, onDismissStatus, onCheckUpdate)
            }
        }
    }
    importDraft?.let { draft ->
        ImportDialog(draft, loading, onImportDraftChange, onImportConfirm, onAddToDraft)
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
            title = { Text(if ((nfcProgress?.currentStep ?: 0) == 0) "Approchez le tag NFC" else "Préparation du tag") },
            text = {
                NfcProgressContent(
                    playlist = playlist,
                    progress = nfcProgress ?: NfcProgress(0, "Approchez et maintenez le tag contre le téléphone."),
                )
            },
            confirmButton = {},
            dismissButton = { TextButton(onClick = onCancelNfc) { Text("Annuler") } },
        )
    }
    nfcResult?.let { message ->
        AlertDialog(
            onDismissRequest = onDismissResult,
            title = { Text(if (message.contains("Tag prêt")) "Tag NFC prêt" else "Résultat NFC") },
            text = { Text(message) },
            confirmButton = { Button(onClick = onDismissResult) { Text("Terminé") } },
        )
    }
    if (updateDialogVisible && availableUpdate != null) {
        AppUpdateDialog(
            update = availableUpdate,
            downloading = updateDownloading,
            progress = updateProgress,
            error = updateError,
            onInstall = onInstallUpdate,
            onDismiss = onDismissUpdate,
        )
    }
}

@Composable
private fun NfcProgressContent(playlist: CloudPlaylist, progress: NfcProgress) {
    val steps = listOf(
        "Vérification de l’état actuel",
        "Suppression des données présentes",
        "Écriture du tag",
        "Vérification du tag",
    )
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(
            "K${playlist.figureId} — ${playlist.name}",
            fontWeight = FontWeight.Bold,
        )
        steps.forEachIndexed { index, label ->
            val step = index + 1
            val isDone = progress.currentStep > step
            val isCurrent = progress.currentStep == step
            val marker = when {
                isDone -> "✓"
                isCurrent -> "●"
                else -> "○"
            }
            val color = when {
                isDone || isCurrent -> MaterialTheme.colorScheme.primary
                else -> MaterialTheme.colorScheme.onSurfaceVariant
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(marker, color = color, fontWeight = FontWeight.Bold, modifier = Modifier.padding(end = 10.dp))
                Text(label, color = color, fontWeight = if (isCurrent) FontWeight.Bold else FontWeight.Normal)
            }
        }
        Text(progress.message, color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp)
    }
}

@Composable
private fun AuthScreen(
    loading: Boolean,
    statusMessage: String?,
    updateChecking: Boolean,
    availableUpdate: AppUpdate?,
    onAuthenticate: (Boolean, String, String, String) -> Unit,
    onDismissStatus: () -> Unit,
    onCheckUpdate: () -> Unit,
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
            TextButton(onClick = onCheckUpdate, enabled = !updateChecking, modifier = Modifier.align(Alignment.CenterHorizontally)) {
                Text(if (availableUpdate != null) "Mise à jour ${availableUpdate.version} disponible" else "Version ${BuildConfig.VERSION_NAME} · Rechercher une mise à jour")
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
    updateChecking: Boolean,
    availableUpdate: AppUpdate?,
    onRefresh: () -> Unit,
    onLogout: () -> Unit,
    onImport: (CloudPlaylist?) -> Unit,
    onEdit: (CloudPlaylist) -> Unit,
    onRename: (CloudPlaylist) -> Unit,
    onDelete: (CloudPlaylist) -> Unit,
    onArmNfc: (CloudPlaylist) -> Unit,
    onDismissStatus: () -> Unit,
    onCheckUpdate: () -> Unit,
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
                    Column(horizontalAlignment = Alignment.End) {
                        TextButton(onClick = onRefresh, enabled = !loading) { Text("Actualiser") }
                        TextButton(onClick = onCheckUpdate, enabled = !updateChecking && !loading) {
                            Text(if (availableUpdate != null) "MàJ ${availableUpdate.version}" else "Mises à jour", fontSize = 11.sp)
                        }
                    }
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
                        PlaylistCard(playlist, loading, onEdit, onRename, onDelete, onArmNfc)
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
    onEdit: (CloudPlaylist) -> Unit,
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
                OutlinedButton(onClick = { onEdit(playlist) }, enabled = !loading) { Text("Modifier") }
                TextButton(onClick = { onDelete(playlist) }, enabled = !loading) { Text("Supprimer", color = MaterialTheme.colorScheme.error) }
            }
        }
    }
}

@Composable
private fun AppUpdateDialog(
    update: AppUpdate,
    downloading: Boolean,
    progress: Int?,
    error: String?,
    onInstall: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = { if (!downloading) onDismiss() },
        icon = {
            Box(Modifier.size(54.dp).background(Color(0xFFEAF8F1), CircleShape), contentAlignment = Alignment.Center) {
                Text("↓", color = Color(0xFF247A58), fontWeight = FontWeight.Black, fontSize = 26.sp)
            }
        },
        title = { Text("FABA Tag ${update.version} est disponible") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Surface(color = Color(0xFFF7F5FA), shape = RoundedCornerShape(12.dp)) {
                    Row(Modifier.fillMaxWidth().padding(12.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                        Column { Text("Installée", fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant); Text(BuildConfig.VERSION_NAME, fontWeight = FontWeight.Bold) }
                        Text("→", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
                        Column(horizontalAlignment = Alignment.End) { Text("Nouvelle", fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant); Text(update.version, fontWeight = FontWeight.Bold) }
                    }
                }
                Text("L'APK est téléchargé depuis la release GitHub officielle et son empreinte SHA-256 est vérifiée avant installation.", color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 13.sp)
                if (update.releaseNotes.isNotBlank()) {
                    Text(update.releaseNotes, modifier = Modifier.heightIn(max = 150.dp), fontSize = 12.sp, overflow = TextOverflow.Ellipsis)
                }
                if (downloading) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp)
                        Text(if (progress == null) "Téléchargement…" else "Téléchargement et vérification… $progress %", fontWeight = FontWeight.Bold, fontSize = 12.sp)
                    }
                }
                error?.let { Text(it, color = MaterialTheme.colorScheme.error, fontSize = 12.sp) }
            }
        },
        confirmButton = {
            Button(onClick = onInstall, enabled = !downloading) {
                Text(if (downloading) "Préparation…" else "Télécharger et installer")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !downloading) { Text("Plus tard") } },
    )
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
    onAddTracks: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = { if (!loading) onChange(null) },
        title = { Text(if (draft.editing) "Modifier la playlist" else "Nouvelle playlist") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(draft.name, { onChange(draft.copy(name = it)) }, label = { Text("Nom") }, singleLine = true)
                OutlinedTextField(draft.figureId, { if (!draft.editing) onChange(draft.copy(figureId = it.filter(Char::isDigit).take(4))) }, label = { Text("ID FABA+ (2000–8999)") }, enabled = !draft.editing, singleLine = true)
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
                    Text("${draft.tracks.size} piste(s) · ordre de lecture", color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp)
                    TextButton(onClick = onAddTracks, enabled = !loading && draft.tracks.size < 99) { Text("＋ Ajouter") }
                }
                LazyColumn(Modifier.heightIn(max = 300.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    itemsIndexed(draft.tracks) { index, track ->
                        Surface(color = Color(0xFFF7F5FA), shape = RoundedCornerShape(10.dp)) {
                            Row(Modifier.fillMaxWidth().padding(start = 10.dp, end = 2.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                                Text(String.format("%02d", index + 1), color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold, fontSize = 11.sp)
                                Text(track.label, Modifier.padding(horizontal = 8.dp).weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis, fontSize = 12.sp)
                                TextButton(
                                    onClick = { onChange(draft.copy(tracks = draft.tracks.moved(index, index - 1))) },
                                    enabled = !loading && index > 0,
                                    contentPadding = PaddingValues(6.dp),
                                ) { Text("↑") }
                                TextButton(
                                    onClick = { onChange(draft.copy(tracks = draft.tracks.moved(index, index + 1))) },
                                    enabled = !loading && index < draft.tracks.lastIndex,
                                    contentPadding = PaddingValues(6.dp),
                                ) { Text("↓") }
                                TextButton(
                                    onClick = { onChange(draft.copy(tracks = draft.tracks.filterIndexed { trackIndex, _ -> trackIndex != index })) },
                                    enabled = !loading,
                                    contentPadding = PaddingValues(6.dp),
                                ) { Text("×", color = MaterialTheme.colorScheme.error) }
                            }
                        }
                    }
                }
            }
        },
        confirmButton = { Button(onClick = { onConfirm(draft) }, enabled = !loading && draft.tracks.isNotEmpty()) { Text(if (loading) "Envoi…" else "Enregistrer") } },
        dismissButton = { TextButton(onClick = { onChange(null) }, enabled = !loading) { Text("Annuler") } },
    )
}

private fun <T> List<T>.moved(from: Int, to: Int): List<T> {
    if (from !in indices || to !in indices || from == to) return this
    return toMutableList().also { values ->
        val value = values.removeAt(from)
        values.add(to, value)
    }
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
