import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { ask, open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  Bug,
  Check,
  ChevronRight,
  CircleHelp,
  Copy,
  Database,
  Disc3,
  Download,
  FolderOpen,
  HardDrive,
  Library,
  LoaderCircle,
  Music2,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  ShieldCheck,
  Sparkles,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { FormEvent, useEffect, useMemo, useState } from "react";
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

type Toast = { tone: "success" | "error" | "info"; message: string };

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

  useEffect(() => {
    if ("__TAURI_INTERNALS__" in window) void refreshSources();
    else setSourceBusy(false);
  }, []);

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

  const editable =
    snapshot?.writable && (snapshot.kind === "fabaPlus" || snapshot.kind === "empty");

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">
            <Disc3 size={24} />
            <Sparkles className="brand-spark" size={12} />
          </div>
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

        <div className="sidebar-footer">
          <ShieldCheck size={17} />
          <span><strong>Sauvegardes automatiques</strong><small>Avant chaque modification</small></span>
        </div>
      </aside>

      <main className="main-content">
        <header className="topbar">
          <div>
            <p className="eyebrow">Bibliothèque audio personnelle</p>
            <h1>{snapshot ? "Contenu de la carte" : "Bonjour 👋"}</h1>
          </div>
          <div className="topbar-actions">
            {snapshot && (
              <button className="button secondary" type="button" onClick={rescan} disabled={busy}>
                <RefreshCw size={16} className={busy ? "spin" : ""} /> Rescanner
              </button>
            )}
            <button
              className="button primary"
              type="button"
              onClick={() => setEditorFigure("new")}
              disabled={!editable || busy}
              title={!editable ? "Ouvrez une carte FABA+ accessible en écriture" : undefined}
            >
              <Plus size={17} /> Ajouter une figurine
            </button>
          </div>
        </header>

        {!snapshot ? (
          <WelcomePanel
            devices={devices}
            busy={busy}
            onOpenCard={openCard}
            onPickFolder={pickCardFolder}
          />
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
              <div className="summary-stat safe"><ShieldCheck size={18} /><span>{snapshot.writable ? "Écriture sécurisée" : "Lecture seule"}</span></div>
            </section>

            {snapshot.warnings.map((warning) => (
              <div className="inline-warning" key={warning}>
                <AlertTriangle size={18} /><span>{warning}</span>
              </div>
            ))}

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

      {diagnosticsOpen && (
        <DiagnosticsModal
          onClose={() => setDiagnosticsOpen(false)}
          onNotify={showToast}
        />
      )}
    </div>
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
        <span className="welcome-badge"><Sparkles size={14} /> Simple, local et sans cloud</span>
        <h2>Vos histoires.<br /><em>Votre FABA+.</em></h2>
        <p>Insérez la carte microSD de votre FABA+, puis organisez vos propres sons sans scripts, Docker ou ligne de commande.</p>
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
          <span><Database size={17} /> Index local SQLite</span>
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
    if (!normalizedId || normalizedId === "0000") {
      onError("Saisissez un identifiant entre 0001 et 9999.");
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
              <label><span>Identifiant de figurine</span><div className="id-input"><b>K</b><input value={figureId} onChange={(event) => setFigureId(event.target.value.replace(/\D/g, "").slice(0, 4))} placeholder="0742" disabled={Boolean(initialFigure)} inputMode="numeric" /></div><small>Choisissez un numéro libre entre 0001 et 9999. Il sera ensuite encodé sur un tag NFC vierge.</small></label>
              <label><span>Nom dans ma bibliothèque</span><input value={customName} onChange={(event) => setCustomName(event.target.value)} placeholder="Histoires du soir" maxLength={80} /><small>Conservé uniquement sur cet ordinateur.</small></label>
            </div>

            {collision && !initialFigure && (
              <div className="replace-notice"><RotateCcw size={17} /><span><strong>K{normalizedId} existe déjà.</strong> Son contenu sera sauvegardé puis remplacé.</span></div>
            )}

            <div className="audio-picker">
              <div className="picker-heading"><div><h3>Pistes audio</h3><p>L'ordre ci-dessous sera l'ordre de lecture.</p></div><button className="button secondary" type="button" onClick={pickAudio}><FolderOpen size={16} /> Choisir des MP3</button></div>
              {audioPaths.length === 0 ? (
                <button className="drop-zone" type="button" onClick={pickAudio}><span><Upload size={25} /></span><strong>Sélectionner vos fichiers MP3</strong><small>1 à 99 pistes · aucun fichier n'est envoyé en ligne</small></button>
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
            <div><button className="button secondary" type="button" onClick={onClose}>Annuler</button><button className="button primary" type="submit" disabled={saving}>{saving ? <LoaderCircle className="spin" size={17} /> : <Upload size={17} />}{initialFigure || collision ? "Sauvegarder et remplacer" : "Ajouter à la carte"}</button></div>
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

function lastPathPart(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || "Carte FABA+";
}

function fileName(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || path;
}

function normalizeFigureId(value: string) {
  if (!value || !/^\d{1,4}$/.test(value)) return "";
  return value.padStart(4, "0");
}

function stringifyError(error: unknown) {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Une erreur inattendue est survenue.";
}

export default App;
