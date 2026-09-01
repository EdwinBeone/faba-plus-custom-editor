import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  Bug,
  Check,
  ChevronRight,
  CircleHelp,
  Cloud,
  CloudOff,
  Copy,
  Database,
  Disc3,
  Download,
  FolderOpen,
  HardDrive,
  Library,
  LoaderCircle,
  LogIn,
  LogOut,
  Music2,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  ShieldCheck,
  Smartphone,
  Sparkles,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import appIconUrl from "../assets/app-icon-ui.png";
import "./App.css";

type CardKind = "fabaPlus" | "legacyFaba" | "empty" | "unknown";

type Track = {
  index: number;
  fileName: string;
  path: string;
  label: string;
  sizeBytes: number;
};

type Figure = {
  id: string;
  folderName: string;
  customName: string | null;
  path: string;
  nfcPayload: string;
  tracks: Track[];
  modifiedAt: string | null;
  warning: string | null;
};

type CardSnapshot = {
  rootPath: string;
  kind: CardKind;
  writable: boolean;
  figures: Figure[];
  warnings: string[];
};

type DetectedCard = {
  id: string;
  label: string;
  mountPath: string;
  removable: boolean;
  likelyFaba: boolean;
  totalBytes: number;
  availableBytes: number;
};

type RecentCard = {
  rootPath: string;
  label: string;
  kind: string;
  lastSeenAt: string;
};

type MutationResult = {
  snapshot: CardSnapshot;
  backupPath: string | null;
  message: string;
};

type DiagnosticReport = {
  content: string;
  path: string;
};

type CloudStatus = {
  endpoint: string;
  authenticated: boolean;
  email: string | null;
  displayName: string | null;
  expiresAt: string | null;
  lastSyncAt: string | null;
};

type CloudTrack = {
  position: number;
  label: string;
  audioAvailable: boolean;
  audioSizeBytes: number;
  audioSha256: string;
  localPath: string | null;
};

type CloudPlaylist = {
  figureId: string;
  name: string;
  nfcPayload: string;
  trackCount: number;
  tracks: CloudTrack[];
  updatedAt: string;
  pendingSync: boolean;
};

type CloudLibrary = {
  version: number;
  playlists: CloudPlaylist[];
  storageUsedBytes: number;
  storageLimitBytes: number;
  offline: boolean;
  pendingChanges: number;
  lastError: string | null;
};

type BatchImport = { paths: string[] };
type PlaylistEdit = { figureId: string; addedPaths: string[] };

type Toast = { tone: "success" | "error" | "info"; message: string };
type DesktopUpdateInfo = {
  currentVersion: string;
  version: string;
  body?: string;
};
type DesktopUpdateState = "idle" | "checking" | "downloading" | "restarting";

const browserPreviewLibrary: CloudLibrary = {
  version: 8,
  playlists: [
    {
      figureId: "2000",
      name: "Histoires du soir",
      nfcPayload: "02190530200000",
      trackCount: 2,
      updatedAt: "2026-08-31T12:00:00Z",
      pendingSync: true,
      tracks: [
        { position: 0, label: "Le dragon", audioAvailable: false, audioSizeBytes: 0, audioSha256: "", localPath: null },
        { position: 1, label: "La forêt", audioAvailable: false, audioSizeBytes: 0, audioSha256: "", localPath: null },
      ],
    },
    {
      figureId: "2001",
      name: "Comptines",
      nfcPayload: "02190530200100",
      trackCount: 1,
      updatedAt: "2026-08-31T12:00:00Z",
      pendingSync: false,
      tracks: [
        { position: 0, label: "Une souris verte", audioAvailable: false, audioSizeBytes: 0, audioSha256: "", localPath: null },
      ],
    },
  ],
  storageUsedBytes: 42_000_000,
  storageLimitBytes: 2_000_000_000,
  offline: true,
  pendingChanges: 1,
  lastError: "Aperçu de l'état hors ligne",
};

const kindLabels: Record<CardKind, string> = {
  fabaPlus: "FABA+",
  legacyFaba: "FABA classique",
  empty: "Dossier vide",
  unknown: "Format inconnu",
};

