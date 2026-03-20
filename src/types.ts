export type AppView = "download" | "tasks" | "history" | "settings";

export type AppLanguage = "zh-CN" | "en-US";

export type DownloadMode = "video" | "audio" | "subtitles" | "video+subtitles";

export type QualityPreset = "best" | "balanced" | "compact";

export type TaskStatus = "queued" | "running" | "done" | "failed" | "cancelled";

export type EnvironmentState = "ready" | "missing" | "warning";

export type FormatKind = "combined" | "audio" | "video";

export type AuthMode = "none" | "browser" | "file";

export type PlaylistScope = "video" | "playlist";

export type CookieBrowser =
  | "chrome"
  | "chromium"
  | "edge"
  | "firefox"
  | "safari"
  | "brave"
  | "opera"
  | "vivaldi"
  | "whale";

export interface EnvironmentCheck {
  id: string;
  label: string;
  status: EnvironmentState;
  version: string | null;
  detail: string;
  required: boolean;
  autoInstallAvailable: boolean;
  autoInstallLabel: string | null;
  manualInstallHint: string | null;
}

export interface EnvironmentSnapshot {
  checks: EnvironmentCheck[];
  recommendedOutputDir: string;
  note: string;
  installerAvailable: boolean;
  installerName: string | null;
}

export interface InstallProgress {
  status: "running" | "done" | "failed";
  progress: number;
  currentFormula: string | null;
  currentStep: number;
  totalSteps: number;
  message: string;
}

export interface PreviewFormat {
  formatId: string;
  downloadSelector: string;
  label: string;
  detail: string;
  size: string;
  kind: FormatKind;
}

export interface PreviewSubtitle {
  language: string;
  type: string;
  format: string;
}

export interface PlaylistEntry {
  index: number;
  title: string;
  duration: string;
  sourceUrl: string;
}

export interface MediaPreview {
  title: string;
  creator: string;
  duration: string;
  platform: string;
  publishedAt: string;
  thumbnail: string;
  formats: PreviewFormat[];
  subtitles: PreviewSubtitle[];
  playlistEntries: PlaylistEntry[];
  sourceUrl: string;
  isPlaylist: boolean;
  totalEntries: number;
}

export interface DownloadTask {
  id: string;
  title: string;
  status: TaskStatus;
  progress: number;
  speed: string;
  eta: string;
  output: string;
  profile: string;
  sourceUrl: string;
  error: string | null;
}

export interface AuthPayload {
  authMode: AuthMode;
  browser: CookieBrowser;
  cookieFile: string;
}

export interface ParseUrlPayload extends AuthPayload {
  url: string;
  playlistScope: PlaylistScope;
  language: AppLanguage;
}

export interface StartDownloadPayload extends AuthPayload {
  url: string;
  title?: string | null;
  mode: DownloadMode;
  formatId: string | null;
  outputDir: string;
  playlistScope: PlaylistScope;
  language: AppLanguage;
}

export interface HistoryItem {
  title: string;
  finishedAt: string;
  profile: string;
  output: string;
}

export interface AppSettings {
  outputDir: string;
  defaultDownloadMode: DownloadMode;
  defaultPlaylistScope: PlaylistScope;
  defaultAuthMode: AuthMode;
  defaultBrowser: CookieBrowser;
  defaultCookieFile: string;
  language: AppLanguage;
}

export interface SettingsGroup {
  title: string;
  description: string;
  items: Array<{
    label: string;
    value: string;
  }>;
}
