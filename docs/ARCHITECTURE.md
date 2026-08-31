# Architecture

## Vue d'ensemble

FABA+ Custom Editor sépare l'interface, les opérations de carte et les métadonnées locales.

```text
React / TypeScript
        │ commandes IPC Tauri
        ▼
Rust ── scan, validation, écriture atomique, sauvegardes
  │
  ├── carte microSD : PLAYER/Kxxxx/CPxx.faba + info
  └── SQLite local : cartes, noms de figurines, noms de pistes
```

Le frontend ne possède pas d'accès général en écriture au système de fichiers. Il transmet les chemins choisis dans des boîtes de dialogue natives aux commandes Rust, qui valident les identifiants, extensions et limites avant toute mutation.

## Modèle FABA+

Pour l'identifiant `0742`, l'application produit :

```text
PLAYER/
└── K0742/
    ├── CP00.faba
    ├── CP01.faba
    └── info
```

Les fichiers `.faba` sont les MP3 d'origine avec l'extension attendue par FABA+. Le fichier `info` contient :

```json
{"totalTracks":2,"characterDir":"02190530074200"}
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

Les fichiers audio restent sur la carte. Le mode WAL protège l'intégrité de l'index en cas d'arrêt brutal de l'application.

## Limites actuelles

- L'ancien format FABA requiert la modification d'étiquettes ID3 et une transformation binaire. Il est scanné mais jamais modifié dans la version 0.1.
- L'application n'écrit pas directement les tags NFC ; elle fournit le payload à copier dans une application NFC dédiée.
- Les installateurs de développement communautaire ne sont pas signés.
