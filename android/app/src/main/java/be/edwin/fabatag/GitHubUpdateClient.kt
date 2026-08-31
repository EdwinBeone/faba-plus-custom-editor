package be.edwin.fabatag

import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

data class AppUpdate(
    val version: String,
    val releaseNotes: String,
    val downloadUrl: String,
    val sha256: String,
    val sizeBytes: Long,
)

class GitHubUpdateClient(
    private val currentVersion: String = BuildConfig.VERSION_NAME,
    private val releasesApiUrl: String = "https://api.github.com/repos/EdwinBeone/faba-plus-custom-editor/releases/latest",
) {
    fun check(): AppUpdate? {
        val release = requestJson(releasesApiUrl)
        val version = release.getString("tag_name").removePrefix("v")
        if (compareVersions(version, currentVersion) <= 0) return null

        val assets = release.getJSONArray("assets")
        val asset = (0 until assets.length())
            .asSequence()
            .map(assets::getJSONObject)
            .firstOrNull { it.getString("name") == APK_ASSET_NAME }
            ?: throw IOException("La release GitHub ne contient pas l'APK Android attendu.")

        val downloadUrl = asset.getString("browser_download_url")
        if (!downloadUrl.startsWith(RELEASE_DOWNLOAD_PREFIX)) {
            throw IOException("L'adresse de téléchargement de la mise à jour est invalide.")
        }
        val digest = asset.optString("digest")
        if (!digest.matches(Regex("^sha256:[0-9a-fA-F]{64}$"))) {
            throw IOException("La release GitHub ne fournit pas d'empreinte SHA-256 valide.")
        }
        return AppUpdate(
            version = version,
            releaseNotes = release.optString("body"),
            downloadUrl = downloadUrl,
            sha256 = digest.substringAfter(':').lowercase(),
            sizeBytes = asset.optLong("size"),
        )
    }

    fun download(update: AppUpdate, destination: File, onProgress: (Int?) -> Unit) {
        destination.parentFile?.mkdirs()
        val partial = File(destination.parentFile, "${destination.name}.part")
        partial.delete()
        val connection = URL(update.downloadUrl).openConnection() as HttpURLConnection
        try {
            connection.instanceFollowRedirects = true
            connection.connectTimeout = 15_000
            connection.readTimeout = 10 * 60_000
            connection.setRequestProperty("Accept", "application/vnd.android.package-archive")
            connection.setRequestProperty("User-Agent", "FABA-Tag-Android/${BuildConfig.VERSION_NAME}")
            val status = connection.responseCode
            if (status !in 200..299) throw IOException("GitHub a refusé le téléchargement de l'APK ($status).")
            val total = connection.contentLengthLong.takeIf { it > 0 } ?: update.sizeBytes.takeIf { it > 0 }
            val digest = MessageDigest.getInstance("SHA-256")
            var downloaded = 0L
            var lastProgress = -2
            connection.inputStream.buffered().use { input ->
                partial.outputStream().buffered().use { output ->
                    val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        output.write(buffer, 0, count)
                        digest.update(buffer, 0, count)
                        downloaded += count
                        val progress = total?.let { ((downloaded * 100L) / it).coerceIn(0, 99).toInt() }
                        val progressKey = progress ?: -1
                        if (progressKey != lastProgress) {
                            lastProgress = progressKey
                            onProgress(progress)
                        }
                    }
                }
            }
            val actualSha256 = digest.digest().joinToString("") { "%02x".format(it) }
            if (actualSha256 != update.sha256) {
                throw IOException("L'empreinte de l'APK ne correspond pas à la release GitHub. Installation annulée.")
            }
            if (update.sizeBytes > 0 && downloaded != update.sizeBytes) {
                throw IOException("Le téléchargement de l'APK est incomplet. Installation annulée.")
            }
            destination.delete()
            if (!partial.renameTo(destination)) {
                partial.copyTo(destination, overwrite = true)
                partial.delete()
            }
            onProgress(100)
        } catch (error: Exception) {
            partial.delete()
            if (error is IOException) throw error
            throw IOException("Téléchargement de la mise à jour Android impossible.", error)
        } finally {
            connection.disconnect()
        }
    }

    private fun requestJson(url: String): JSONObject {
        val connection = URL(url).openConnection() as HttpURLConnection
        return try {
            connection.connectTimeout = 10_000
            connection.readTimeout = 15_000
            connection.setRequestProperty("Accept", "application/vnd.github+json")
            connection.setRequestProperty("X-GitHub-Api-Version", "2022-11-28")
            connection.setRequestProperty("User-Agent", "FABA-Tag-Android/$currentVersion")
            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val text = stream?.bufferedReader(Charsets.UTF_8)?.use { it.readText() }.orEmpty()
            if (status !in 200..299) throw IOException("GitHub n'a pas pu vérifier la dernière version ($status).")
            JSONObject(text)
        } catch (error: Exception) {
            if (error is IOException) throw error
            throw IOException("Vérification de la mise à jour Android impossible.", error)
        } finally {
            connection.disconnect()
        }
    }

    companion object {
        const val APK_ASSET_NAME = "FABA-Tag-Android.apk"
        private const val RELEASE_DOWNLOAD_PREFIX =
            "https://github.com/EdwinBeone/faba-plus-custom-editor/releases/download/"

        internal fun compareVersions(left: String, right: String): Int {
            val leftParts = parseVersion(left)
            val rightParts = parseVersion(right)
            for (index in leftParts.indices) {
                val comparison = leftParts[index].compareTo(rightParts[index])
                if (comparison != 0) return comparison
            }
            return 0
        }

        private fun parseVersion(value: String): List<Int> {
            val match = Regex("^v?(\\d+)\\.(\\d+)\\.(\\d+)$").matchEntire(value)
                ?: throw IOException("Version GitHub invalide : $value")
            return match.groupValues.drop(1).map(String::toInt)
        }
    }
}
