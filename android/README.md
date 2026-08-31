# FABA Tag pour Android

Application Android native, distribuée hors store, qui utilise la bibliothèque de `faba.bo1.be`.

Fonctions :

- connexion ou création du même compte que sur le desktop ;
- import de 1 à 99 MP3 dans une nouvelle playlist ;
- remplacement des pistes, renommage et suppression ;
- choix automatique du premier ID libre entre `2000` et `8999` ;
- écriture d'un tag NFC NDEF en un clic et vérification immédiate du contenu.

Compilation locale :

```bash
./gradlew testDebugUnitTest assembleDebug
```

L'APK debug est créé dans `app/build/outputs/apk/debug/`. La publication GitHub utilise une clé de signature conservée uniquement dans les secrets Actions `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` et `ANDROID_STORE_PASSWORD`.

L'application ne modifie pas l'UID physique d'un tag et ne verrouille pas le tag en lecture seule. Elle accepte uniquement le payload calculé par le serveur pour un ID personnalisé valide.
