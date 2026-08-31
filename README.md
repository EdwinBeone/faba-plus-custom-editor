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
- bibliothèque locale SQLite liée aux cartes déjà rencontrées ;
- noms de figurines et noms de pistes conservés localement ;
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

1. Éteignez la FABA+ et retirez sa carte microSD.
2. Insérez la carte dans l'ordinateur.
3. Ouvrez FABA+ Custom Editor et sélectionnez la carte détectée.
4. Cliquez sur **Ajouter une figurine**, choisissez un identifiant et vos MP3. L'application place la figurine dans `PLAYER/Kxxxx`.
5. Réordonnez les pistes, acceptez l'avertissement, puis enregistrez.
6. Éjectez proprement la carte avant de la remettre dans la FABA+.

Pour associer un tag, choisissez un identifiant personnalisé entre `2000` et `8999`, ajoutez d'abord son contenu sur la carte, puis écrivez un enregistrement texte NDEF contenant `02190530XXXX00` sur un tag NFC vierge. Les plages `0xxx`, `1xxx` et `9xxx` sont réservées par FABA+. La compatibilité des tags et les risques spécifiques sont détaillés dans la [FAQ du projet d'origine](https://github.com/wansors/myfaba-hacks/blob/main/FAQ.md).

## Sécurité des données

L'application fonctionne localement : aucun son et aucune donnée de bibliothèque ne sont envoyés sur Internet.

Avant de remplacer ou retirer une figurine, son dossier complet est copié dans le répertoire de données de l'application :

- Windows : `%APPDATA%\\be.edwin.fabapluscustomeditor\\backups`
- macOS : `~/Library/Application Support/be.edwin.fabapluscustomeditor/backups`
- Linux : `~/.local/share/be.edwin.fabapluscustomeditor/backups`

Une sauvegarde ne remplace pas une copie complète de la carte microSD avant la première utilisation.

## Diagnostic

Le bouton **Diagnostic technique** ouvre le journal local de l'application. Il indique les étapes d'écriture, le chemin de la carte, l'identifiant de figurine et la cause système complète d'une erreur. Il n'enregistre ni le contenu des fichiers audio ni leurs données binaires.

Le journal peut être actualisé, copié pour un rapport de bug ou effacé directement depuis cette fenêtre.

## Développement

Le projet utilise [Tauri 2](https://v2.tauri.app/), React, TypeScript, Rust et SQLite.

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
```

Consultez [ARCHITECTURE.md](docs/ARCHITECTURE.md) pour le modèle de données et les garanties d'écriture, puis [CONTRIBUTING.md](CONTRIBUTING.md) avant de proposer une contribution.

## État et feuille de route

La version `0.1.x` cible FABA+ et traite l'ancien FABA en lecture seule. Les prochaines étapes envisagées sont la restauration guidée des sauvegardes, la signature des installateurs, les mises à jour intégrées et, après davantage de validation sur matériel réel, l'édition sûre de l'ancien format chiffré.

## Crédits et licence

Ce projet est basé sur les découvertes et le format documentés par [`wansors/myfaba-hacks`](https://github.com/wansors/myfaba-hacks). Voir [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

FABA+ Custom Editor est distribué sous licence [GNU GPL v3](LICENSE). Il ne contient ni ne distribue aucun contenu audio officiel FABA.
