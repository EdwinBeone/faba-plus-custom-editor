# FABA+ Custom Editor

Une application de bureau moderne pour gérer simplement **vos propres sons** sur la carte microSD d'une FABA+.

![Écran d'accueil de FABA+ Custom Editor](docs/screenshots/welcome.png)

[![CI](https://github.com/EdwinBeone/faba-plus-custom-editor/actions/workflows/ci.yml/badge.svg)](https://github.com/EdwinBeone/faba-plus-custom-editor/actions/workflows/ci.yml)
[![GPL-3.0](https://img.shields.io/badge/licence-GPL--3.0-blue.svg)](LICENSE)

> [!WARNING]
> FABA+ est un appareil connecté. Le fabricant peut potentiellement détecter du contenu ou des tags non officiels et bloquer l'appareil. Utilisez cet outil à vos risques, uniquement avec des contenus que vous avez le droit d'utiliser. Ce projet n'est pas affilié à FABA.

## Pourquoi ce projet ?

Les excellentes recherches de [`wansors/myfaba-hacks`](https://github.com/wansors/myfaba-hacks) rendent la personnalisation possible, mais l'utilisation actuelle demande des scripts, une structure de dossiers précise et plusieurs opérations manuelles. FABA+ Custom Editor regroupe ce travail dans une interface claire pour Windows, macOS et Linux.

## Fonctionnalités

- détection automatique des cartes et supports amovibles ;
- sélection manuelle d'un dossier en solution de secours ;
- détection du dossier FABA+ `PLAYER`, puis scan non destructif des dossiers `Kxxxx` et du fichier `info` ;
- bibliothèque locale complète, utilisable sans carte SD et hors ligne ;
- ajout, renommage, remplacement, suppression et écoute des playlists depuis le PC ;
- import multiple par bouton ou glisser-déposer, avec création automatique des playlists et IDs ;
- compte FABA Cloud partagé entre le PC et Android ;
- synchronisation complète des playlists et de leurs MP3, avec contrôle d'intégrité SHA-256 ;
- synchronisation de toute la bibliothèque vers une carte SD en un clic, sans supprimer les contenus supplémentaires déjà présents ;
- application Android installable hors store : import de MP3, gestion de la bibliothèque et écriture NFC vérifiée ;
- ajout ou remplacement de 1 à 99 fichiers MP3, dans l'ordre choisi ;
- génération automatique de `00.faba`, `01.faba`, etc., avec titres ID3 FABA+ et fichier `info` ;
- sauvegarde locale automatique avant chaque remplacement ou suppression ;
- remplacement atomique : l'ancien dossier est restauré si l'écriture échoue ;
- lecture des pistes directement depuis la carte ;
- copie du code NFC `02190530XXXX00` ;
- export d'une figurine vers un dossier choisi ;
- journal technique local consultable, copiable et effaçable depuis l'application ;
- détection de l'ancien format FABA en lecture seule.

## Installation

Téléchargez la dernière version dans la page [Releases](https://github.com/EdwinBeone/faba-plus-custom-editor/releases).

| Système | Fichier conseillé |
| --- | --- |
| Windows 10/11 | installateur `.exe` (NSIS) ou `.msi` |
| macOS Intel et Apple Silicon | image disque universelle `.dmg` |
| Linux | `.AppImage`, `.deb` ou `.rpm` |

Les versions initiales ne sont pas signées. Windows SmartScreen ou macOS Gatekeeper peuvent donc afficher un avertissement. La signature de code fait partie de la feuille de route.

## Utilisation

1. Ouvrez FABA+ Custom Editor et cliquez sur **Importer des MP3**. Une carte SD n'est pas nécessaire.
2. Choisissez une playlist par fichier ou une playlist qui regroupe toute la sélection. Les IDs libres sont attribués automatiquement.
3. Organisez la bibliothèque sur le PC : renommez, remplacez, supprimez ou écoutez les pistes. En cas de panne réseau, les changements restent dans le cache local.
4. Éteignez la FABA+, retirez sa carte microSD puis insérez-la dans l'ordinateur.
5. Ouvrez la carte détectée et cliquez sur **Synchroniser la carte**. Un même ID est sauvegardé puis remplacé ; les autres dossiers de la SD restent intacts.
6. Éjectez proprement la carte avant de la remettre dans la FABA+.

Pour associer un tag, choisissez un identifiant personnalisé entre `2000` et `8999`, ajoutez d'abord son contenu sur la carte, puis écrivez un enregistrement texte NDEF contenant `02190530XXXX00` sur un tag NFC vierge. Les plages `0xxx`, `1xxx` et `9xxx` sont réservées par FABA+. La compatibilité des tags et les risques spécifiques sont détaillés dans la [FAQ du projet d'origine](https://github.com/wansors/myfaba-hacks/blob/main/FAQ.md).

### FABA Cloud et Android

1. Dans l'application PC, ouvrez **FABA Cloud** et créez un compte : la bibliothèque locale et ses MP3 sont envoyés dans votre bibliothèque privée.
2. Installez `FABA-Tag-Android.apk` depuis la page Releases et connectez le même compte.
3. Sur Android, importez des MP3 ou choisissez une playlist existante, touchez **Écrire le tag NFC**, puis approchez un tag NDEF compatible.
4. Une playlist créée sur Android apparaît dans le cache du PC. Ouvrez une carte et cliquez sur **Synchroniser la carte** pour produire tous les fichiers FABA+ manquants ou mis à jour.

La carte SD reste une cible explicite : la synchronisation écrase uniquement les IDs présents dans la bibliothèque et ne supprime jamais les autres playlists de la carte.

## Sécurité des données

Sans compte, toutes les opérations restent locales. Lorsque FABA Cloud est activé, les noms, pistes et MP3 personnels sont synchronisés via HTTPS sur `faba.bo1.be`. Le mot de passe est haché côté serveur avec Argon2 et n'est jamais conservé par les applications. Android chiffre le jeton de session avec Android Keystore ; le PC conserve un jeton révocable dans sa base locale. Le service applique une limite par piste et un quota par compte.

Avant de remplacer ou retirer une figurine, son dossier complet est copié dans le répertoire de données de l'application :

- Windows : `%APPDATA%\\be.edwin.fabapluscustomeditor\\backups`
- macOS : `~/Library/Application Support/be.edwin.fabapluscustomeditor/backups`
- Linux : `~/.local/share/be.edwin.fabapluscustomeditor/backups`

Une sauvegarde ne remplace pas une copie complète de la carte microSD avant la première utilisation.

## Diagnostic

Le bouton **Diagnostic technique** ouvre le journal local de l'application. Il indique les étapes d'écriture, le chemin de la carte, l'identifiant de figurine et la cause système complète d'une erreur. Il n'enregistre ni le contenu des fichiers audio ni leurs données binaires.

Le journal peut être actualisé, copié pour un rapport de bug ou effacé directement depuis cette fenêtre.

## Développement

Le projet utilise [Tauri 2](https://v2.tauri.app/), React, TypeScript, Rust et SQLite pour le desktop, Axum/PostgreSQL pour le cloud, et Kotlin/Jetpack Compose pour Android.

```bash
npm install
npm run tauri dev
```

Sous Linux, installez d'abord les [prérequis Tauri](https://v2.tauri.app/start/prerequisites/) (notamment WebKitGTK 4.1). Les contrôles utilisés par la CI sont :

```bash
npm run check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path cloud/Cargo.toml
cd android && ./gradlew testDebugUnitTest assembleDebug
```

Consultez [ARCHITECTURE.md](docs/ARCHITECTURE.md) pour le modèle de données et les garanties d'écriture, puis [CONTRIBUTING.md](CONTRIBUTING.md) avant de proposer une contribution.

## État et feuille de route

La version `0.3.x` cible FABA+, traite l'ancien FABA en lecture seule et fournit une bibliothèque locale/cloud complète avec synchronisation SD additive et application Android NFC. Les prochaines étapes envisagées sont la restauration guidée des sauvegardes et davantage de validation sur différents modèles de tags et téléphones.

## Crédits et licence

Ce projet est basé sur les découvertes et le format documentés par [`wansors/myfaba-hacks`](https://github.com/wansors/myfaba-hacks). Voir [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

FABA+ Custom Editor est distribué sous licence [GNU GPL v3](LICENSE). Il ne contient ni ne distribue aucun contenu audio officiel FABA.
