import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { startTransition, useEffect, useState } from "react";
import "./App.css";
import {
  authOptionsFor,
  browserOptionsFor,
  copyFor,
  modeOptionsFor,
  scopeOptionsFor,
  statusLabelFor,
} from "./i18n";
import { fallbackEnvironment } from "./mock";
import type {
  AppView,
  AppLanguage,
  AppSettings,
  AuthMode,
  CookieBrowser,
  DownloadMode,
  DownloadTask,
  HistoryItem,
  MediaPreview,
  ParseUrlPayload,
  PlaylistEntry,
  PlaylistScope,
  StartDownloadPayload,
  TaskStatus,
} from "./types";

function App() {
  const [settings, setSettings] = useState<AppSettings>(buildDefaultSettings());
  const [savedSettings, setSavedSettings] = useState<AppSettings>(buildDefaultSettings());
  const [activeView, setActiveView] = useState<AppView>("download");
  const [downloadMode, setDownloadMode] = useState<DownloadMode>("video");
  const [playlistScope, setPlaylistScope] = useState<PlaylistScope>("video");
  const [authMode, setAuthMode] = useState<AuthMode>("none");
  const [authModeTouched, setAuthModeTouched] = useState(false);
  const [browser, setBrowser] = useState<CookieBrowser>("chrome");
  const [cookieFile, setCookieFile] = useState("");
  const [urlInput, setUrlInput] = useState("");
  const [saveDirectory, setSaveDirectory] = useState(
    fallbackEnvironment.recommendedOutputDir,
  );
  const [preview, setPreview] = useState<MediaPreview | null>(null);
  const [selectedFormatId, setSelectedFormatId] = useState<string | null>(null);
  const [selectedPlaylistEntryIndex, setSelectedPlaylistEntryIndex] = useState<number | null>(
    null,
  );
  const [playlistEntryPreviews, setPlaylistEntryPreviews] = useState<
    Record<number, MediaPreview>
  >({});
  const [playlistEntrySelections, setPlaylistEntrySelections] = useState<
    Record<number, string>
  >({});
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [isParsing, setIsParsing] = useState(false);
  const [isStartingDownload, setIsStartingDownload] = useState(false);
  const [isLoadingPlaylistEntry, setIsLoadingPlaylistEntry] = useState(false);
  const [parseError, setParseError] = useState("");
  const [downloadError, setDownloadError] = useState("");
  const [playlistEntryError, setPlaylistEntryError] = useState("");
  const [settingsError, setSettingsError] = useState("");
  const [settingsMessage, setSettingsMessage] = useState("");
  const [isSavingSettings, setIsSavingSettings] = useState(false);
  const language = settings.language;
  const copy = copyFor(language);
  const modeOptions = modeOptionsFor(language);
  const authOptions = authOptionsFor(language);
  const browserOptions = browserOptionsFor(language);
  const playlistScopeOptions = scopeOptionsFor(language);

  useEffect(() => {
    let mounted = true;

    async function loadInitialState() {
      try {
        const [existingTasks, existingHistory, persistedSettings] = await Promise.all([
          invoke<DownloadTask[]>("get_tasks"),
          invoke<HistoryItem[]>("get_history"),
          invoke<AppSettings>("get_settings"),
        ]);

        if (!mounted) {
          return;
        }

        setSettings(persistedSettings);
        setSavedSettings(persistedSettings);
        setDownloadMode(persistedSettings.defaultDownloadMode);
        setPlaylistScope(persistedSettings.defaultPlaylistScope);
        setAuthMode(preferredAuthModeForUrl("", persistedSettings.defaultAuthMode));
        setAuthModeTouched(false);
        setBrowser(persistedSettings.defaultBrowser);
        setCookieFile(persistedSettings.defaultCookieFile);
        setSaveDirectory(persistedSettings.outputDir);
        setTasks(existingTasks);
        setHistory(existingHistory);
      } catch {
        if (!mounted) {
          return;
        }

        const defaults = buildDefaultSettings();
        setSettings(defaults);
        setSavedSettings(defaults);
        setDownloadMode(defaults.defaultDownloadMode);
        setPlaylistScope(defaults.defaultPlaylistScope);
        setAuthMode(preferredAuthModeForUrl("", defaults.defaultAuthMode));
        setAuthModeTouched(false);
        setBrowser(defaults.defaultBrowser);
        setCookieFile(defaults.defaultCookieFile);
        setSaveDirectory(defaults.outputDir);
        setHistory([]);
      }
    }

    void loadInitialState();

    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let cleanupTaskEvents: (() => void) | undefined;
    let cleanupHistoryEvents: (() => void) | undefined;

    async function bindTaskEvents() {
      cleanupTaskEvents = await listen<DownloadTask>("download-task-updated", (event) => {
        if (cancelled) {
          return;
        }

        setTasks((current) => upsertTask(current, event.payload));
      });

      cleanupHistoryEvents = await listen<HistoryItem>("history-item-added", (event) => {
        if (cancelled) {
          return;
        }

        setHistory((current) => {
          const next = current.filter(
            (item) =>
              !(
                item.title === event.payload.title &&
                item.finishedAt === event.payload.finishedAt &&
                item.output === event.payload.output
              ),
          );
          return [event.payload, ...next];
        });
      });
    }

    void bindTaskEvents();

    return () => {
      cancelled = true;
      cleanupTaskEvents?.();
      cleanupHistoryEvents?.();
    };
  }, []);

  const normalizedUrls = urlInput
    .split("\n")
    .map((url) => url.trim())
    .filter(Boolean);

  const firstUrl = normalizedUrls[0] ?? "";
  const playlistMode = detectPlaylistMode(firstUrl, settings.defaultPlaylistScope);
  const selectedFormat =
    preview?.formats.find((format) => format.formatId === selectedFormatId) ?? null;
  const visibleFormats = filterFormatsForMode(preview?.formats ?? [], downloadMode);
  const visibleSubtitles = preview?.subtitles.slice(0, 4) ?? [];
  const visiblePlaylistEntries = preview?.playlistEntries ?? [];
  const selectedPlaylistEntry =
    visiblePlaylistEntries.find((entry) => entry.index === selectedPlaylistEntryIndex) ?? null;
  const selectedPlaylistEntryPreview =
    (selectedPlaylistEntryIndex !== null
      ? playlistEntryPreviews[selectedPlaylistEntryIndex]
      : null) ?? null;
  const visibleSelectedPlaylistEntryFormats = filterFormatsForMode(
    selectedPlaylistEntryPreview?.formats ?? [],
    downloadMode,
  );
  const settingsDirty = isSettingsDirty(settings, savedSettings);

  const statusCounts = tasks.reduce<Record<TaskStatus, number>>(
    (accumulator, task) => {
      accumulator[task.status] += 1;
      return accumulator;
    },
    {
      queued: 0,
      running: 0,
      done: 0,
      failed: 0,
      cancelled: 0,
    },
  );

  useEffect(() => {
    setPlaylistScope(playlistMode.defaultScope);
  }, [playlistMode.defaultScope, firstUrl]);

  useEffect(() => {
    setAuthModeTouched(false);
  }, [firstUrl]);

  useEffect(() => {
    if (authModeTouched) {
      return;
    }

    setAuthMode(preferredAuthModeForUrl(firstUrl, settings.defaultAuthMode));
  }, [authModeTouched, firstUrl, settings.defaultAuthMode]);

  useEffect(() => {
    if (!preview) {
      return;
    }

    const hasSelectedVisibleFormat = visibleFormats.some(
      (format) => format.formatId === selectedFormatId,
    );

    if (!hasSelectedVisibleFormat) {
      setSelectedFormatId(visibleFormats[0]?.formatId ?? null);
    }
  }, [preview, selectedFormatId, visibleFormats]);

  useEffect(() => {
    if (!preview?.isPlaylist) {
      setSelectedPlaylistEntryIndex(null);
      setPlaylistEntryPreviews({});
      setPlaylistEntrySelections({});
      setPlaylistEntryError("");
    }
  }, [preview]);

  useEffect(() => {
    if (!selectedPlaylistEntry || visibleSelectedPlaylistEntryFormats.length === 0) {
      return;
    }

    const currentSelection = playlistEntrySelections[selectedPlaylistEntry.index];
    const hasVisibleSelection = visibleSelectedPlaylistEntryFormats.some(
      (format) => format.downloadSelector === currentSelection,
    );

    if (!hasVisibleSelection) {
      setPlaylistEntrySelections((current) => ({
        ...current,
        [selectedPlaylistEntry.index]: visibleSelectedPlaylistEntryFormats[0].downloadSelector,
      }));
    }
  }, [
    playlistEntrySelections,
    selectedPlaylistEntry,
    visibleSelectedPlaylistEntryFormats,
  ]);

  async function handleParse() {
    setParseError("");
    setDownloadError("");
    setPlaylistEntryError("");

    if (!firstUrl) {
      setParseError(
        language === "en-US"
          ? "Enter a valid link before parsing."
          : "请先输入一个可解析的链接。",
      );
      return;
    }

    setIsParsing(true);

    try {
      const nextPreview = await invoke<MediaPreview>("parse_url", {
        payload: {
          url: firstUrl,
          playlistScope,
          authMode,
          browser,
          cookieFile,
          language,
        } satisfies ParseUrlPayload,
      });

      setPreview(nextPreview);
      setSelectedFormatId(defaultFormatId(nextPreview, downloadMode));
      setSelectedPlaylistEntryIndex(null);
      setPlaylistEntryPreviews({});
      setPlaylistEntrySelections({});
    } catch (error) {
      setPreview(null);
      setSelectedFormatId(null);
      setSelectedPlaylistEntryIndex(null);
      setPlaylistEntryPreviews({});
      setPlaylistEntrySelections({});
      setParseError(stringifyError(error));
    } finally {
      setIsParsing(false);
    }
  }

  async function handleSelectPlaylistEntry(entry: PlaylistEntry) {
    setSelectedPlaylistEntryIndex(entry.index);
    setPlaylistEntryError("");

    if (playlistEntryPreviews[entry.index] || !entry.sourceUrl) {
      return;
    }

    setIsLoadingPlaylistEntry(true);

    try {
      const nextPreview = await invoke<MediaPreview>("parse_url", {
        payload: {
          url: entry.sourceUrl,
          playlistScope: "video",
          authMode,
          browser,
          cookieFile,
          language,
        } satisfies ParseUrlPayload,
      });

      setPlaylistEntryPreviews((current) => ({
        ...current,
        [entry.index]: nextPreview,
      }));
      setPlaylistEntrySelections((current) => ({
        ...current,
        [entry.index]:
          current[entry.index] ??
          defaultDownloadSelector(nextPreview, downloadMode) ??
          "",
      }));
    } catch (error) {
      setPlaylistEntryError(stringifyError(error));
    } finally {
      setIsLoadingPlaylistEntry(false);
    }
  }

  async function handleStartDownload() {
    setDownloadError("");
    const defaultSelector =
      downloadMode === "subtitles"
        ? null
        : selectedFormat?.downloadSelector ?? selectedFormatId;

    const batchTargets = buildBatchTargets({
      normalizedUrls,
      preview,
      defaultSelector,
      playlistEntrySelections,
      downloadMode,
      playlistScope,
    });
    const firstTargetUrl = batchTargets[0]?.url ?? "";

    if (!firstTargetUrl) {
      setDownloadError(
        language === "en-US"
          ? "Enter a link and parse it before downloading."
          : "请先输入链接并完成解析。",
      );
      return;
    }

    if (downloadMode !== "subtitles" && !selectedFormatId) {
      setDownloadError(
        language === "en-US"
          ? "Choose an available format from the preview first."
          : "请先从解析结果中选择一个可下载格式。",
      );
      return;
    }

    setIsStartingDownload(true);

    try {
      const results = await Promise.allSettled(
        batchTargets.map((target) =>
          invoke<DownloadTask>("start_download", {
            payload: {
              url: target.url,
              title: target.title,
              mode: downloadMode,
              formatId: target.formatId,
              outputDir: saveDirectory,
              playlistScope: target.playlistScope,
              authMode,
              browser,
              cookieFile,
              language,
            } satisfies StartDownloadPayload,
          }),
        ),
      );

      const succeeded = results
        .filter((result): result is PromiseFulfilledResult<DownloadTask> => result.status === "fulfilled")
        .map((result) => result.value);
      const failed = results
        .map((result, index) => ({ result, url: batchTargets[index]?.url ?? "" }))
        .filter(
          (item): item is {
            result: PromiseRejectedResult;
            url: string;
          } => item.result.status === "rejected",
        );

      if (succeeded.length > 0) {
        setTasks((current) =>
          succeeded.reduce((next, task) => upsertTask(next, task), current),
        );
        startTransition(() => setActiveView("tasks"));
      }

      if (failed.length > 0) {
        const previewUrls = failed
          .slice(0, 2)
          .map((item) => item.url)
          .join(language === "en-US" ? "; " : "；");
        const firstError = stringifyError(failed[0].result.reason);
        setDownloadError(copy.downloadSummary(batchTargets.length, succeeded.length, failed.length, previewUrls, firstError));
      }
    } catch (error) {
      setDownloadError(stringifyError(error));
    } finally {
      setIsStartingDownload(false);
    }
  }

  async function handleCancelTask(taskId: string) {
    try {
      const nextTask = await invoke<DownloadTask>("cancel_download", { taskId });
      setTasks((current) => upsertTask(current, nextTask));
    } catch (error) {
      setDownloadError(stringifyError(error));
    }
  }

  async function handleRetryTask(taskId: string) {
    try {
      const nextTask = await invoke<DownloadTask>("retry_download", { taskId });
      setTasks((current) => upsertTask(current, nextTask));
    } catch (error) {
      setDownloadError(stringifyError(error));
    }
  }

  async function handleClearTasks(scope: "completed" | "failed" | "all") {
    try {
      const nextTasks = await invoke<DownloadTask[]>("clear_tasks", { scope });
      setTasks(nextTasks);
    } catch (error) {
      setDownloadError(stringifyError(error));
    }
  }

  async function handleSaveSettings() {
    setSettingsError("");
    setSettingsMessage("");
    setIsSavingSettings(true);

    try {
      const persisted = await invoke<AppSettings>("save_settings", { payload: settings });
      setSettings(persisted);
      setSavedSettings(persisted);
      setDownloadMode(persisted.defaultDownloadMode);
      setPlaylistScope(detectPlaylistMode(firstUrl, persisted.defaultPlaylistScope).defaultScope);
      setAuthMode(preferredAuthModeForUrl(firstUrl, persisted.defaultAuthMode));
      setAuthModeTouched(false);
      setBrowser(persisted.defaultBrowser);
      setCookieFile(persisted.defaultCookieFile);
      setSaveDirectory(persisted.outputDir);
      setSettingsMessage(copy.saveSuccess);
    } catch (error) {
      setSettingsError(stringifyError(error));
    } finally {
      setIsSavingSettings(false);
    }
  }

  function handleResetSettings() {
    setSettings(savedSettings);
    setSettingsError("");
    setSettingsMessage("");
  }

  async function handlePickOutputDirectory() {
    setSettingsError("");
    setSettingsMessage("");

    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: settings.outputDir || saveDirectory,
      });

      if (typeof selected !== "string" || !selected.trim()) {
        return;
      }

      setSettings((current) => ({
        ...current,
        outputDir: selected,
      }));
    } catch (error) {
      setSettingsError(stringifyError(error));
    }
  }

  return (
    <main className="app-shell">
      <nav className="top-nav" aria-label="Primary">
        {[
          { id: "download", label: copy.tabs.download },
          { id: "tasks", label: copy.tabs.tasks },
          { id: "history", label: copy.tabs.history },
          { id: "settings", label: copy.tabs.settings },
        ].map((tab) => (
          <button
            key={tab.id}
            type="button"
            className={tab.id === activeView ? "nav-pill active" : "nav-pill"}
            onClick={() => {
              startTransition(() => setActiveView(tab.id as AppView));
            }}
          >
            {tab.label}
          </button>
        ))}
      </nav>

      {activeView === "download" ? (
        <section className="dashboard-grid dashboard-grid-wide">
          <article className="panel composer-panel">
            <div className="panel-header">
              <div>
                <p className="eyebrow">{copy.input.eyebrow}</p>
                <h2>{copy.input.title}</h2>
              </div>
              <span className="panel-tag">{copy.input.currentParse}</span>
            </div>
            <div className="panel-scroll-body">
              <label className="field-label" htmlFor="urls">
                {copy.input.urlLabel}
              </label>
              <textarea
                id="urls"
                className="url-input"
                value={urlInput}
                onChange={(event) => setUrlInput(event.currentTarget.value)}
                placeholder={copy.input.urlPlaceholder}
              />

              <p className="helper-copy compact">{copy.parseBatchSummary(normalizedUrls.length)}</p>

              {playlistMode.showScopeSelector ? (
                <div className="inline-card scope-card">
                  <span className="field-label">{copy.input.parseScope}</span>
                  <div className="scope-grid">
                    {playlistScopeOptions.map((option) => (
                      <button
                        key={option.value}
                        type="button"
                        className={
                          option.value === playlistScope
                            ? "select-chip active"
                            : "select-chip"
                        }
                        onClick={() => setPlaylistScope(option.value)}
                      >
                        <strong>{option.label}</strong>
                        <span>{option.hint}</span>
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}

              <div className="chip-grid">
                {modeOptions.map((option) => (
                  <button
                    key={option.value}
                    type="button"
                    className={
                      option.value === downloadMode ? "select-chip active" : "select-chip"
                    }
                    onClick={() => setDownloadMode(option.value)}
                  >
                    <strong>{option.label}</strong>
                    <span>{option.hint}</span>
                  </button>
                ))}
              </div>

              <div className="inline-card auth-card">
                <span className="field-label">{copy.input.authMethod}</span>
                <div className="auth-grid">
                  {authOptions.map((option) => (
                    <button
                      key={option.value}
                      type="button"
                      className={
                        option.value === authMode ? "select-chip active" : "select-chip"
                      }
                      onClick={() => {
                        setAuthModeTouched(true);
                        setAuthMode(option.value);
                      }}
                    >
                      <strong>{option.label}</strong>
                      <span>{option.hint}</span>
                    </button>
                  ))}
                </div>

                {isBilibiliUrl(firstUrl) ? (
                  <p className="helper-copy compact">{copy.input.bilibiliHint}</p>
                ) : null}

                {authMode === "browser" ? (
                  <div className="auth-detail-row">
                    <label className="field-label" htmlFor="cookie-browser">
                      {copy.input.browser}
                    </label>
                    <select
                      id="cookie-browser"
                      value={browser}
                      onChange={(event) => setBrowser(event.currentTarget.value as CookieBrowser)}
                    >
                      {browserOptions.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                  </div>
                ) : null}

                {authMode === "file" ? (
                  <div className="auth-detail-row">
                    <label className="field-label" htmlFor="cookie-file">
                      {copy.input.cookieFile}
                    </label>
                    <input
                      id="cookie-file"
                      value={cookieFile}
                      onChange={(event) => setCookieFile(event.currentTarget.value)}
                      placeholder={copy.input.cookiePlaceholder}
                    />
                  </div>
                ) : null}
              </div>

              {parseError ? <p className="error-banner">{parseError}</p> : null}
              {downloadError ? <p className="error-banner">{downloadError}</p> : null}

              <div className="selection-summary">
                <span>{copy.input.currentSelection}</span>
                <strong>
                  {selectedFormat
                    ? `${selectedFormat.label} / ${selectedFormat.detail}`
                    : copy.input.waitingFormat}
                </strong>
              </div>

              <div className="action-row">
                <button
                  type="button"
                  className="primary-action"
                  onClick={() => void handleParse()}
                  disabled={isParsing}
                >
                  {isParsing ? copy.input.parsingButton : copy.input.parseButton}
                </button>
                <button
                  type="button"
                  className="secondary-action"
                  onClick={() => void handleStartDownload()}
                  disabled={isStartingDownload}
                >
                  {isStartingDownload
                    ? copy.startingLabel(normalizedUrls.length)
                    : copy.startDownloadLabel(normalizedUrls.length)}
                </button>
              </div>
            </div>
          </article>

          <article className="panel preview-panel preview-panel-wide">
            <div className="panel-header">
              <div>
                <p className="eyebrow">{copy.preview.eyebrow}</p>
                <h2>{preview?.title ?? copy.preview.waiting}</h2>
              </div>
              <span className="panel-tag">{preview?.platform ?? copy.appName}</span>
            </div>
            <div className="panel-scroll-body">
              {preview ? (
                <>
                  <div
                    className="preview-cover"
                    style={{ backgroundImage: `url(${preview.thumbnail})` }}
                  />

                  <div className="meta-grid">
                    <div>
                      <span>{copy.preview.author}</span>
                      <strong>{preview.creator}</strong>
                    </div>
                    <div>
                      <span>{copy.preview.duration}</span>
                      <strong>{preview.duration}</strong>
                    </div>
                    <div>
                      <span>{copy.preview.publishedAt}</span>
                      <strong>{preview.publishedAt}</strong>
                    </div>
                    <div>
                      <span>{copy.preview.contentType}</span>
                      <strong>
                        {playlistScope === "video"
                          ? copy.preview.currentVideo
                          : preview.isPlaylist
                          ? copy.preview.playlistLabel(preview.totalEntries)
                          : copy.preview.singleMedia}
                      </strong>
                    </div>
                  </div>

                  <div className="stack-section">
                    <div className="section-title-row">
                      <h3>{copy.preview.downloadableFormats}</h3>
                      <span className="text-meta">{copy.preview.itemCount(preview.formats.length)}</span>
                    </div>
                    <div className="list-stack format-grid">
                      {visibleFormats.length > 0 ? (
                        visibleFormats.map((format) => (
                          <button
                            key={format.formatId}
                            type="button"
                            className={
                              format.formatId === selectedFormatId
                                ? "list-card format-card active"
                                : "list-card format-card"
                            }
                            onClick={() => setSelectedFormatId(format.formatId)}
                          >
                            <div>
                              <strong>{format.label}</strong>
                              <p>{format.detail}</p>
                            </div>
                            <div className="format-side">
                              <span>{format.size}</span>
                              <small>{format.kind}</small>
                            </div>
                          </button>
                        ))
                      ) : (
                        <div className="empty-state">{copy.preview.noFormats}</div>
                      )}
                    </div>
                  </div>

                  <div className="stack-section stack-two-column">
                    <section>
                      <div className="section-title-row">
                        <h3>{copy.preview.subtitles}</h3>
                        <span className="text-meta">{copy.preview.itemCount(preview.subtitles.length)}</span>
                      </div>
                      <div className="list-stack compact">
                        {visibleSubtitles.length > 0 ? (
                          visibleSubtitles.map((subtitle) => (
                            <div
                              key={`${subtitle.language}-${subtitle.type}-${subtitle.format}`}
                              className="list-card"
                            >
                              <div>
                                <strong>{subtitle.language}</strong>
                                <p>{subtitle.type}</p>
                              </div>
                              <span>{subtitle.format}</span>
                            </div>
                          ))
                        ) : (
                          <div className="empty-state">{copy.preview.noSubtitles}</div>
                        )}
                      </div>
                    </section>

                    <section>
                      <div className="section-title-row">
                        <h3>{copy.preview.playlistPreview}</h3>
                      <span className="text-meta">
                        {preview.isPlaylist
                          ? copy.preview.itemCount(preview.totalEntries)
                          : copy.preview.notPlaylist}
                      </span>
                    </div>
                    <div className="list-stack compact">
                      {visiblePlaylistEntries.length > 0 ? (
                        visiblePlaylistEntries.map((entry) => (
                          <button
                            key={entry.index}
                            type="button"
                            className={
                              entry.index === selectedPlaylistEntryIndex
                                ? "list-card playlist-entry-button active"
                                : "list-card playlist-entry-button"
                            }
                            onClick={() => void handleSelectPlaylistEntry(entry)}
                          >
                            <div>
                              <strong>
                                #{entry.index} {entry.title}
                              </strong>
                            </div>
                            <span>{entry.duration}</span>
                          </button>
                        ))
                      ) : (
                        <div className="empty-state">{copy.preview.noPlaylistEntries}</div>
                      )}
                    </div>

                    {preview.isPlaylist ? (
                      <div className="stack-section">
                        <div className="section-title-row">
                          <h3>{copy.preview.entryFormats}</h3>
                          <span className="text-meta">
                            {selectedPlaylistEntry
                              ? `#${selectedPlaylistEntry.index}`
                              : copy.preview.clickEntry}
                          </span>
                        </div>
                        {playlistEntryError ? (
                          <div className="error-banner">{playlistEntryError}</div>
                        ) : null}
                        {isLoadingPlaylistEntry ? (
                          <div className="empty-state">{copy.preview.loadingEntryFormats}</div>
                        ) : selectedPlaylistEntry && visibleSelectedPlaylistEntryFormats.length > 0 ? (
                          <div className="list-stack format-grid">
                            {visibleSelectedPlaylistEntryFormats.map((format) => (
                              <button
                                key={`${selectedPlaylistEntry.index}-${format.formatId}`}
                                type="button"
                                className={
                                  playlistEntrySelections[selectedPlaylistEntry.index] ===
                                  format.downloadSelector
                                    ? "list-card format-card active"
                                    : "list-card format-card"
                                }
                                onClick={() =>
                                  setPlaylistEntrySelections((current) => ({
                                    ...current,
                                    [selectedPlaylistEntry.index]: format.downloadSelector,
                                  }))
                                }
                              >
                                <div>
                                  <strong>{format.label}</strong>
                                  <p>{format.detail}</p>
                                </div>
                                <div className="format-side">
                                  <span>{format.size}</span>
                                  <small>{format.kind}</small>
                                </div>
                              </button>
                            ))}
                          </div>
                        ) : (
                          <div className="empty-state">{copy.preview.entryFormatsHint}</div>
                        )}
                      </div>
                    ) : null}
                  </section>
                </div>
                </>
              ) : (
                <div className="empty-preview">
                  <p>{copy.preview.emptyTitle}</p>
                  <span>{copy.preview.emptyHint}</span>
                </div>
              )}
              </div>
          </article>
        </section>
      ) : null}

      {activeView === "tasks" ? (
        <section className="content-grid">
          <article className="panel wide-panel">
            <div className="panel-header">
              <div>
                <p className="eyebrow">{copy.tasks.eyebrow}</p>
                <h2>{copy.tasks.title}</h2>
              </div>
              <div className="task-toolbar">
                <span className="panel-tag">
                  {copy.tasks.runningFailed(statusCounts.running, statusCounts.failed)}
                </span>
                <button
                  type="button"
                  className="ghost-action"
                  onClick={() => void handleClearTasks("completed")}
                >
                  {copy.tasks.clearCompleted}
                </button>
                <button
                  type="button"
                  className="ghost-action"
                  onClick={() => void handleClearTasks("failed")}
                >
                  {copy.tasks.clearFailed}
                </button>
                <button
                  type="button"
                  className="ghost-action"
                  onClick={() => void handleClearTasks("all")}
                >
                  {copy.tasks.clearAll}
                </button>
              </div>
            </div>
            <div className="panel-scroll-body">
              <div className="task-stats">
                <div className="metric-card small">
                  <span>{copy.tasks.queued}</span>
                  <strong>{statusCounts.queued}</strong>
                </div>
                <div className="metric-card small">
                  <span>{copy.tasks.running}</span>
                  <strong>{statusCounts.running}</strong>
                </div>
                <div className="metric-card small">
                  <span>{copy.tasks.done}</span>
                  <strong>{statusCounts.done}</strong>
                </div>
                <div className="metric-card small">
                  <span>{copy.tasks.failed}</span>
                  <strong>{statusCounts.failed + statusCounts.cancelled}</strong>
                </div>
              </div>

              <div className="list-stack">
                {tasks.length > 0 ? (
                  tasks.map((task) => (
                    <div key={task.id} className="task-row">
                      <div className="task-main">
                        <div className="task-title-row">
                          <strong>{task.title}</strong>
                          <span className={`status-badge ${task.status}`}>
                            {statusLabelFor(language, task.status)}
                          </span>
                        </div>
                        <div className="progress-track">
                          <span style={{ width: `${task.progress}%` }} />
                        </div>
                        <p>{task.output}</p>
                        <small className="task-profile">{task.profile}</small>
                        {task.error ? <small className="task-error">{task.error}</small> : null}
                      </div>
                      <div className="task-side">
                        <strong>{Math.round(task.progress)}%</strong>
                        <small>{task.speed}</small>
                        <small>{task.eta}</small>
                        <div className="task-actions">
                          {(task.status === "queued" || task.status === "running") ? (
                            <button
                              type="button"
                              className="ghost-action"
                              onClick={() => void handleCancelTask(task.id)}
                            >
                              {copy.tasks.cancel}
                            </button>
                          ) : null}
                          {(task.status === "failed" || task.status === "cancelled") ? (
                            <button
                              type="button"
                              className="secondary-action"
                              onClick={() => void handleRetryTask(task.id)}
                            >
                              {copy.tasks.retry}
                            </button>
                          ) : null}
                        </div>
                      </div>
                    </div>
                  ))
                ) : (
                  <div className="empty-state">{copy.tasks.empty}</div>
                )}
              </div>
            </div>
          </article>
        </section>
      ) : null}

      {activeView === "history" ? (
        <section className="content-grid">
          <article className="panel wide-panel">
            <div className="panel-header">
              <div>
                <p className="eyebrow">{copy.history.eyebrow}</p>
                <h2>{copy.history.title}</h2>
              </div>
              <span className="panel-tag">{copy.history.count(history.length)}</span>
            </div>

            <div className="panel-scroll-body">
              <div className="list-stack">
                {history.length > 0 ? (
                  history.map((item) => (
                    <div key={`${item.title}-${item.finishedAt}`} className="history-row">
                      <div>
                        <strong>{item.title}</strong>
                        <p>{item.profile}</p>
                      </div>
                      <div className="history-meta">
                        <span>{item.finishedAt}</span>
                        <small>{item.output}</small>
                      </div>
                    </div>
                  ))
                ) : (
                  <div className="empty-state">{copy.history.empty}</div>
                )}
              </div>
            </div>
          </article>
        </section>
      ) : null}

      {activeView === "settings" ? (
        <section className="content-grid settings-grid">
          <article className="panel">
            <div className="panel-header">
              <div>
                <p className="eyebrow">{copy.settings.eyebrow}</p>
                <h2>{copy.settings.defaultsTitle}</h2>
              </div>
              <span className="panel-tag">{copy.settings.startupTag}</span>
            </div>

            <div className="panel-scroll-body settings-form">
              <div className="settings-field">
                <span className="field-label">{copy.settings.languageLabel}</span>
                <div className="settings-browser-grid">
                  {(["zh-CN", "en-US"] as AppLanguage[]).map((value) => (
                    <button
                      key={`settings-language-${value}`}
                      type="button"
                      className={settings.language === value ? "select-chip active" : "select-chip"}
                      onClick={() =>
                        setSettings((current) => ({
                          ...current,
                          language: value,
                        }))
                      }
                    >
                      <strong>{copy.languages[value]}</strong>
                    </button>
                  ))}
                </div>
              </div>

              <div className="settings-field">
                <label className="field-label" htmlFor="settings-output-dir">
                  {copy.settings.outputDir}
                </label>
                <div className="path-picker-row">
                  <input
                    id="settings-output-dir"
                    value={settings.outputDir}
                    readOnly
                    placeholder={copy.settings.outputPlaceholder}
                  />
                  <button
                    type="button"
                    className="secondary-action"
                    onClick={() => void handlePickOutputDirectory()}
                  >
                    {copy.settings.pickDirectory}
                  </button>
                </div>
                <p className="helper-copy compact">
                  {copy.settings.outputHint}
                </p>
              </div>

              <div className="settings-field">
                <span className="field-label">{copy.settings.defaultMode}</span>
                <div className="chip-grid">
                  {modeOptions.map((option) => (
                    <button
                      key={`settings-mode-${option.value}`}
                      type="button"
                      className={
                        settings.defaultDownloadMode === option.value
                          ? "select-chip active"
                          : "select-chip"
                      }
                      onClick={() =>
                        setSettings((current) => ({
                          ...current,
                          defaultDownloadMode: option.value,
                        }))
                      }
                    >
                      <strong>{option.label}</strong>
                      <span>{option.hint}</span>
                    </button>
                  ))}
                </div>
              </div>

              <div className="settings-field">
                <span className="field-label">{copy.settings.defaultScope}</span>
                <div className="scope-grid">
                  {playlistScopeOptions.map((option) => (
                    <button
                      key={`settings-scope-${option.value}`}
                      type="button"
                      className={
                        settings.defaultPlaylistScope === option.value
                          ? "select-chip active"
                          : "select-chip"
                      }
                      onClick={() =>
                        setSettings((current) => ({
                          ...current,
                          defaultPlaylistScope: option.value,
                        }))
                      }
                    >
                      <strong>{option.label}</strong>
                      <span>{option.hint}</span>
                    </button>
                  ))}
                </div>
                <p className="helper-copy compact">
                  {copy.settings.scopeHint}
                </p>
              </div>
            </div>
          </article>

          <article className="panel">
            <div className="panel-header">
              <div>
                <p className="eyebrow">{copy.settings.eyebrow}</p>
                <h2>{copy.settings.authTitle}</h2>
              </div>
              <span className="panel-tag">{copy.settings.authTag}</span>
            </div>

            <div className="panel-scroll-body settings-form">
              <div className="settings-field">
                <span className="field-label">{copy.settings.defaultAuth}</span>
                <div className="auth-grid">
                  {authOptions.map((option) => (
                    <button
                      key={`settings-auth-${option.value}`}
                      type="button"
                      className={
                        settings.defaultAuthMode === option.value
                          ? "select-chip active"
                          : "select-chip"
                      }
                      onClick={() =>
                        setSettings((current) => ({
                          ...current,
                          defaultAuthMode: option.value,
                        }))
                      }
                    >
                      <strong>{option.label}</strong>
                      <span>{option.hint}</span>
                    </button>
                  ))}
                </div>
              </div>

              {settings.defaultAuthMode === "browser" ? (
                <div className="settings-field">
                  <span className="field-label">{copy.settings.defaultBrowser}</span>
                  <div className="settings-browser-grid">
                    {browserOptions.map((option) => (
                      <button
                        key={`settings-browser-${option.value}`}
                        type="button"
                        className={
                          settings.defaultBrowser === option.value
                            ? "select-chip active"
                            : "select-chip"
                        }
                        onClick={() =>
                          setSettings((current) => ({
                            ...current,
                            defaultBrowser: option.value,
                          }))
                        }
                      >
                        <strong>{option.label}</strong>
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}

              {settings.defaultAuthMode === "file" ? (
                <div className="settings-field">
                  <label className="field-label" htmlFor="settings-cookie-file">
                    {copy.settings.defaultCookieFile}
                  </label>
                  <input
                    id="settings-cookie-file"
                    value={settings.defaultCookieFile}
                    onChange={(event) =>
                      setSettings((current) => ({
                        ...current,
                        defaultCookieFile: event.currentTarget.value,
                      }))
                    }
                    placeholder={copy.settings.defaultCookiePlaceholder}
                  />
                </div>
              ) : null}
            </div>
          </article>

          <article className="panel">
            <div className="panel-header">
              <div>
                <p className="eyebrow">{copy.settings.eyebrow}</p>
                <h2>{copy.settings.saveTitle}</h2>
              </div>
              <span className="panel-tag">
                {settingsDirty ? copy.settings.dirtyTag : copy.settings.syncedTag}
              </span>
            </div>

            <div className="panel-scroll-body settings-form">
              <div className="settings-summary-card">
                <div className="setting-row">
                  <span>{copy.settings.currentOutput}</span>
                  <strong>{settings.outputDir || copy.settings.notSet}</strong>
                </div>
                <div className="setting-row">
                  <span>{copy.settings.summaryMode}</span>
                  <strong>{labelForMode(settings.defaultDownloadMode, language)}</strong>
                </div>
                <div className="setting-row">
                  <span>{copy.settings.summaryAuth}</span>
                  <strong>{labelForAuthMode(settings.defaultAuthMode, language)}</strong>
                </div>
              </div>

              {settingsError ? <p className="error-banner">{settingsError}</p> : null}
              {settingsMessage ? <p className="success-banner">{settingsMessage}</p> : null}

              <div className="action-row settings-actions">
                <button
                  type="button"
                  className="primary-action"
                  onClick={() => void handleSaveSettings()}
                  disabled={isSavingSettings || !settingsDirty}
                >
                  {isSavingSettings ? copy.settings.saving : copy.settings.save}
                </button>
                <button
                  type="button"
                  className="ghost-action"
                  onClick={handleResetSettings}
                  disabled={isSavingSettings || !settingsDirty}
                >
                  {copy.settings.reset}
                </button>
              </div>

              <p className="supporting-copy">
                {copy.settings.saveHint}
              </p>
            </div>
          </article>
        </section>
      ) : null}
    </main>
  );
}

function buildDefaultSettings(): AppSettings {
  return {
    outputDir: fallbackEnvironment.recommendedOutputDir,
    defaultDownloadMode: "video",
    defaultPlaylistScope: "video",
    defaultAuthMode: "none",
    defaultBrowser: "chrome",
    defaultCookieFile: "",
    language: "zh-CN",
  };
}

function defaultFormatId(preview: MediaPreview, mode: DownloadMode) {
  return filterFormatsForMode(preview.formats, mode)[0]?.formatId ?? null;
}

function defaultDownloadSelector(preview: MediaPreview, mode: DownloadMode) {
  return filterFormatsForMode(preview.formats, mode)[0]?.downloadSelector ?? null;
}

function filterFormatsForMode(formats: MediaPreview["formats"], mode: DownloadMode) {
  if (mode === "audio") {
    return formats.filter((format) => format.kind === "audio");
  }

  if (mode === "video" || mode === "video+subtitles") {
    return formats.filter((format) => {
      if (format.kind === "audio") {
        return false;
      }

      const height = parseFormatHeight(format.label);
      return height === null || height > 360;
    });
  }

  return formats;
}

function parseFormatHeight(label: string) {
  const match = label.match(/(\d+)p/i);
  return match ? Number.parseInt(match[1], 10) : null;
}

function buildBatchTargets({
  normalizedUrls,
  preview,
  defaultSelector,
  playlistEntrySelections,
  downloadMode,
  playlistScope,
}: {
  normalizedUrls: string[];
  preview: MediaPreview | null;
  defaultSelector: string | null;
  playlistEntrySelections: Record<number, string>;
  downloadMode: DownloadMode;
  playlistScope: PlaylistScope;
}) {
  if (preview?.isPlaylist && playlistScope === "playlist" && preview.playlistEntries.length > 0) {
    return preview.playlistEntries
      .filter((entry) => entry.sourceUrl)
      .map((entry) => ({
        url: entry.sourceUrl,
        title: entry.title,
        formatId:
          downloadMode === "subtitles"
            ? null
            : playlistEntrySelections[entry.index] || defaultSelector,
        playlistScope: "video" as PlaylistScope,
      }));
  }

  return normalizedUrls.map((url, index) => ({
    url: index === 0 ? preview?.sourceUrl ?? url : url,
    title: index === 0 ? preview?.title ?? null : null,
    formatId: downloadMode === "subtitles" ? null : defaultSelector,
    playlistScope,
  }));
}

function upsertTask(tasks: DownloadTask[], nextTask: DownloadTask) {
  const existingIndex = tasks.findIndex((task) => task.id === nextTask.id);

  if (existingIndex === -1) {
    return [nextTask, ...tasks];
  }

  return tasks.map((task) => (task.id === nextTask.id ? nextTask : task));
}

function stringifyError(error: unknown) {
  if (typeof error === "string") {
    return error;
  }

  if (error instanceof Error) {
    return error.message;
  }

  return copyFor("zh-CN").unknownError;
}

function isSettingsDirty(current: AppSettings, saved: AppSettings) {
  return JSON.stringify(current) !== JSON.stringify(saved);
}

function labelForMode(mode: DownloadMode, language: AppLanguage) {
  return modeOptionsFor(language).find((option) => option.value === mode)?.label ?? mode;
}

function labelForAuthMode(mode: AuthMode, language: AppLanguage) {
  return authOptionsFor(language).find((option) => option.value === mode)?.label ?? mode;
}

function preferredAuthModeForUrl(url: string, defaultAuthMode: AuthMode) {
  if (isBilibiliUrl(url)) {
    return "none" as AuthMode;
  }

  return defaultAuthMode;
}

function isBilibiliUrl(url: string) {
  const lowercased = url.toLowerCase();
  return lowercased.includes("bilibili.com/") || lowercased.includes("b23.tv/");
}

function detectPlaylistMode(url: string, defaultScope: PlaylistScope) {
  if (!url) {
    return { showScopeSelector: false, defaultScope };
  }

  try {
    const parsed = new URL(url);
    const hasVideo = parsed.searchParams.has("v");
    const hasPlaylist = parsed.searchParams.has("list");

    if (hasVideo && hasPlaylist) {
      return { showScopeSelector: true, defaultScope };
    }

    if (hasPlaylist) {
      return { showScopeSelector: false, defaultScope: "playlist" as PlaylistScope };
    }
  } catch {
    return { showScopeSelector: false, defaultScope };
  }

  return { showScopeSelector: false, defaultScope: "video" as PlaylistScope };
}

export default App;
