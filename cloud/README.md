# FABA Cloud

API privée de synchronisation complète de la bibliothèque FABA+. Elle conserve les métadonnées dans PostgreSQL et les MP3 personnels dans un volume Docker dédié.

## Démarrage

```bash
./scripts/bootstrap.sh
curl http://127.0.0.1:8787/health
```

Le service écoute uniquement sur `127.0.0.1:8787`. Le fichier `deploy/apache/faba.bo1.be.conf` publie l'API derrière Apache. Sur Debian, activez `proxy`, `proxy_http` et `headers`, puis laissez Certbot créer le vhost HTTPS :

```bash
sudo a2enmod proxy proxy_http headers ssl
sudo a2ensite faba.bo1.be.conf
sudo apache2ctl configtest
sudo systemctl reload apache2
sudo certbot --apache -d faba.bo1.be --redirect
```

## API

- `POST /api/v1/auth/register`
- `POST /api/v1/auth/login`
- `POST /api/v1/auth/logout`
- `GET /api/v1/me`
- `GET /api/v1/library`
- `POST /api/v1/library/sync`
- `PUT|DELETE /api/v1/library/playlists/{figureId}`
- `PUT|GET /api/v1/library/playlists/{figureId}/tracks/{position}/audio`

Toutes les routes hors inscription, connexion et santé attendent `Authorization: Bearer fab_live_…`.

Les identifiants personnalisés sont limités à `2000–8999`. Les valeurs par défaut autorisent 200 Mo par piste, 5 Go par compte et 50 Go au total ; elles sont ajustables avec `MAX_TRACK_BYTES`, `MAX_ACCOUNT_BYTES` et `MAX_TOTAL_BYTES`.

## Sauvegarde

`./scripts/backup.sh` crée un dump PostgreSQL et une archive du volume audio dans `cloud/backups`. Les deux fichiers portant le même horodatage forment une sauvegarde cohérente à conserver ensemble.
