# Journal des versions

## 0.1.3

- correction du format audio FABA+ validé sur matériel : `00.faba`, `01.faba`, etc. ;
- suppression des anciennes métadonnées ID3 et ajout du titre `KxxxxCPyy` attendu ;
- refus des plages d'identifiants `0xxx`, `1xxx` et `9xxx`, réservées au contenu officiel ou aux tests ;
- lecture compatible avec les anciens noms `CPxx.faba` afin de pouvoir diagnostiquer et retirer les dossiers créés par les versions précédentes.

## 0.1.2

- détection prioritaire du dossier `PLAYER` d'une carte FABA+, même s'il ne contient encore que `KTEST` ;
- écriture des nouvelles figurines dans `PLAYER/Kxxxx` au lieu de la racine de la carte ;
- avertissement lorsqu'un dossier `Kxxxx` mal placé est détecté à la racine.

## 0.1.1

- correction de l'écriture d'une nouvelle figurine sous Windows : le fichier `info` est désormais fermé avant le renommage atomique du dossier ;
- ajout d'un écran **Diagnostic technique** avec journal persistant, copie, actualisation et effacement ;
- affichage de la chaîne d'erreur système complète ;
- ajout des tests Rust sur Windows dans la CI ;
- clarification de l'identifiant à quatre chiffres et de son lien avec un tag NFC vierge.

## 0.1.0

- première version publique pour Windows, macOS et Linux.