function App() {
  const [devices, setDevices] = useState<DetectedCard[]>([]);
  const [recentCards, setRecentCards] = useState<RecentCard[]>([]);
  const [snapshot, setSnapshot] = useState<CardSnapshot | null>(null);
  const [selectedFigureId, setSelectedFigureId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [sourceBusy, setSourceBusy] = useState(true);
  const [toast, setToast] = useState<Toast | null>(null);
  const [editorFigure, setEditorFigure] = useState<Figure | "new" | null>(null);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [cloudOpen, setCloudOpen] = useState(false);
  const [cloudStatus, setCloudStatus] = useState<CloudStatus | null>(null);
  const [cloudLibrary, setCloudLibrary] = useState<CloudLibrary | null>(null);
  const [cloudBusy, setCloudBusy] = useState(false);
  const [batchImport, setBatchImport] = useState<BatchImport | null>(null);
  const [playlistEdit, setPlaylistEdit] = useState<PlaylistEdit | null>(null);
  const [draggingFiles, setDraggingFiles] = useState(false);
  const [dropPlaylistId, setDropPlaylistId] = useState<string | null>(null);
  const [desktopUpdate, setDesktopUpdate] = useState<DesktopUpdateInfo | null>(null);
  const [updateState, setUpdateState] = useState<DesktopUpdateState>("idle");
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  const updateRef = useRef<Update | null>(null);

  const refreshSources = async () => {
    setSourceBusy(true);
    try {
      const [detected, recent] = await Promise.all([
        invoke<DetectedCard[]>("detect_cards"),
        invoke<RecentCard[]>("recent_cards"),
      ]);
      setDevices(detected);
      setRecentCards(recent);
    } catch (error) {
      showToast("error", stringifyError(error));
    } finally {
      setSourceBusy(false);
    }
  };

  const refreshCloud = async (notify = false): Promise<boolean> => {
    setCloudBusy(true);
    try {
      const status = await invoke<CloudStatus>("cloud_status");
      setCloudStatus(status);
      const library = await invoke<CloudLibrary>(notify ? "cloud_sync" : "cloud_library");
      setCloudLibrary(library);
      setCloudStatus(await invoke<CloudStatus>("cloud_status"));
      if (notify) {
        showToast(
          library.offline ? "info" : "success",
          library.offline
            ? `Mode hors ligne : ${library.pendingChanges} modification(s) conservée(s) sur ce PC.`
            : "Bibliothèque locale et FABA Cloud synchronisés.",
        );
      }
      return true;
    } catch (error) {
      if (notify) showToast("error", stringifyError(error));
      return false;
    } finally {
      setCloudBusy(false);
    }
  };

  const checkDesktopUpdate = async (notify = false) => {
    if (!("__TAURI_INTERNALS__" in window) || updateState === "downloading" || updateState === "restarting") return;
    setUpdateState("checking");
    try {
      const update = await check({ timeout: 20_000 });
      if (updateRef.current && updateRef.current !== update) void updateRef.current.close();
      updateRef.current = update;
      if (update) {
        setDesktopUpdate({
          currentVersion: update.currentVersion,
          version: update.version,
          body: update.body,
        });
        setUpdateDismissed(false);
      } else {
        setDesktopUpdate(null);
        if (notify) showToast("success", "FABA+ Custom Editor est déjà à jour.");
      }
    } catch (error) {
      if (notify) showToast("error", `Vérification de la mise à jour impossible : ${stringifyError(error)}`);
    } finally {
      setUpdateState("idle");
    }
  };

  const installDesktopUpdate = async () => {
    const update = updateRef.current;
    if (!update) return;
    setUpdateState("downloading");
    setUpdateProgress(0);
    let downloaded = 0;
    let total: number | undefined;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength;
          downloaded = 0;
          setUpdateProgress(total ? 0 : null);
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setUpdateProgress(total ? Math.min(100, Math.round(downloaded * 100 / total)) : null);
        } else {
          setUpdateProgress(100);
        }
      });
      setUpdateState("restarting");
      await relaunch();
    } catch (error) {
      setUpdateState("idle");
      setUpdateProgress(null);
      showToast("error", `Installation de la mise à jour impossible : ${stringifyError(error)}`);
    }
  };

  useEffect(() => {
    if ("__TAURI_INTERNALS__" in window) {
      void refreshSources();
      void refreshCloud();
      const updateTimer = window.setTimeout(() => void checkDesktopUpdate(), 1500);
      return () => window.clearTimeout(updateTimer);
    }
    else {
      setSourceBusy(false);
      if (import.meta.env.DEV) {
        const preview = new URLSearchParams(window.location.search);
        if (preview.has("preview-library")) setCloudLibrary(browserPreviewLibrary);
        if (preview.has("preview-update")) {
          setDesktopUpdate({
            currentVersion: "0.5.0",
            version: "0.5.1",
            body: "Écriture NFC Android en quatre étapes, strictement limitée à une seule session.",
          });
        }
      }
    }
  }, []);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void (async () => {
      const appWindow = getCurrentWindow();
      const scaleFactor = await appWindow.scaleFactor();
      const stop = await appWindow.onDragDropEvent((event) => {
        const payload = event.payload;
        const targetId = payload.type === "leave"
          ? null
          : document
              .elementFromPoint(payload.position.x / scaleFactor, payload.position.y / scaleFactor)
              ?.closest<HTMLElement>("[data-playlist-id]")
              ?.dataset.playlistId ?? null;
        if (payload.type === "enter" || payload.type === "over") {
          setDraggingFiles(true);
          setDropPlaylistId(playlistEdit?.figureId ?? targetId);
        } else if (payload.type === "leave") {
          setDraggingFiles(false);
          setDropPlaylistId(null);
        } else if (payload.type === "drop") {
          setDraggingFiles(false);
          setDropPlaylistId(null);
          const paths = payload.paths.filter(isMp3Path);
          if (paths.length === 0) {
            showToast("error", "Déposez uniquement des fichiers MP3.");
          } else if (playlistEdit) {
            setPlaylistEdit((current) => current && ({ ...current, addedPaths: [...current.addedPaths, ...paths] }));
          } else if (targetId) {
            setPlaylistEdit({ figureId: targetId, addedPaths: paths });
          } else {
            setBatchImport({ paths });
          }
        }
      });
      if (cancelled) stop();
      else unlisten = stop;
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [playlistEdit?.figureId]);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 5200);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const showToast = (tone: Toast["tone"], message: string) => {
    setToast({ tone, message });
  };

  const openCard = async (path: string) => {
    setBusy(true);
    try {
      const result = await invoke<CardSnapshot>("scan_card", { path });
      setSnapshot(result);
      setSelectedFigureId(result.figures[0]?.id ?? null);
      setQuery("");
      await refreshSources();
      await refreshCloud();
    } catch (error) {
      showToast("error", stringifyError(error));
    } finally {
      setBusy(false);
    }
  };

  const pickCardFolder = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choisir la carte microSD ou son dossier de contenu",
    });
    if (typeof selected === "string") await openCard(selected);
  };

  const rescan = async () => {
    if (snapshot) await openCard(snapshot.rootPath);
    else await refreshSources();
  };

  const figures = useMemo(() => {
    if (!snapshot) return [];
    const needle = query.trim().toLocaleLowerCase("fr");
    if (!needle) return snapshot.figures;
    return snapshot.figures.filter((figure) =>
      [figure.id, figure.customName, figure.folderName]
        .filter(Boolean)
        .some((value) => value!.toLocaleLowerCase("fr").includes(needle)),
    );
  }, [query, snapshot]);

  const selectedFigure = snapshot?.figures.find(
    (figure) => figure.id === selectedFigureId,
  );

  const handleMutation = (result: MutationResult) => {
    setSnapshot(result.snapshot);
    setSelectedFigureId((current) =>
      result.snapshot.figures.some((figure) => figure.id === current)
        ? current
        : (result.snapshot.figures[0]?.id ?? null),
    );
    showToast("success", result.message);
    void refreshSources();
  };

  const deleteSelected = async () => {
    if (!snapshot || !selectedFigure) return;
    const accepted = await ask(
      `Retirer « ${displayName(selectedFigure)} » de la carte ?\n\nUne sauvegarde locale sera créée automatiquement avant la suppression.`,
      { title: "Retirer la figurine", kind: "warning" },
    );
    if (!accepted) return;
    setBusy(true);
    try {
      const result = await invoke<MutationResult>("delete_figure", {
        rootPath: snapshot.rootPath,
        figureId: selectedFigure.id,
      });
      handleMutation(result);
    } catch (error) {
      showToast("error", stringifyError(error));
    } finally {
      setBusy(false);
    }
  };

  const exportSelected = async () => {
    if (!snapshot || !selectedFigure) return;
    const destination = await open({
      directory: true,
      multiple: false,
      title: "Exporter la figurine vers…",
    });
    if (typeof destination !== "string") return;
    setBusy(true);
    try {
      const exportedTo = await invoke<string>("export_figure", {
        rootPath: snapshot.rootPath,
        figureId: selectedFigure.id,
        destinationPath: destination,
      });
      showToast("success", `Copie exportée dans ${exportedTo}`);
    } catch (error) {
      showToast("error", stringifyError(error));
    } finally {
      setBusy(false);
    }
  };

  const renameSelected = async () => {
    if (!snapshot || !selectedFigure) return;
    const name = window.prompt(
      "Nom affiché dans votre bibliothèque locale :",
      selectedFigure.customName ?? "",
    );
    if (name === null) return;
    try {
      const result = await invoke<CardSnapshot>("rename_figure", {
        rootPath: snapshot.rootPath,
        figureId: selectedFigure.id,
        customName: name,
      });
      setSnapshot(result);
      showToast("success", "Nom local enregistré.");
    } catch (error) {
      showToast("error", stringifyError(error));
    }
  };

  const copyNfcPayload = async () => {
    if (!selectedFigure) return;
    await navigator.clipboard.writeText(selectedFigure.nfcPayload);
    showToast("info", "Code NFC copié.");
  };

  const importCloudPlaylist = async (playlist: CloudPlaylist) => {
    if (!snapshot || !editable) {
      showToast("error", "Ouvrez d'abord une carte FABA+ accessible en écriture.");
      return;
    }
    if (playlist.tracks.some((track) => !track.audioAvailable)) {
      showToast("error", "Cette playlist n'a pas encore tous ses fichiers audio dans le cache local.");
      return;
    }
    const alreadyExists = snapshot.figures.some((figure) => figure.id === playlist.figureId);
    if (alreadyExists) {
      const accepted = await ask(
        `K${playlist.figureId} existe déjà sur cette carte. La remplacer par la version de la bibliothèque ?\n\nUne sauvegarde locale sera créée avant le remplacement.`,
        { title: "Écrire la playlist", kind: "warning" },
      );
      if (!accepted) return;
    }
    setBusy(true);
    try {
      const result = await invoke<MutationResult>("cloud_import_playlist", {
        rootPath: snapshot.rootPath,
        figureId: playlist.figureId,
      });
      handleMutation(result);
    } catch (error) {
      showToast("error", stringifyError(error));
    } finally {
      setBusy(false);
    }
  };

  const pickBatchAudio = async () => {
    if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window) && new URLSearchParams(window.location.search).has("preview-library")) {
      setBatchImport({ paths: ["/Musique/Le dragon.mp3", "/Musique/La forêt.mp3"] });
      return;
    }
    const selected = await open({
      multiple: true,
      filters: [{ name: "Fichiers MP3", extensions: ["mp3"] }],
      title: "Importer des sons dans la bibliothèque",
    });
    const paths = typeof selected === "string" ? [selected] : selected;
    if (paths?.length) setBatchImport({ paths });
  };

  const applyLibrary = (library: CloudLibrary, message: string) => {
    setCloudLibrary(library);
    showToast(
      library.offline ? "info" : "success",
      library.offline && library.pendingChanges > 0
        ? `${message} Synchronisation cloud en attente.`
        : message,
    );
    void invoke<CloudStatus>("cloud_status").then(setCloudStatus);
  };

  const renameLibraryPlaylist = async (playlist: CloudPlaylist) => {
    const name = window.prompt("Nouveau nom de la playlist :", playlist.name);
    if (name === null || !name.trim()) return;
    setCloudBusy(true);
    try {
      applyLibrary(
        await invoke<CloudLibrary>("library_rename_playlist", {
          figureId: playlist.figureId,
          name,
        }),
        "Playlist renommée.",
      );
    } catch (error) {
      showToast("error", stringifyError(error));
    } finally {
      setCloudBusy(false);
    }
  };

  const editLibraryPlaylist = (playlist: CloudPlaylist) => {
    setPlaylistEdit({ figureId: playlist.figureId, addedPaths: [] });
  };

  const deleteLibraryPlaylist = async (playlist: CloudPlaylist) => {
    const accepted = await ask(
      `Supprimer « ${playlist.name} » de votre bibliothèque locale et du cloud ?\n\nLa carte SD ne sera pas modifiée.`,
      { title: "Supprimer la playlist", kind: "warning" },
    );
    if (!accepted) return;
    setCloudBusy(true);
    try {
      applyLibrary(
        await invoke<CloudLibrary>("library_delete_playlist", {
          figureId: playlist.figureId,
        }),
        "Playlist supprimée de la bibliothèque.",
      );
    } catch (error) {
      showToast("error", stringifyError(error));
    } finally {
      setCloudBusy(false);
    }
  };

  const syncLibraryToCard = async () => {
    if (!snapshot || !editable) {
      showToast("error", "Ouvrez d'abord une carte FABA+ accessible en écriture.");
      return;
    }
    const accepted = await ask(
      `Synchroniser toute la bibliothèque sur ${lastPathPart(snapshot.rootPath)} ?\n\nLes IDs identiques seront remplacés avec sauvegarde. Les autres contenus déjà présents sur la carte seront conservés.`,
      { title: "Synchroniser la carte", kind: "info" },
    );
    if (!accepted) return;
    setBusy(true);
    try {
      handleMutation(
        await invoke<MutationResult>("sync_library_to_card", {
          rootPath: snapshot.rootPath,
        }),
      );
    } catch (error) {
      showToast("error", stringifyError(error));
    } finally {
      setBusy(false);
    }
  };

  const editable =
    snapshot?.writable && (snapshot.kind === "fabaPlus" || snapshot.kind === "empty");

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark" aria-hidden="true"><img src={appIconUrl} alt="" /></div>
          <div>
            <strong>FABA+</strong>
            <span>Custom Editor</span>
          </div>
        </div>

        <nav className="side-nav" aria-label="Navigation principale">
          <button className="nav-item active" type="button">
            <Library size={18} /> Bibliothèque
          </button>
          <button className="nav-item" type="button" onClick={pickCardFolder}>
            <FolderOpen size={18} /> Ouvrir un dossier
          </button>
          <button className="nav-item" type="button" onClick={() => setDiagnosticsOpen(true)}>
            <Bug size={18} /> Diagnostic technique
          </button>
          <button
            className={`nav-item ${desktopUpdate ? "update-available" : ""}`}
            type="button"
            onClick={() => desktopUpdate ? setUpdateDismissed(false) : void checkDesktopUpdate(true)}
            disabled={updateState === "checking"}
          >
            {updateState === "checking" ? <LoaderCircle className="spin" size={18} /> : <Download size={18} />}
            {desktopUpdate ? `Mise à jour ${desktopUpdate.version}` : "Rechercher une mise à jour"}
          </button>
          <button className="nav-item" type="button" onClick={() => setCloudOpen(true)}>
            {cloudStatus?.authenticated ? <Cloud size={18} /> : <CloudOff size={18} />} FABA Cloud
          </button>
        </nav>

        <section className="source-section">
          <div className="section-label">
            <span>Cartes détectées</span>
            <button
              className="icon-button quiet"
              type="button"
              onClick={refreshSources}
              aria-label="Rafraîchir les cartes"
            >
              <RefreshCw size={14} className={sourceBusy ? "spin" : ""} />
            </button>
          </div>
          {devices.length === 0 && !sourceBusy ? (
            <p className="source-empty">Aucun support amovible détecté.</p>
          ) : (
            devices.map((device) => (
              <button
                className={`source-card ${snapshot?.rootPath === device.mountPath ? "selected" : ""}`}
                type="button"
                key={device.id}
                onClick={() => openCard(device.mountPath)}
              >
                <span className="source-icon">
                  <HardDrive size={18} />
                  {device.likelyFaba && <i />}
                </span>
                <span className="source-copy">
                  <strong>{device.label || "Carte microSD"}</strong>
                  <small>
                    {device.totalBytes > 0
                      ? `${formatBytes(device.totalBytes - device.availableBytes)} utilisés`
                      : device.mountPath}
                  </small>
                </span>
                <ChevronRight size={15} />
              </button>
            ))
          )}
        </section>

        {recentCards.length > 0 && (
          <section className="source-section recent-section">
            <div className="section-label"><span>Récemment ouvertes</span></div>
            {recentCards.slice(0, 4).map((card) => (
              <button
                className="recent-card"
                type="button"
                key={card.rootPath}
                onClick={() => openCard(card.rootPath)}
                title={card.rootPath}
              >
                <Database size={15} />
                <span><strong>{card.label}</strong><small>{card.kind}</small></span>
              </button>
            ))}
          </section>
        )}

        <button className={`sidebar-footer cloud-footer ${cloudStatus?.authenticated ? "connected" : ""}`} type="button" onClick={() => setCloudOpen(true)}>
          {cloudStatus?.authenticated ? <Cloud size={17} /> : <ShieldCheck size={17} />}
          <span>
            <strong>{cloudStatus?.authenticated ? "Bibliothèque synchronisée" : "Sauvegardes automatiques"}</strong>
            <small>{cloudStatus?.authenticated ? cloudStatus.email : "Avant chaque modification"}</small>
          </span>
        </button>
      </aside>

      <main className="main-content">
        <header className="topbar">
          <div>
            <p className="eyebrow">Bibliothèque audio personnelle</p>
            <h1>{snapshot ? "Contenu de la carte" : "Bonjour 👋"}</h1>
          </div>
          <div className="topbar-actions">
            <button className={`button account-button ${cloudStatus?.authenticated ? "connected" : "secondary"}`} type="button" onClick={() => setCloudOpen(true)}>
              {cloudBusy ? <LoaderCircle className="spin" size={16} /> : cloudStatus?.authenticated ? <Cloud size={16} /> : <LogIn size={16} />}
              {cloudStatus?.authenticated ? cloudStatus.displayName || "Mon compte" : "Se connecter"}
            </button>
            {snapshot && (
              <button className="button secondary" type="button" onClick={rescan} disabled={busy}>
                <RefreshCw size={16} className={busy ? "spin" : ""} /> Rescanner
              </button>
            )}
            <button
              className="button primary"
              type="button"
              onClick={pickBatchAudio}
              disabled={busy || cloudBusy}
            >
              <Plus size={17} /> Importer des MP3
            </button>
          </div>
        </header>

        {!snapshot ? (
          <>
            {cloudLibrary && (
              <CloudLibraryPanel
                library={cloudLibrary}
                onSync={() => refreshCloud(true)}
                onAdd={pickBatchAudio}
                onRename={renameLibraryPlaylist}
                onEdit={editLibraryPlaylist}
                onDelete={deleteLibraryPlaylist}
                busy={cloudBusy || busy}
              />
            )}
            <WelcomePanel
              devices={devices}
              busy={busy}
              onOpenCard={openCard}
              onPickFolder={pickCardFolder}
            />
          </>
        ) : (
          <>
            <section className="card-summary">
              <div className="summary-device">
                <div className="summary-icon"><HardDrive size={25} /></div>
                <div>
                  <div className="summary-title-row">
                    <h2>{lastPathPart(snapshot.rootPath)}</h2>
                    <span className={`status-pill ${snapshot.kind}`}>{kindLabels[snapshot.kind]}</span>
                  </div>
                  <p title={snapshot.rootPath}>{snapshot.rootPath}</p>
                </div>
              </div>
              <div className="summary-stat"><strong>{snapshot.figures.length}</strong><span>figurines</span></div>
              <div className="summary-stat"><strong>{totalTracks(snapshot)}</strong><span>pistes</span></div>
              <button className={`summary-stat cloud-summary ${cloudStatus?.authenticated ? "synced" : ""}`} type="button" onClick={() => cloudStatus?.authenticated ? void refreshCloud(true) : setCloudOpen(true)}>
                {cloudBusy ? <LoaderCircle className="spin" size={18} /> : cloudStatus?.authenticated ? <Cloud size={18} /> : <CloudOff size={18} />}
                <span>{cloudStatus?.authenticated ? "Cloud synchronisé" : "Cloud désactivé"}</span>
              </button>
              <div className="summary-stat safe"><ShieldCheck size={18} /><span>{snapshot.writable ? "Écriture sécurisée" : "Lecture seule"}</span></div>
            </section>

            {snapshot.warnings.map((warning) => (
              <div className="inline-warning" key={warning}>
                <AlertTriangle size={18} /><span>{warning}</span>
              </div>
            ))}

            {cloudLibrary && (
              <CloudLibraryPanel
                library={cloudLibrary}
                onSync={() => refreshCloud(true)}
                onAdd={pickBatchAudio}
                onRename={renameLibraryPlaylist}
                onEdit={editLibraryPlaylist}
                onDelete={deleteLibraryPlaylist}
                onImport={importCloudPlaylist}
                onSyncCard={syncLibraryToCard}
                cardWritable={Boolean(editable)}
                busy={cloudBusy || busy}
              />
            )}

            <section className="workspace">
              <div className="library-panel">
                <div className="library-toolbar">
                  <div>
                    <h2>Mes figurines</h2>
                    <span>{figures.length} résultat{figures.length > 1 ? "s" : ""}</span>
                  </div>
                  <label className="search-box">
                    <Search size={17} />
                    <input
                      value={query}
                      onChange={(event) => setQuery(event.target.value)}
                      placeholder="Rechercher un nom ou un ID…"
                    />
                  </label>
                </div>

                {snapshot.figures.length === 0 ? (
                  <div className="empty-library">
                    <div className="empty-art"><Music2 size={38} /><span>+</span></div>
                    <h3>Cette carte est prête</h3>
                    <p>Ajoutez vos MP3 et l’application créera la structure FABA+ correcte.</p>
                    <button className="button primary" type="button" onClick={() => setEditorFigure("new")} disabled={!editable}>
                      <Upload size={17} /> Choisir mes sons
                    </button>
                  </div>
                ) : figures.length === 0 ? (
                  <div className="no-results"><Search size={30} /><p>Aucune figurine ne correspond à cette recherche.</p></div>
                ) : (
                  <div className="figure-grid">
                    {figures.map((figure) => (
                      <button
                        type="button"
                        className={`figure-card ${selectedFigureId === figure.id ? "selected" : ""}`}
                        key={figure.id}
                        onClick={() => setSelectedFigureId(figure.id)}
                      >
                        <span className={`figure-art art-${Number(figure.id) % 6}`}>
                          <Music2 size={29} />
                          <small>K{figure.id}</small>
                        </span>
                        <span className="figure-copy">
                          <strong>{displayName(figure)}</strong>
                          <small>{figure.tracks.length} piste{figure.tracks.length > 1 ? "s" : ""}</small>
                        </span>
                        {figure.warning ? <AlertTriangle className="figure-warning" size={16} /> : <ChevronRight size={16} />}
                      </button>
                    ))}
                  </div>
                )}
              </div>

              {selectedFigure && (
                <aside className="detail-panel">
                  <div className={`detail-art art-${Number(selectedFigure.id) % 6}`}>
                    <Disc3 size={42} />
                    <span>K{selectedFigure.id}</span>
                  </div>
                  <div className="detail-heading">
                    <div>
                      <p>Figurine {selectedFigure.id}</p>
                      <h2>{displayName(selectedFigure)}</h2>
                    </div>
                    <button className="icon-button" type="button" onClick={renameSelected} title="Renommer localement">
                      <Pencil size={16} />
                    </button>
                  </div>

                  {selectedFigure.warning && (
                    <div className="detail-warning"><AlertTriangle size={15} /> {selectedFigure.warning}</div>
                  )}

                  <div className="nfc-card">
                    <div><span>Code à écrire sur le tag NFC</span><code>{selectedFigure.nfcPayload}</code></div>
                    <button className="icon-button" type="button" onClick={copyNfcPayload} title="Copier le code"><Copy size={16} /></button>
                  </div>

                  <div className="track-heading"><h3>Pistes audio</h3><span>{selectedFigure.tracks.length}</span></div>
                  <div className="track-list">
                    {selectedFigure.tracks.map((track) => (
                      <div className="track-row" key={track.path}>
                        <span className="track-index">{String(track.index + 1).padStart(2, "0")}</span>
                        <div><strong>{track.label}</strong><small>{formatBytes(track.sizeBytes)}</small></div>
                        {snapshot.kind === "fabaPlus" && (
                          <audio controls preload="none" src={convertFileSrc(track.path)} aria-label={`Écouter ${track.label}`} />
                        )}
                      </div>
                    ))}
                  </div>

                  <div className="detail-actions">
                    <button className="button secondary" type="button" onClick={exportSelected} disabled={busy}>
                      <Download size={16} /> Exporter
                    </button>
                    <button
                      className="button secondary"
                      type="button"
                      onClick={() => setEditorFigure(selectedFigure)}
                      disabled={!editable || busy}
                    >
                      <RotateCcw size={16} /> Remplacer les sons
                    </button>
                    <button className="button danger" type="button" onClick={deleteSelected} disabled={!editable || busy}>
                      <Trash2 size={16} /> Retirer
                    </button>
                  </div>
                </aside>
              )}
            </section>
          </>
        )}
      </main>

      {busy && <div className="busy-overlay" aria-live="polite"><LoaderCircle className="spin" size={34} /><span>Opération en cours…</span></div>}
      {draggingFiles && <div className="drop-overlay" aria-live="polite"><Upload size={40} /><strong>{dropPlaylistId ? `Ajouter à K${dropPlaylistId}` : "Déposez vos MP3"}</strong><span>{dropPlaylistId ? "Les fichiers seront ajoutés à cette playlist avant validation." : "Ils seront ajoutés à la bibliothèque locale."}</span></div>}
      {toast && (
        <div className={`toast ${toast.tone}`} role="status">
          {toast.tone === "success" ? <Check size={18} /> : toast.tone === "error" ? <AlertTriangle size={18} /> : <CircleHelp size={18} />}
          <span>{toast.message}</span>
          <button type="button" onClick={() => setToast(null)} aria-label="Fermer"><X size={15} /></button>
        </div>
      )}

      {snapshot && editorFigure && (
        <FigureEditor
          rootPath={snapshot.rootPath}
          existingFigures={snapshot.figures}
          initialFigure={editorFigure === "new" ? null : editorFigure}
          onClose={() => setEditorFigure(null)}
          onSaved={(result) => {
            setEditorFigure(null);
            handleMutation(result);
          }}
          onError={(message) => showToast("error", message)}
        />
      )}

      {batchImport && (
        <BatchImportModal
          paths={batchImport.paths}
          onClose={() => setBatchImport(null)}
          onImported={(library) => {
            setBatchImport(null);
            applyLibrary(library, "Import terminé.");
          }}
          onNotify={showToast}
        />
      )}

      {playlistEdit && cloudLibrary?.playlists.find((playlist) => playlist.figureId === playlistEdit.figureId) && (
        <PlaylistEditorModal
          playlist={cloudLibrary.playlists.find((playlist) => playlist.figureId === playlistEdit.figureId)!}
          addedPaths={playlistEdit.addedPaths}
          onClose={() => setPlaylistEdit(null)}
          onSaved={(library) => {
            setPlaylistEdit(null);
            applyLibrary(library, "Playlist mise à jour.");
          }}
          onNotify={showToast}
        />
      )}

      {diagnosticsOpen && (
        <DiagnosticsModal
          onClose={() => setDiagnosticsOpen(false)}
          onNotify={showToast}
        />
      )}

      {cloudOpen && (
        <CloudAccountModal
          status={cloudStatus}
          onClose={() => setCloudOpen(false)}
          onStatus={setCloudStatus}
          onRefresh={async () => {
            return refreshCloud(true);
          }}
          onLibrary={setCloudLibrary}
          onNotify={showToast}
        />
      )}

      {desktopUpdate && !updateDismissed && (
        <DesktopUpdateModal
          update={desktopUpdate}
          state={updateState}
          progress={updateProgress}
          onInstall={() => void installDesktopUpdate()}
          onClose={() => setUpdateDismissed(true)}
        />
      )}
    </div>
  );
}

