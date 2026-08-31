package be.edwin.fabatag

data class AccountSession(
    val token: String,
    val email: String,
    val displayName: String,
)

data class CloudTrack(
    val position: Int,
    val label: String,
    val audioAvailable: Boolean,
    val audioSizeBytes: Long?,
    val audioSha256: String?,
)

data class CloudPlaylist(
    val figureId: String,
    val name: String,
    val nfcPayload: String,
    val trackCount: Int,
    val tracks: List<CloudTrack>,
)

data class CloudLibrary(
    val version: Long,
    val playlists: List<CloudPlaylist>,
    val storageUsedBytes: Long,
    val storageLimitBytes: Long,
)

object PayloadValidator {
    private val payloadPattern = Regex("^02190530([2-8][0-9]{3})00$")

    fun isValid(playlist: CloudPlaylist): Boolean {
        val match = payloadPattern.matchEntire(playlist.nfcPayload) ?: return false
        return playlist.figureId == match.groupValues[1] && playlist.trackCount > 0
    }
}
