package be.edwin.fabatag

import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.io.InputStream

class ApiClient(private val baseUrl: String = BuildConfig.FABA_API_BASE_URL) {
    fun login(email: String, password: String): AccountSession = authenticate(
        "auth/login",
        JSONObject()
            .put("email", email.trim())
            .put("password", password)
            .put("clientName", "FABA Tag Android"),
    )

    fun register(email: String, password: String, displayName: String): AccountSession = authenticate(
        "auth/register",
        JSONObject()
            .put("email", email.trim())
            .put("password", password)
            .put("displayName", displayName.trim())
            .put("clientName", "FABA Tag Android"),
    )

    fun library(token: String): CloudLibrary {
        val body = request("GET", "library", token = token)
        return parseLibrary(body)
    }

    fun savePlaylist(
        token: String,
        figureId: String,
        name: String,
        labels: List<String>,
        resetAudio: Boolean,
    ): CloudLibrary {
        val tracks = org.json.JSONArray()
        labels.forEachIndexed { position, label ->
            tracks.put(JSONObject().put("position", position).put("label", label))
        }
        val payload = JSONObject()
            .put("figureId", figureId)
            .put("name", name.trim())
            .put("tracks", tracks)
            .put("resetAudio", resetAudio)
        return parseLibrary(request("PUT", "library/playlists/$figureId", payload, token))
    }

    fun deletePlaylist(token: String, figureId: String): CloudLibrary =
        parseLibrary(request("DELETE", "library/playlists/$figureId", token = token))

    fun uploadAudio(
        token: String,
        figureId: String,
        position: Int,
        source: InputStream,
    ) {
        val connection = URL(baseUrl + "library/playlists/$figureId/tracks/$position/audio")
            .openConnection() as HttpURLConnection
        try {
            connection.requestMethod = "PUT"
            connection.connectTimeout = 10_000
            connection.readTimeout = 10 * 60_000
            connection.doOutput = true
            connection.setChunkedStreamingMode(64 * 1024)
            connection.setRequestProperty("Accept", "application/json")
            connection.setRequestProperty("Authorization", "Bearer $token")
            connection.setRequestProperty("Content-Type", "audio/mpeg")
            connection.outputStream.use { output -> source.use { it.copyTo(output, 64 * 1024) } }
            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val text = stream?.bufferedReader(Charsets.UTF_8)?.use { it.readText() }.orEmpty()
            if (status !in 200..299) throw ApiException(status, errorMessage(status, text))
        } catch (error: ApiException) {
            throw error
        } catch (error: Exception) {
            throw IOException("Envoi du MP3 vers FABA Cloud impossible.", error)
        } finally {
            connection.disconnect()
        }
    }

    private fun parseLibrary(body: JSONObject): CloudLibrary {
        val playlistsJson = body.getJSONArray("playlists")
        val playlists = buildList {
            for (index in 0 until playlistsJson.length()) {
                val playlist = playlistsJson.getJSONObject(index)
                val tracksJson = playlist.getJSONArray("tracks")
                val tracks = buildList {
                    for (trackIndex in 0 until tracksJson.length()) {
                        val track = tracksJson.getJSONObject(trackIndex)
                        add(
                            CloudTrack(
                                position = track.getInt("position"),
                                label = track.getString("label"),
                                audioAvailable = track.optBoolean("audioAvailable"),
                                audioSizeBytes = track.optLong("audioSizeBytes").takeIf { track.has("audioSizeBytes") && !track.isNull("audioSizeBytes") },
                                audioSha256 = track.optString("audioSha256").takeIf { track.has("audioSha256") && !track.isNull("audioSha256") },
                            ),
                        )
                    }
                }
                add(
                    CloudPlaylist(
                        figureId = playlist.getString("figureId"),
                        name = playlist.getString("name"),
                        nfcPayload = playlist.getString("nfcPayload"),
                        trackCount = playlist.getInt("trackCount"),
                        tracks = tracks,
                    ),
                )
            }
        }
        return CloudLibrary(
            version = body.getLong("version"),
            playlists = playlists,
            storageUsedBytes = body.optLong("storageUsedBytes"),
            storageLimitBytes = body.optLong("storageLimitBytes"),
        )
    }

    fun logout(token: String) {
        request("POST", "auth/logout", token = token, allowEmpty = true)
    }

    private fun authenticate(path: String, payload: JSONObject): AccountSession {
        val body = request("POST", path, payload)
        val account = body.getJSONObject("account")
        return AccountSession(
            token = body.getString("token"),
            email = account.getString("email"),
            displayName = account.getString("displayName"),
        )
    }

    private fun request(
        method: String,
        path: String,
        payload: JSONObject? = null,
        token: String? = null,
        allowEmpty: Boolean = false,
    ): JSONObject {
        val connection = URL(baseUrl + path).openConnection() as HttpURLConnection
        return try {
            connection.requestMethod = method
            connection.connectTimeout = 10_000
            connection.readTimeout = 15_000
            connection.setRequestProperty("Accept", "application/json")
            connection.setRequestProperty("User-Agent", "FABA-Tag-Android/${BuildConfig.VERSION_NAME}")
            token?.let { connection.setRequestProperty("Authorization", "Bearer $it") }
            payload?.let {
                connection.doOutput = true
                connection.setRequestProperty("Content-Type", "application/json; charset=utf-8")
                connection.outputStream.use { output -> output.write(it.toString().toByteArray(Charsets.UTF_8)) }
            }
            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val text = stream?.bufferedReader(Charsets.UTF_8)?.use { it.readText() }.orEmpty()
            if (status !in 200..299) {
                throw ApiException(status, errorMessage(status, text))
            }
            if (text.isBlank() && allowEmpty) JSONObject() else JSONObject(text)
        } catch (error: ApiException) {
            throw error
        } catch (error: Exception) {
            throw IOException("Connexion à FABA Cloud impossible. Vérifiez Internet puis réessayez.", error)
        } finally {
            connection.disconnect()
        }
    }

    private fun errorMessage(status: Int, text: String): String = runCatching {
        JSONObject(text).getJSONObject("error").getString("message")
    }.getOrDefault("Le serveur FABA Cloud a répondu avec l'erreur $status.")
}

class ApiException(val status: Int, message: String) : IOException(message)