function DesktopUpdateModal({
  update,
  state,
  progress,
  onInstall,
  onClose,
}: {
  update: DesktopUpdateInfo;
  state: DesktopUpdateState;
  progress: number | null;
  onInstall: () => void;
  onClose: () => void;
}) {
  const installing = state === "downloading" || state === "restarting";
  return (
    <div className="modal-backdrop" role="presentation">
      <section className="modal update-modal" role="dialog" aria-modal="true" aria-labelledby="update-title">
        <div className="modal-header">
          <div>
            <span className="modal-icon update-icon"><Download size={20} /></span>
            <div><p>Mise à jour sécurisée</p><h2 id="update-title">FABA+ Custom Editor {update.version}</h2></div>
          </div>
          {!installing && <button className="icon-button" type="button" onClick={onClose} aria-label="Plus tard"><X size={19} /></button>}
        </div>
        <div className="update-body">
          <div className="update-version-row">
            <span>Version installée <strong>{update.currentVersion}</strong></span>
            <ChevronRight size={18} />
            <span>Nouvelle version <strong>{update.version}</strong></span>
          </div>
          <p className="update-copy">La mise à jour provient de la release GitHub officielle et sa signature sera contrôlée avant l'installation.</p>
          {update.body?.trim() && <div className="update-notes"><strong>Nouveautés</strong><p>{update.body}</p></div>}
          {installing && (
            <div className="update-progress" aria-live="polite">
              <div><span>{state === "restarting" ? "Redémarrage…" : "Téléchargement et installation…"}</span><strong>{progress === null ? "" : `${progress} %`}</strong></div>
              <div className={progress === null ? "indeterminate" : ""}><i style={progress === null ? undefined : { width: `${progress}%` }} /></div>
            </div>
          )}
        </div>
        <div className="modal-footer update-footer">
          <span><ShieldCheck size={16} /> Signature de la release vérifiée automatiquement</span>
          <div>
            <button className="button secondary" type="button" onClick={onClose} disabled={installing}>Plus tard</button>
            <button className="button primary" type="button" onClick={onInstall} disabled={installing}>
              {installing ? <LoaderCircle className="spin" size={17} /> : <Download size={17} />}
              {state === "restarting" ? "Redémarrage…" : "Installer maintenant"}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}

function CloudAccountModal({
  status,
  onClose,
  onStatus,
  onRefresh,
  onLibrary,
  onNotify,
}: {
  status: CloudStatus | null;
  onClose: () => void;
  onStatus: (status: CloudStatus) => void;
  onRefresh: () => Promise<boolean>;
  onLibrary: (library: CloudLibrary | null) => void;
  onNotify: (tone: Toast["tone"], message: string) => void;
}) {
  const [registerMode, setRegisterMode] = useState(false);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [loading, setLoading] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (registerMode && displayName.trim().length === 0) {
      onNotify("error", "Saisissez le nom à afficher.");
      return;
    }
    if (password.length < 10) {
      onNotify("error", "Le mot de passe doit contenir au moins 10 caractères.");
      return;
    }
    setLoading(true);
    try {
      const next = await invoke<CloudStatus>(registerMode ? "cloud_register" : "cloud_login", {
        email,
        password,
        ...(registerMode ? { displayName } : {}),
      });
      setPassword("");
      onStatus(next);
      const synced = await onRefresh();
      if (!synced) {
        onNotify("error", "Compte connecté, mais la synchronisation a échoué. Vous pourrez la relancer sans ressaisir le mot de passe.");
      }
    } catch (error) {
      onNotify("error", stringifyError(error));
    } finally {
      setLoading(false);
    }
  };

  const logout = async () => {
    setLoading(true);
    try {
      const next = await invoke<CloudStatus>("cloud_logout");
      onStatus(next);
      onLibrary(await invoke<CloudLibrary>("cloud_library"));
      onNotify("success", "Compte déconnecté de cet ordinateur.");
    } catch (error) {
      onNotify("error", stringifyError(error));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="modal cloud-modal" role="dialog" aria-modal="true" aria-labelledby="cloud-title">
        <div className="modal-header">
          <div><span className="modal-icon cloud-icon"><Cloud size={20} /></span><div><p>Bibliothèque partagée</p><h2 id="cloud-title">FABA Cloud</h2></div></div>
          <button className="icon-button" type="button" onClick={onClose} aria-label="Fermer"><X size={19} /></button>
        </div>

        {status?.authenticated ? (
          <div className="cloud-account-body">
            <div className="cloud-account-hero"><span><Cloud size={26} /></span><div><strong>{status.displayName}</strong><small>{status.email}</small></div></div>
            <div className="cloud-info-grid">
              <div><span>Serveur</span><strong>{status.endpoint.replace(/^https?:\/\//, "")}</strong></div>
              <div><span>Dernière synchro</span><strong>{status.lastSyncAt ? formatDate(status.lastSyncAt) : "À effectuer"}</strong></div>
            </div>
            <div className="cloud-explainer"><Smartphone size={20} /><p>Playlists et MP3 sont synchronisés dans votre compte privé. Android peut ajouter des sons ; le PC les importe ensuite sur la carte SD au format FABA+.</p></div>
            <div className="cloud-account-actions">
              <button className="button secondary" type="button" onClick={logout} disabled={loading}><LogOut size={16} /> Déconnecter</button>
              <button className="button primary" type="button" onClick={onRefresh} disabled={loading}>{loading ? <LoaderCircle className="spin" size={16} /> : <RefreshCw size={16} />} Synchroniser maintenant</button>
            </div>
          </div>
        ) : (
          <form onSubmit={submit}>
            <div className="cloud-auth-body">
              <div className="cloud-auth-intro"><Cloud size={24} /><div><strong>{registerMode ? "Créer mon compte" : "Retrouver ma bibliothèque"}</strong><p>Une seule connexion, puis la synchronisation devient automatique.</p></div></div>
              {registerMode && <label><span>Nom affiché</span><input value={displayName} onChange={(event) => setDisplayName(event.target.value)} autoComplete="name" maxLength={80} placeholder="Edwin" required /></label>}
              <label><span>Adresse e-mail</span><input value={email} onChange={(event) => setEmail(event.target.value)} type="email" autoComplete="email" placeholder="vous@exemple.be" required /></label>
              <label><span>Mot de passe</span><input value={password} onChange={(event) => setPassword(event.target.value)} type="password" autoComplete={registerMode ? "new-password" : "current-password"} minLength={10} placeholder="10 caractères minimum" required /></label>
              <button className="button primary cloud-submit" type="submit" disabled={loading}>{loading ? <LoaderCircle className="spin" size={17} /> : <LogIn size={17} />}{registerMode ? "Créer et connecter" : "Se connecter"}</button>
              <button className="link-button" type="button" onClick={() => setRegisterMode((current) => !current)}>{registerMode ? "J'ai déjà un compte" : "Créer un nouveau compte"}</button>
              <small className="privacy-note"><ShieldCheck size={14} /> Le mot de passe n'est jamais conservé dans l'application.</small>
            </div>
          </form>
        )}
      </section>
    </div>
  );
}

function BatchImportModal({
  paths,
  onClose,
  onImported,
  onNotify,
}: {
  paths: string[];
  onClose: () => void;
  onImported: (library: CloudLibrary) => void;
  onNotify: (tone: Toast["tone"], message: string) => void;
}) {
  const [mode, setMode] = useState<"onePerFile" | "singlePlaylist">(
    paths.length === 1 ? "singlePlaylist" : "onePerFile",
  );
  const [playlistName, setPlaylistName] = useState(
    paths.length === 1 ? fileNameWithoutExtension(paths[0]) : "",
  );
  const [loading, setLoading] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (mode === "singlePlaylist" && !playlistName.trim()) {
      onNotify("error", "Donnez un nom à la playlist.");
      return;
    }
    setLoading(true);
    try {
      onImported(
        await invoke<CloudLibrary>("library_import_batch", {
          audioPaths: paths,
          mode,
          playlistName: mode === "singlePlaylist" ? playlistName : null,
        }),
      );
    } catch (error) {
      onNotify("error", stringifyError(error));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && !loading && onClose()}>
      <form className="modal batch-modal" role="dialog" aria-modal="true" aria-labelledby="batch-title" onSubmit={submit}>
        <div className="modal-header">
          <div><span className="modal-icon"><Upload size={20} /></span><div><p>Import en lot</p><h2 id="batch-title">Ajouter {paths.length} fichier{paths.length > 1 ? "s" : ""} MP3</h2></div></div>
          <button className="icon-button" type="button" onClick={onClose} disabled={loading} aria-label="Fermer"><X size={19} /></button>
        </div>
        <div className="batch-body">
          <div className="batch-files">
            {paths.map((path) => <span key={path}><Music2 size={14} /><strong>{lastPathPart(path)}</strong></span>)}
          </div>
          <fieldset className="batch-modes">
            <legend>Comment créer les playlists ?</legend>
            <label className={mode === "onePerFile" ? "selected" : ""}>
              <input type="radio" name="batch-mode" checked={mode === "onePerFile"} onChange={() => setMode("onePerFile")} />
              <span><strong>Une playlist par fichier</strong><small>Chaque playlist reprend le nom du MP3. Les IDs sont générés automatiquement.</small></span>
            </label>
            <label className={mode === "singlePlaylist" ? "selected" : ""}>
              <input type="radio" name="batch-mode" checked={mode === "singlePlaylist"} onChange={() => setMode("singlePlaylist")} />
              <span><strong>Une playlist avec tous les sons</strong><small>Les pistes suivent l’ordre de la sélection.</small></span>
            </label>
          </fieldset>
          {mode === "singlePlaylist" && (
            <label className="batch-name"><span>Nom de la playlist</span><input value={playlistName} onChange={(event) => setPlaylistName(event.target.value)} maxLength={100} autoFocus placeholder="Ex. Histoires du soir" required /></label>
          )}
          <p className="batch-hint"><Database size={15} /> Import local immédiat. Si le cloud est indisponible, tout reste sur ce PC et sera envoyé plus tard.</p>
        </div>
        <div className="modal-footer">
          <span><Sparkles size={16} /> IDs personnalisés libres entre 2000 et 8999</span>
          <div><button className="button secondary" type="button" onClick={onClose} disabled={loading}>Annuler</button><button className="button primary" type="submit" disabled={loading}>{loading ? <LoaderCircle className="spin" size={16} /> : <Upload size={16} />} Importer</button></div>
        </div>
      </form>
    </div>
  );
}

type PlaylistEditorTrack = {
  key: string;
  label: string;
  path: string | null;
  added: boolean;
};

function PlaylistEditorModal({
  playlist,
  addedPaths,
  onClose,
  onSaved,
  onNotify,
}: {
  playlist: CloudPlaylist;
  addedPaths: string[];
  onClose: () => void;
  onSaved: (library: CloudLibrary) => void;
  onNotify: (tone: Toast["tone"], message: string) => void;
}) {
  const [tracks, setTracks] = useState<PlaylistEditorTrack[]>(() => [
    ...playlist.tracks.map((track) => ({
      key: `existing-${track.position}`,
      label: track.label,
      path: track.localPath,
      added: false,
    })),
    ...addedPaths.map((path, index) => ({
      key: `dropped-${index}-${path}`,
      label: fileNameWithoutExtension(path),
      path,
      added: true,
    })),
  ]);
  const handledDropCount = useRef(addedPaths.length);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const pending = addedPaths.slice(handledDropCount.current);
    handledDropCount.current = addedPaths.length;
    if (pending.length === 0) return;
    if (tracks.length + pending.length > 99) {
      onNotify("error", "Une playlist est limitée à 99 pistes.");
      return;
    }
    setTracks((current) => [
      ...current,
      ...pending.map((path, index) => ({
        key: `dropped-${handledDropCount.current}-${index}-${path}`,
        label: fileNameWithoutExtension(path),
        path,
        added: true,
      })),
    ]);
  }, [addedPaths]);

  const pickMore = async () => {
    const selected = await open({
      multiple: true,
      filters: [{ name: "Fichiers MP3", extensions: ["mp3"] }],
      title: `Ajouter des sons à ${playlist.name}`,
    });
    const paths = typeof selected === "string" ? [selected] : selected;
    if (!paths?.length) return;
    if (tracks.length + paths.length > 99) {
      onNotify("error", "Une playlist est limitée à 99 pistes.");
      return;
    }
    setTracks((current) => [
      ...current,
      ...paths.map((path, index) => ({
        key: `picked-${Date.now()}-${index}-${path}`,
        label: fileNameWithoutExtension(path),
        path,
        added: true,
      })),
    ]);
  };

  const move = (from: number, to: number) => {
    if (to < 0 || to >= tracks.length) return;
    setTracks((current) => {
      const next = [...current];
      const [track] = next.splice(from, 1);
      next.splice(to, 0, track);
      return next;
    });
  };

  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (tracks.length === 0) {
      onNotify("error", "Une playlist doit conserver au moins une piste.");
      return;
    }
    if (tracks.length > 99) {
      onNotify("error", "Une playlist est limitée à 99 pistes.");
      return;
    }
    if (tracks.some((track) => !track.path)) {
      onNotify("error", "Supprimez les pistes absentes du cache ou resynchronisez le cloud avant d'enregistrer.");
      return;
    }
    setLoading(true);
    try {
      onSaved(await invoke<CloudLibrary>("library_replace_playlist", {
        figureId: playlist.figureId,
        audioPaths: tracks.map((track) => track.path!),
        trackLabels: tracks.map((track) => track.label),
      }));
    } catch (error) {
      onNotify("error", stringifyError(error));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && !loading && onClose()}>
      <form className="modal playlist-editor-modal" role="dialog" aria-modal="true" aria-labelledby="playlist-editor-title" onSubmit={save}>
        <div className="modal-header">
          <div><span className="modal-icon"><Music2 size={20} /></span><div><p>K{playlist.figureId} · édition complète</p><h2 id="playlist-editor-title">{playlist.name}</h2></div></div>
          <button className="icon-button" type="button" onClick={onClose} disabled={loading} aria-label="Fermer"><X size={19} /></button>
        </div>
        <div className="playlist-editor-body">
          <button className="playlist-add-zone" type="button" onClick={pickMore} disabled={loading || tracks.length >= 99}>
            <Plus size={20} /><span><strong>Ajouter des fichiers MP3</strong><small>ou déposez-les directement dans cette fenêtre</small></span>
          </button>
          <div className="playlist-editor-list">
            {tracks.map((track, index) => (
              <div key={track.key} className={!track.path ? "missing" : ""}>
                <span className="playlist-track-index">{String(index + 1).padStart(2, "0")}</span>
                <span className="playlist-track-copy"><strong>{track.label}</strong><small>{track.path ? (track.added ? "Nouveau fichier" : lastPathPart(track.path)) : "Audio absent du cache local"}</small></span>
                <button type="button" onClick={() => move(index, index - 1)} disabled={loading || index === 0} title="Monter"><ArrowUp size={14} /></button>
                <button type="button" onClick={() => move(index, index + 1)} disabled={loading || index === tracks.length - 1} title="Descendre"><ArrowDown size={14} /></button>
                <button type="button" className="remove-track" onClick={() => setTracks((current) => current.filter((_, trackIndex) => trackIndex !== index))} disabled={loading} title="Retirer"><Trash2 size={14} /></button>
              </div>
            ))}
          </div>
          <p className="batch-hint"><Database size={15} /> L'ordre affiché sera l'ordre de lecture sur FABA+, Android et la carte SD.</p>
        </div>
        <div className="modal-footer">
          <span><Music2 size={16} /> {tracks.length}/99 piste{tracks.length > 1 ? "s" : ""}</span>
          <div><button className="button secondary" type="button" onClick={onClose} disabled={loading}>Annuler</button><button className="button primary" type="submit" disabled={loading || tracks.length === 0}>{loading ? <LoaderCircle className="spin" size={16} /> : <Check size={16} />} Enregistrer l'ordre</button></div>
        </div>
      </form>
    </div>
  );
}

