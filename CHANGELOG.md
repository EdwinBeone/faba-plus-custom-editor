# Journal des versions

## 0.5.1

- remplacement du verrou NFC Android par une machine d'état atomique propre à chaque appui sur **Écrire le tag NFC** ;
- ajout du parcours explicite en quatre étapes : inspection, suppression des anciennes données, écriture, puis relecture de vérification ;
- affichage en direct de l'étape NFC en cours et ajout de journaux techniques `FabaNfc` dans Logcat ;
- arrêt définitif du lecteur NFC dès le résultat obtenu : les callbacks répétés ou provenant d'une ancienne session sont ignorés jusqu'à la fermeture du résultat et un nouvel appui sur le bouton ;
- prise en charge de la vérification après formatage d'un tag NDEF vierge au moyen d'une unique redétection contrôlée.

## 0.5.0

- ajout de l'auto-update signé Tauri sur Windows, macOS et Linux, directement depuis la dernière release GitHub ;
- vérification automatique au démarrage, recherche manuelle, notes de version, progression de téléchargement et redémarrage après installation sur PC ;
- ajout de la recherche de mise à jour Android depuis GitHub avec téléchargement de l'APK signé existant ;
- contrôle strict du nom de l'asset, de l'URL de release, de la taille et de l'empreinte SHA-256 de l'APK avant de demander son installation ;
- parcours guidé vers l'autorisation Android « installer des applications inconnues », nécessaire uniquement la première fois ;
- génération et publication automatiques de `latest.json` et des signatures d'update desktop dans chaque release.

## 0.4.0

- correction de l'écriture NFC Android : le lecteur reste actif jusqu'à la fin de l'écriture et de la vérification, tout en refusant les callbacks NFC répétés ;
- ajout d'un éditeur complet de playlist sur Android avec ajout de MP3, retrait de pistes et réordonnancement ;
- ajout d'un éditeur complet sur PC avec ajout multiple, réordonnancement et conservation des vrais titres des pistes ;
- glisser-déposer direct de MP3 sur une playlist existante pour les ajouter à celle-ci ;
- nouvelle icône originale et cohérente pour Android, Windows, macOS et Linux, également affichée dans l'interface desktop.

## 0.3.0

- la bibliothèque locale devient l'espace de travail principal, même sans carte SD et sans connexion cloud ;
- ajout, renommage, remplacement, suppression et écoute des playlists directement depuis le PC ;
- cache audio complet avec synchronisation cloud différée lorsque le serveur est indisponible ;
- import multiple par sélection ou glisser-déposer, avec une playlist par MP3 ou une playlist regroupée ;
- génération automatique d'identifiants personnalisés libres entre `2000` et `8999` ;
- ajout du bouton **Synchroniser la carte** : les IDs identiques sont remplacés avec sauvegarde, sans supprimer les autres contenus présents sur la SD ;
- correction des marges système Android pour ne plus passer sous la barre d'état ;
- arrêt de la session d'écriture NFC Android après une seule détection, afin d'éviter les écritures et messages de succès en boucle.

## 0.2.0

- ajout de FABA Cloud avec comptes, sessions révocables et bibliothèque complète synchronisée ;
- stockage des MP3 avec quota, limite par piste et vérification SHA-256 ;
- fusion non destructive entre les cartes SD et la bibliothèque centrale ;
- import d'une playlist cloud complète vers la carte via l'application desktop ;
- ajout de l'application Android native pour importer, renommer, remplacer ou supprimer des playlists ;
- écriture NFC Android en un clic, limitée aux IDs `2000–8999`, avec vérification NDEF après écriture ;
- ajout du déploiement Docker/PostgreSQL, du vhost Apache et des builds Android dans la CI.

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
