# Publication et mises à jour

Les applications desktop et Android utilisent la dernière release publique du dépôt GitHub `EdwinBeone/faba-plus-custom-editor` comme canal stable unique.

## Desktop

Le plugin updater de Tauri lit :

`https://github.com/EdwinBeone/faba-plus-custom-editor/releases/latest/download/latest.json`

La CI crée les paquets d'update et leurs signatures. La clé publique est intégrée à `src-tauri/tauri.conf.json`. La clé privée ne doit jamais être ajoutée au dépôt : elle est enregistrée dans le secret GitHub Actions `TAURI_SIGNING_PRIVATE_KEY` et sa copie locale protégée se trouve dans le répertoire de données personnel du mainteneur.

Perdre cette clé privée empêcherait de publier une mise à jour acceptée par les versions déjà installées. Sa rotation exige donc une release intermédiaire contenant les deux chemins de confiance.

## Android

L'application interroge l'API `releases/latest`, sélectionne exclusivement l'asset `FABA-Tag-Android.apk`, puis exige une empreinte GitHub `sha256:` valide. Après téléchargement, la taille et l'empreinte du fichier sont contrôlées avant l'ouverture de l'installateur système.

Toutes les releases Android doivent continuer à être signées avec le même keystore. Android refusera sinon de mettre à jour l'application déjà installée.

## Nouvelle release

1. Incrémenter la version dans `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` et `android/app/build.gradle.kts`. Incrémenter aussi `versionCode` Android.
2. Ajouter les changements dans `CHANGELOG.md` et exécuter tous les contrôles indiqués dans le README.
3. Pousser le commit puis le tag `vX.Y.Z`.
4. Attendre la fin du workflow **Build installers**.
5. Vérifier que la release contient `latest.json`, les signatures desktop, les installateurs habituels et `FABA-Tag-Android.apk` avec une empreinte SHA-256.
