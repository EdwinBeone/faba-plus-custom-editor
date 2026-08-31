# Contribuer

Merci de contribuer à FABA+ Custom Editor.

## Avant une pull request

1. Ouvrez une issue pour les changements importants ou liés au format de carte.
2. Travaillez sur une branche courte.
3. N'ajoutez aucun son commercial, dump de carte ou identifiant personnel.
4. Ajoutez un test Rust pour toute modification du scan ou de l'écriture.
5. Exécutez les contrôles suivants :

```bash
npm ci
npm run check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path cloud/Cargo.toml -- --check
cargo test --manifest-path cloud/Cargo.toml
cd android && ./gradlew testDebugUnitTest assembleDebug
```

Les changements de format doivent préserver les trois garanties fondamentales : validation stricte, sauvegarde avant mutation et restauration de l'état précédent en cas d'échec.

## Signaler un problème

Indiquez le système d'exploitation, la version de l'application, le type de FABA et une arborescence anonymisée de la carte. Ne joignez jamais les pistes audio originales.