function CloudLibraryPanel({
  library,
  onSync,
  onAdd,
  onRename,
  onEdit,
  onDelete,
  onImport,
  onSyncCard,
  cardWritable,
  busy,
}: {
  library: CloudLibrary;
  onSync: () => Promise<unknown>;
  onAdd: () => Promise<void>;
  onRename: (playlist: CloudPlaylist) => Promise<void>;
  onEdit: (playlist: CloudPlaylist) => void;
  onDelete: (playlist: CloudPlaylist) => Promise<void>;
  onImport?: (playlist: CloudPlaylist) => Promise<void>;
  onSyncCard?: () => Promise<void>;
  cardWritable?: boolean;
  busy: boolean;
}) {
  const completeCount = library.playlists.filter((playlist) => playlist.tracks.every((track) => track.audioAvailable)).length;
  return (
    <section className="cloud-library-panel">
      <div className="cloud-library-heading">
        <div><span>{library.offline ? <CloudOff size={18} /> : <Cloud size={18} />}</span><div><p>{completeCount}/{library.playlists.length} prêtes · {formatBytes(library.storageUsedBytes)} · {library.offline ? "cache local" : "cloud à jour"}</p><h2>Ma bibliothèque</h2></div></div>
        <div className="library-heading-actions">
          <button className="button secondary" type="button" onClick={onSync} disabled={busy}>{busy ? <LoaderCircle className="spin" size={15} /> : <RefreshCw size={15} />} Actualiser</button>
          <button className="button secondary" type="button" onClick={onAdd} disabled={busy}><Plus size={15} /> Ajouter</button>
          {onSyncCard && <button className="button primary" type="button" onClick={onSyncCard} disabled={busy || !cardWritable || library.playlists.length === 0}><HardDrive size={15} /> Synchroniser la carte</button>}
        </div>
      </div>
      {(library.offline || library.pendingChanges > 0) && (
        <div className="library-offline-note"><CloudOff size={16} /><span><strong>{library.offline ? "Mode hors ligne" : "Synchronisation en attente"}</strong>{library.pendingChanges > 0 ? ` · ${library.pendingChanges} modification(s) conservée(s) localement.` : " · Bibliothèque disponible depuis le cache de ce PC."}</span></div>
      )}
      {library.playlists.length === 0 ? (
        <button className="cloud-library-empty" type="button" onClick={onAdd}><Upload size={26} /><p><strong>Votre bibliothèque est vide.</strong><br />Importez des MP3 maintenant, sans connecter de carte SD.</p></button>
      ) : (
        <div className="cloud-playlist-grid">
          {library.playlists.map((playlist) => (
            <article key={playlist.figureId} data-playlist-id={playlist.figureId}>
              <span className={`figure-art art-${Number(playlist.figureId) % 6}`}><Music2 size={24} /><small>K{playlist.figureId}</small></span>
              <div className="cloud-playlist-copy"><strong>{playlist.name}</strong><small>{playlist.trackCount} piste{playlist.trackCount > 1 ? "s" : ""} · {playlist.tracks.every((track) => track.audioAvailable) ? "audio local complet" : "audio incomplet"}</small>{playlist.pendingSync && <em>En attente de cloud</em>}</div>
              <div className="cloud-playlist-tools">
                <button className="icon-button" type="button" onClick={() => void onRename(playlist)} disabled={busy} title="Renommer"><Pencil size={14} /></button>
                <button className="icon-button" type="button" onClick={() => onEdit(playlist)} disabled={busy} title="Modifier les pistes"><Music2 size={14} /></button>
                <button className="icon-button danger-icon" type="button" onClick={() => void onDelete(playlist)} disabled={busy} title="Supprimer"><Trash2 size={14} /></button>
              </div>
              <div className="managed-track-list">
                {playlist.tracks.map((track) => (
                  <div key={track.position}><span>{String(track.position + 1).padStart(2, "0")}</span><strong>{track.label}</strong>{track.localPath ? <audio controls preload="none" src={convertFileSrc(track.localPath)} aria-label={`Écouter ${track.label}`} /> : <small>Absent du cache</small>}</div>
                ))}
              </div>
              <div className="cloud-playlist-actions"><code>{playlist.nfcPayload}</code>{onImport && <button className="button secondary" type="button" onClick={() => void onImport(playlist)} disabled={busy || !cardWritable || playlist.tracks.some((track) => !track.audioAvailable)}><Download size={13} /> Écrire uniquement celle-ci</button>}</div>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

function DiagnosticsModal({
  onClose,
  onNotify,
}: {
  onClose: () => void;
  onNotify: (tone: Toast["tone"], message: string) => void;
}) {
  const [report, setReport] = useState<DiagnosticReport | null>(null);
  const [loading, setLoading] = useState(true);

  const loadReport = async () => {
    setLoading(true);
    try {
      setReport(await invoke<DiagnosticReport>("get_diagnostics"));
    } catch (error) {
      onNotify("error", stringifyError(error));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadReport();
  }, []);

  const copyReport = async () => {
    if (!report) return;
    await navigator.clipboard.writeText(report.content);
    onNotify("success", "Journal technique copié.");
  };

  const clearReport = async () => {
    setLoading(true);
    try {
      setReport(await invoke<DiagnosticReport>("clear_diagnostics"));
      onNotify("success", "Journal technique effacé.");
    } catch (error) {
      onNotify("error", stringifyError(error));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="modal diagnostics-modal" role="dialog" aria-modal="true" aria-labelledby="diagnostics-title">
        <div className="modal-header">
          <div>
            <span className="modal-icon diagnostics-icon"><Bug size={20} /></span>
            <div><p>Assistance et débogage</p><h2 id="diagnostics-title">Diagnostic technique</h2></div>
          </div>
          <button className="icon-button" type="button" onClick={onClose} aria-label="Fermer"><X size={17} /></button>
        </div>
        <div className="diagnostics-body">
          <div className="diagnostics-intro">
            <div>
              <strong>Journal local de l’application</strong>
              <p>Les opérations sur la carte et les erreurs système sont enregistrées ici. Aucun contenu audio n’est inclus.</p>
            </div>
            <button className="button secondary" type="button" onClick={loadReport} disabled={loading}>
              <RefreshCw size={15} className={loading ? "spin" : ""} /> Actualiser
            </button>
          </div>
          <div className="diagnostics-path" title={report?.path}>
            <span>Fichier</span><code>{report?.path ?? "Chargement…"}</code>
          </div>
          <pre className="diagnostics-log" aria-live="polite">
            {loading && !report ? "Chargement du journal…" : report?.content || "Le journal est vide."}
          </pre>
        </div>
        <div className="modal-footer diagnostics-footer">
          <span><ShieldCheck size={16} /> Conservé uniquement sur cet ordinateur</span>
          <div>
            <button className="button secondary" type="button" onClick={clearReport} disabled={loading}><Trash2 size={15} /> Effacer</button>
            <button className="button primary" type="button" onClick={copyReport} disabled={!report?.content}><Copy size={15} /> Copier les logs</button>
          </div>
        </div>
      </section>
    </div>
  );
}

function WelcomePanel({
  devices,
  busy,
  onOpenCard,
  onPickFolder,
}: {
  devices: DetectedCard[];
  busy: boolean;
  onOpenCard: (path: string) => Promise<void>;
  onPickFolder: () => Promise<void>;
}) {
  const likelyCard = devices.find((device) => device.likelyFaba) ?? devices[0];
  return (
    <section className="welcome-panel">
      <div className="welcome-copy">
        <span className="welcome-badge"><Sparkles size={14} /> Simple, local et synchronisé</span>
        <h2>Vos histoires.<br /><em>Votre FABA+.</em></h2>
        <p>Commencez par importer et organiser vos sons sur ce PC. Insérez la carte microSD uniquement quand vous êtes prêt à la synchroniser.</p>
        <div className="welcome-actions">
          {likelyCard && (
            <button className="button primary large" type="button" onClick={() => onOpenCard(likelyCard.mountPath)} disabled={busy}>
              <HardDrive size={19} /> Ouvrir {likelyCard.label || "la carte détectée"}
            </button>
          )}
          <button className={`button ${likelyCard ? "secondary" : "primary"} large`} type="button" onClick={onPickFolder} disabled={busy}>
            <FolderOpen size={19} /> Choisir un dossier
          </button>
        </div>
        <div className="trust-row">
          <span><ShieldCheck size={17} /> Sauvegarde avant écriture</span>
          <span><Database size={17} /> Local hors ligne + cloud</span>
          <span><HardDrive size={17} /> Windows · macOS · Linux</span>
        </div>
      </div>
      <div className="welcome-visual" aria-hidden="true">
        <div className="orb orb-one" />
        <div className="orb orb-two" />
        <div className="mock-window">
          <div className="mock-top"><i /><i /><i /><span>FABA+ Custom Editor</span></div>
          <div className="mock-body">
            <div className="mock-side"><b /><b /><b /></div>
            <div className="mock-content">
              <div className="mock-title"><span /><small /></div>
              <div className="mock-cards">
                {[0, 1, 2, 3].map((value) => <div key={value}><i className={`art-${value}`}><Music2 /></i><span /><small /></div>)}
              </div>
            </div>
          </div>
        </div>
        <div className="floating-card safe-float"><ShieldCheck size={21} /><span><strong>Carte protégée</strong><small>Sauvegarde terminée</small></span></div>
        <div className="floating-card tracks-float"><Music2 size={21} /><span><strong>12 pistes</strong><small>prêtes à écouter</small></span></div>
      </div>
    </section>
  );
}

function FigureEditor({
  rootPath,
  existingFigures,
  initialFigure,
  onClose,
  onSaved,
  onError,
}: {
  rootPath: string;
  existingFigures: Figure[];
  initialFigure: Figure | null;
  onClose: () => void;
  onSaved: (result: MutationResult) => void;
  onError: (message: string) => void;
}) {
  const [figureId, setFigureId] = useState(initialFigure?.id ?? "");
  const [customName, setCustomName] = useState(initialFigure?.customName ?? "");
  const [audioPaths, setAudioPaths] = useState<string[]>([]);
  const [riskAccepted, setRiskAccepted] = useState(false);
  const [saving, setSaving] = useState(false);

  const normalizedId = normalizeFigureId(figureId);
  const validCustomId = isCustomFigureId(normalizedId);
  const showInvalidId = figureId.length === 4 && !validCustomId;
  const collision = existingFigures.find((figure) => figure.id === normalizedId);

  const pickAudio = async () => {
    const selected = await open({
      multiple: true,
      directory: false,
      title: "Choisir les pistes MP3 dans l'ordre de lecture",
      filters: [{ name: "Audio MP3", extensions: ["mp3"] }],
    });
    if (Array.isArray(selected)) setAudioPaths(selected);
    else if (typeof selected === "string") setAudioPaths([selected]);
  };

  const moveTrack = (index: number, direction: -1 | 1) => {
    const target = index + direction;
    if (target < 0 || target >= audioPaths.length) return;
    setAudioPaths((paths) => {
      const next = [...paths];
      [next[index], next[target]] = [next[target], next[index]];
      return next;
    });
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!validCustomId) {
      onError("Choisissez un identifiant entre 2000 et 8999. Les plages 0xxx, 1xxx et 9xxx sont réservées par FABA+.");
      return;
    }
    if (audioPaths.length === 0) {
      onError("Ajoutez au moins une piste MP3.");
      return;
    }
    if (!riskAccepted) {
      onError("Confirmez avoir compris l'avertissement concernant FABA+.");
      return;
    }
    setSaving(true);
    try {
      const result = await invoke<MutationResult>("save_figure", {
        rootPath,
        figureId: normalizedId,
        customName,
        audioPaths,
      });
      onSaved(result);
    } catch (error) {
      onError(stringifyError(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="modal" role="dialog" aria-modal="true" aria-labelledby="editor-title">
        <div className="modal-header">
          <div><span className="modal-icon"><Music2 size={20} /></span><div><p>{initialFigure ? "Mettre à jour" : "Nouvelle création"}</p><h2 id="editor-title">{initialFigure ? `Remplacer K${initialFigure.id}` : "Ajouter une figurine"}</h2></div></div>
          <button className="icon-button" type="button" onClick={onClose} aria-label="Fermer"><X size={19} /></button>
        </div>
        <form onSubmit={submit}>
          <div className="modal-body">
            <div className="form-grid">
              <label><span>Identifiant de figurine</span><div className={`id-input ${showInvalidId ? "invalid" : ""}`}><b>K</b><input value={figureId} onChange={(event) => setFigureId(event.target.value.replace(/\D/g, "").slice(0, 4))} placeholder="3101" disabled={Boolean(initialFigure)} inputMode="numeric" aria-invalid={showInvalidId} /></div><small className={showInvalidId ? "field-error" : ""}>{showInvalidId ? "Identifiant réservé ou invalide : utilisez une valeur de 2000 à 8999." : "Choisissez un numéro libre entre 2000 et 8999. Les plages 0xxx, 1xxx et 9xxx sont réservées par FABA+."}</small></label>
              <label><span>Nom dans ma bibliothèque</span><input value={customName} onChange={(event) => setCustomName(event.target.value)} placeholder="Histoires du soir" maxLength={80} /><small>Conservé uniquement sur cet ordinateur.</small></label>
            </div>

            {collision && !initialFigure && (
              <div className="replace-notice"><RotateCcw size={17} /><span><strong>K{normalizedId} existe déjà.</strong> Son contenu sera sauvegardé puis remplacé.</span></div>
            )}

            <div className="audio-picker">
              <div className="picker-heading"><div><h3>Pistes audio</h3><p>L'ordre ci-dessous sera l'ordre de lecture.</p></div><button className="button secondary" type="button" onClick={pickAudio}><FolderOpen size={16} /> Choisir des MP3</button></div>
              {audioPaths.length === 0 ? (
                <button className="drop-zone" type="button" onClick={pickAudio}><span><Upload size={25} /></span><strong>Sélectionner vos fichiers MP3</strong><small>1 à 99 pistes · synchronisation cloud après enregistrement</small></button>
              ) : (
                <div className="selected-tracks">
                  {audioPaths.map((path, index) => (
                    <div key={`${path}-${index}`}>
                      <span className="track-index">{String(index + 1).padStart(2, "0")}</span>
                      <span className="selected-track-name"><strong>{fileName(path)}</strong><small>{path}</small></span>
                      <button type="button" onClick={() => moveTrack(index, -1)} disabled={index === 0} aria-label="Monter"><ArrowUp size={15} /></button>
                      <button type="button" onClick={() => moveTrack(index, 1)} disabled={index === audioPaths.length - 1} aria-label="Descendre"><ArrowDown size={15} /></button>
                      <button type="button" onClick={() => setAudioPaths((paths) => paths.filter((_, itemIndex) => itemIndex !== index))} aria-label="Retirer"><X size={15} /></button>
                    </div>
                  ))}
                </div>
              )}
            </div>

            <label className="risk-check">
              <input type="checkbox" checked={riskAccepted} onChange={(event) => setRiskAccepted(event.target.checked)} />
              <span><strong>Je comprends que FABA+ est un appareil connecté.</strong><small>Le contenu ou les tags non officiels peuvent être détectés par le fabricant. J'utilise uniquement des sons que j'ai le droit d'utiliser et j'accepte le risque lié à la modification.</small></span>
            </label>
          </div>
          <div className="modal-footer">
            <span><ShieldCheck size={16} /> Une sauvegarde précède tout remplacement</span>
            <div><button className="button secondary" type="button" onClick={onClose}>Annuler</button><button className="button primary" type="submit" disabled={saving || !validCustomId}>{saving ? <LoaderCircle className="spin" size={17} /> : <Upload size={17} />}{initialFigure || collision ? "Sauvegarder et remplacer" : "Ajouter à la carte"}</button></div>
          </div>
        </form>
      </section>
    </div>
  );
}

function displayName(figure: Figure) {
  return figure.customName?.trim() || `Figurine K${figure.id}`;
}

function totalTracks(snapshot: CardSnapshot) {
  return snapshot.figures.reduce((total, figure) => total + figure.tracks.length, 0);
}

function formatBytes(value: number) {
  if (!value) return "0 o";
  const units = ["o", "Ko", "Mo", "Go", "To"];
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  return `${(value / 1024 ** index).toLocaleString("fr-BE", { maximumFractionDigits: index ? 1 : 0 })} ${units[index]}`;
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString("fr-BE", { dateStyle: "short", timeStyle: "short" });
}

function lastPathPart(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || "Carte FABA+";
}

function fileName(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || path;
}

function fileNameWithoutExtension(path: string) {
  return fileName(path).replace(/\.[^.]+$/, "");
}

function isMp3Path(path: string) {
  return /\.mp3$/i.test(path);
}

function normalizeFigureId(value: string) {
  if (!value || !/^\d{1,4}$/.test(value)) return "";
  return value.padStart(4, "0");
}

function isCustomFigureId(value: string) {
  return /^\d{4}$/.test(value) && value[0] >= "2" && value[0] <= "8";
}

function stringifyError(error: unknown) {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Une erreur inattendue est survenue.";
}

export default App;
