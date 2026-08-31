# Architecture

## Vue d'ensemble

FABA+ Custom Editor sépare l'interface, les opérations de carte, la base locale et la bibliothèque cloud.

```text
React / TypeScript
        │ commandes IPC Tauri
        ▼
Rust ── scan, validation, écriture atomique, sauvegardes
  │
  ├── carte microSD : PLAYER/Kxxxx/00.faba + info
  ├── SQLite local : cartes, noms de figurines, noms de pistes, session cloud
  └── HTTPS ── Axum ── PostgreSQL + volume audio privé

Android / Jetpack Compose
  ├── HTTPS ── même compte et même bibliothèque
  └── Android NFC ── écriture et vérification NDEF
```

Le frontend ne possède pas d'accès général en écriture au système de fichiers. Il transmet les chemins choisis dans des boîtes de dialogue natives aux commandes Rust, qui valident les identifiants, extensions et limites avant toute mutation.

## Modèle FABA+

Pour l'identifiant personnalisé `3101`, l'application produit :

```text
PLAYER/
└── K3101/
    ├── 00.faba
    ├── 01.faba
    └── info
```

Les fichiers `.faba` conservent le flux audio MP3. L'application remplace leurs métadonnées par un unique titre ID3v2.3 UTF-16 (`K3101CP01`, `K3101CP02`, etc.), puis écrit le fichier `info` :

```json
{"totalTracks":2,"characterDir":"02190530310100"}
```

## Garanties d'écriture

Lors d'un remplacement :

1. le dossier existant est copié dans le répertoire local `backups` ;
2. les nouveaux fichiers sont écrits dans un dossier temporaire sur la carte ;
3. l'ancien dossier est renommé sans être supprimé ;
4. le dossier temporaire prend le nom final ;
5. l'ancienne copie sur la carte est supprimée uniquement après la réussite du renommage ;
6. si le renommage final échoue, l'ancien dossier reprend immédiatement son nom.

Cette stratégie évite les états partiellement écrits visibles par la FABA+. Une panne physique de la carte peut néanmoins toujours causer une perte de données, d'où la sauvegarde préalable.

## Base locale

SQLite stocke uniquement :

- les chemins et dates des cartes rencontrées ;
- le type de carte détecté ;
- les noms locaux donnés aux figurines ;
- les libellés des pistes importées.

Le mode WAL protège l'intégrité de l'index en cas d'arrêt brutal. Sans connexion cloud, les fichiers audio restent exclusivement sur la carte.

## Bibliothèque cloud

Le cloud conserve les métadonnées dans PostgreSQL et les MP3 dans un volume Docker distinct. Chaque piste possède sa taille et son empreinte SHA-256. Le desktop compare cette empreinte avant un envoi et la revérifie après un téléchargement. Les suppressions sont explicites : synchroniser une carte incomplète ne supprime pas les playlists créées depuis Android.

Les sessions sont des jetons opaques ; seul leur SHA-256 est conservé côté serveur. Les mots de passe utilisent Argon2 et le nombre de calculs simultanés est borné. L'API limite les corps JSON, les pistes à 200 Mo, chaque compte à 5 Go et le service à 50 Go par défaut. Le conteneur API s'exécute sans privilèges, avec un système de fichiers racine en lecture seule ; seul le volume audio est inscriptible.

## Flux Android

Android sélectionne des documents `audio/mpeg` via le sélecteur système, crée ou met à jour la playlist par API, puis envoie chaque piste. Le jeton local est chiffré par une clé AES-GCM non exportable d'Android Keystore. Pour le NFC, l'application n'accepte que les payloads correspondant exactement à l'ID `2000–8999`, écrit un enregistrement texte NDEF et le relit avant d'annoncer le succès.

## Limites actuelles

- L'ancien format FABA requiert la modification d'étiquettes ID3 et une transformation binaire. Il est scanné mais jamais modifié dans la version 0.2.
- L'écriture NFC directe est disponible sur Android ; le desktop continue d'afficher et de copier le payload.
- Les installateurs de développement communautaire ne sont pas signés.
- L'API accepte uniquement des MP3 ; les autres formats audio ne sont pas encore transcodés automatiquement.
