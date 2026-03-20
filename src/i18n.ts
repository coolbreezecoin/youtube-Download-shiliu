import type {
  AppLanguage,
  AppView,
  AuthMode,
  CookieBrowser,
  DownloadMode,
  PlaylistScope,
  TaskStatus,
} from "./types";

type Locale = "zh" | "en";

const messages = {
  zh: {
    appName: "拾流下载器",
    tabs: {
      download: "下载",
      tasks: "任务",
      history: "历史",
      settings: "设置",
    } satisfies Record<AppView, string>,
    modeLabels: {
      video: "视频",
      audio: "音频",
      subtitles: "字幕",
      "video+subtitles": "视频 + 字幕",
    } satisfies Record<DownloadMode, string>,
    modeHints: {
      video: "下载所选媒体格式",
      audio: "对所选源格式提取音频",
      subtitles: "只下载字幕，不下载媒体",
      "video+subtitles": "下载媒体并附带字幕",
    } satisfies Record<DownloadMode, string>,
    authLabels: {
      none: "不使用 Cookie",
      browser: "从浏览器读取",
      file: "导入 Cookie 文件",
    } satisfies Record<AuthMode, string>,
    authHints: {
      none: "适合公开可下载内容",
      browser: "适合 YouTube 登录态场景",
      file: "适合 Netscape 格式文件",
    } satisfies Record<AuthMode, string>,
    scopeLabels: {
      video: "当前视频",
      playlist: "整个播放列表",
    } satisfies Record<PlaylistScope, string>,
    scopeHints: {
      video: "优先只解析并下载当前播放的视频",
      playlist: "按列表容器解析和下载",
    } satisfies Record<PlaylistScope, string>,
    browsers: {
      chrome: "Chrome",
      chromium: "Chromium",
      edge: "Edge",
      firefox: "Firefox",
      safari: "Safari",
      brave: "Brave",
      opera: "Opera",
      vivaldi: "Vivaldi",
      whale: "Whale",
    } satisfies Record<CookieBrowser, string>,
    languages: {
      "zh-CN": "简体中文",
      "en-US": "English",
    } satisfies Record<AppLanguage, string>,
    statusLabels: {
      queued: "等待中",
      running: "下载中",
      done: "已完成",
      failed: "失败",
      cancelled: "已取消",
    } satisfies Record<TaskStatus, string>,
    startDownloadLabel: (count: number) =>
      count > 1 ? `批量下载 ${count} 条` : "下载所选格式",
    startingLabel: (count: number) => (count > 1 ? "批量入队中..." : "启动中..."),
    parseBatchSummary: (count: number) => `已识别 ${count} 条链接`,
    downloadSummary: (count: number, succeeded: number, failed: number, preview: string, firstError: string) =>
      `共 ${count} 条链接，成功加入 ${succeeded} 条，失败 ${failed} 条。${preview ? `失败示例：${preview}。` : ""}${firstError}`,
    saveSuccess: "设置已保存，新的默认值已经生效。",
    unknownError: "发生了未知错误",
    input: {
      eyebrow: "链接输入",
      title: "把链接交给拾流下载器处理",
      currentParse: "当前解析第一条链接",
      urlLabel: "视频或播放列表 URL",
      urlPlaceholder: "粘贴一个或多个链接。当前会先解析第一条。",
      parseScope: "解析范围",
      authMethod: "认证方式",
      bilibiliHint: "B 站链接默认不继承浏览器 Cookie；如确有需要，你仍可手动开启。",
      browser: "浏览器",
      cookieFile: "Cookie 文件路径",
      cookiePlaceholder: "例如 ~/Downloads/youtube-cookies.txt",
      currentSelection: "当前选择",
      waitingFormat: "等待解析后选择格式",
      parseButton: "解析链接",
      parsingButton: "解析中...",
    },
    preview: {
      eyebrow: "解析结果",
      waiting: "等待解析",
      author: "作者",
      duration: "时长",
      publishedAt: "发布日期",
      contentType: "内容类型",
      currentVideo: "当前视频",
      singleMedia: "单个媒体",
      playlistLabel: (count: number) => `播放列表 (${count} 项)`,
      downloadableFormats: "可下载格式",
      subtitles: "字幕",
      playlistPreview: "播放列表预览",
      notPlaylist: "非播放列表",
      entryFormats: "条目格式",
      clickEntry: "点击任意条目查看",
      noFormats: "当前结果没有可直接下载的媒体格式，常见于播放列表预览或站点限制。",
      noSubtitles: "当前内容没有可用字幕信息。",
      noPlaylistEntries: "当前解析结果不是播放列表，或站点没有返回条目预览。",
      loadingEntryFormats: "正在加载该条目的可下载格式...",
      entryFormatsHint: "点击某一条视频后，可以为这条单独指定格式；未指定的条目会按默认规则下载。",
      emptyTitle: "还没有解析结果。",
      emptyHint: "先解析链接，右侧会撑满展示所有可下载格式，再从中选择下载。",
      itemCount: (count: number) => `${count} 项`,
    },
    tasks: {
      eyebrow: "任务中心",
      title: "拾流下载器任务队列",
      runningFailed: (running: number, failed: number) => `运行中 ${running} / 失败 ${failed}`,
      clearCompleted: "清空已完成",
      clearFailed: "清空失败/取消",
      clearAll: "全部清空",
      queued: "等待中",
      running: "下载中",
      done: "已完成",
      failed: "失败 / 取消",
      cancel: "取消",
      retry: "重试",
      empty: "还没有任务。先在下载页解析格式并发起下载。",
    },
    history: {
      eyebrow: "历史记录",
      title: "拾流下载器最近完成的任务",
      count: (count: number) => `${count} 条记录`,
      empty: "还没有完成记录。下载完成后会自动出现在这里。",
    },
    settings: {
      eyebrow: "设置",
      defaultsTitle: "拾流下载器默认值",
      startupTag: "启动时自动应用",
      languageLabel: "界面语言",
      outputDir: "默认保存目录",
      outputPlaceholder: "请选择下载目录",
      pickDirectory: "选择目录",
      outputHint: "会调用系统目录选择器，保存后作为默认下载目录使用。",
      defaultMode: "默认下载模式",
      defaultScope: "混合链接默认范围",
      scopeHint: "只影响 `watch?v=...&list=...` 这类同时带视频和播放列表的链接。",
      authTitle: "拾流下载器认证默认值",
      authTag: "用于 YouTube 场景",
      defaultAuth: "默认认证方式",
      defaultBrowser: "默认浏览器",
      defaultCookieFile: "默认 Cookie 文件路径",
      defaultCookiePlaceholder: "认证方式为 Cookie 文件时会自动填入",
      saveTitle: "保存拾流下载器设置",
      dirtyTag: "有未保存变更",
      syncedTag: "已同步到本地",
      currentOutput: "当前保存目录",
      notSet: "未设置",
      summaryMode: "默认下载模式",
      summaryAuth: "默认认证方式",
      saving: "保存中...",
      save: "保存设置",
      reset: "撤销未保存修改",
      saveHint: "保存后会立即更新下载页默认值，并在下次启动时自动恢复。",
    },
  },
  en: {
    appName: "StreamGrabber",
    tabs: {
      download: "Download",
      tasks: "Tasks",
      history: "History",
      settings: "Settings",
    },
    modeLabels: {
      video: "Video",
      audio: "Audio",
      subtitles: "Subtitles",
      "video+subtitles": "Video + Subtitles",
    },
    modeHints: {
      video: "Download the selected media format",
      audio: "Extract audio from the selected source format",
      subtitles: "Download subtitles only",
      "video+subtitles": "Download media with subtitles",
    },
    authLabels: {
      none: "No Cookies",
      browser: "From Browser",
      file: "Cookie File",
    },
    authHints: {
      none: "Best for public content",
      browser: "Best for YouTube signed-in sessions",
      file: "Use a Netscape-format cookie file",
    },
    scopeLabels: {
      video: "Current Video",
      playlist: "Whole Playlist",
    },
    scopeHints: {
      video: "Parse and download only the current video first",
      playlist: "Parse and download the playlist container",
    },
    browsers: {
      chrome: "Chrome",
      chromium: "Chromium",
      edge: "Edge",
      firefox: "Firefox",
      safari: "Safari",
      brave: "Brave",
      opera: "Opera",
      vivaldi: "Vivaldi",
      whale: "Whale",
    },
    languages: {
      "zh-CN": "Simplified Chinese",
      "en-US": "English",
    },
    statusLabels: {
      queued: "Queued",
      running: "Downloading",
      done: "Done",
      failed: "Failed",
      cancelled: "Cancelled",
    },
    startDownloadLabel: (count: number) =>
      count > 1 ? `Download ${count} Items` : "Download Selected Format",
    startingLabel: (count: number) => (count > 1 ? "Queueing..." : "Starting..."),
    parseBatchSummary: (count: number) => `${count} link(s) recognized`,
    downloadSummary: (count: number, succeeded: number, failed: number, preview: string, firstError: string) =>
      `${count} link(s) total, ${succeeded} queued, ${failed} failed.${preview ? ` Failed examples: ${preview}.` : ""} ${firstError}`.trim(),
    saveSuccess: "Settings saved. New defaults are now active.",
    unknownError: "An unknown error occurred",
    input: {
      eyebrow: "Links",
      title: "Send links to StreamGrabber",
      currentParse: "Only the first link is parsed for preview",
      urlLabel: "Video or playlist URL",
      urlPlaceholder: "Paste one or more links. Only the first one will be parsed first.",
      parseScope: "Parse scope",
      authMethod: "Authentication",
      bilibiliHint: "Bilibili links do not inherit browser cookies by default; you can still enable them manually if needed.",
      browser: "Browser",
      cookieFile: "Cookie file path",
      cookiePlaceholder: "Example: ~/Downloads/youtube-cookies.txt",
      currentSelection: "Current selection",
      waitingFormat: "Parse first, then choose a format",
      parseButton: "Parse Link",
      parsingButton: "Parsing...",
    },
    preview: {
      eyebrow: "Preview",
      waiting: "Waiting for parsing",
      author: "Creator",
      duration: "Duration",
      publishedAt: "Published",
      contentType: "Content Type",
      currentVideo: "Current Video",
      singleMedia: "Single Media",
      playlistLabel: (count: number) => `Playlist (${count} items)`,
      downloadableFormats: "Available Formats",
      subtitles: "Subtitles",
      playlistPreview: "Playlist Preview",
      notPlaylist: "Not a playlist",
      entryFormats: "Entry Formats",
      clickEntry: "Click an item to inspect",
      noFormats: "No directly downloadable media formats were returned. This is common for playlist previews or site restrictions.",
      noSubtitles: "No subtitle information is available for this content.",
      noPlaylistEntries: "This result is not a playlist, or the site did not return entry previews.",
      loadingEntryFormats: "Loading formats for this entry...",
      entryFormatsHint: "Click an item to override its format. Unselected entries will use the default rule.",
      emptyTitle: "No preview yet.",
      emptyHint: "Parse a link first. All available formats will appear here for selection.",
      itemCount: (count: number) => `${count} items`,
    },
    tasks: {
      eyebrow: "Task Center",
      title: "StreamGrabber Queue",
      runningFailed: (running: number, failed: number) => `Running ${running} / Failed ${failed}`,
      clearCompleted: "Clear Completed",
      clearFailed: "Clear Failed / Cancelled",
      clearAll: "Clear All",
      queued: "Queued",
      running: "Downloading",
      done: "Done",
      failed: "Failed / Cancelled",
      cancel: "Cancel",
      retry: "Retry",
      empty: "No tasks yet. Parse a format on the download page and start a task first.",
    },
    history: {
      eyebrow: "History",
      title: "Recently completed downloads",
      count: (count: number) => `${count} record(s)`,
      empty: "No completed downloads yet. Finished tasks will appear here automatically.",
    },
    settings: {
      eyebrow: "Settings",
      defaultsTitle: "Default Preferences",
      startupTag: "Applied on startup",
      languageLabel: "App language",
      outputDir: "Default output directory",
      outputPlaceholder: "Choose a download directory",
      pickDirectory: "Choose Folder",
      outputHint: "Uses the system folder picker and saves the result as the default output directory.",
      defaultMode: "Default download mode",
      defaultScope: "Default mixed-link scope",
      scopeHint: "Only affects mixed links like `watch?v=...&list=...`.",
      authTitle: "Default Authentication",
      authTag: "Used mostly for YouTube",
      defaultAuth: "Default authentication",
      defaultBrowser: "Default browser",
      defaultCookieFile: "Default cookie file path",
      defaultCookiePlaceholder: "Filled automatically when Cookie File is selected",
      saveTitle: "Save Preferences",
      dirtyTag: "Unsaved changes",
      syncedTag: "Saved locally",
      currentOutput: "Current output directory",
      notSet: "Not set",
      summaryMode: "Default download mode",
      summaryAuth: "Default authentication",
      saving: "Saving...",
      save: "Save Settings",
      reset: "Discard Changes",
      saveHint: "Saved defaults take effect immediately and will be restored on the next launch.",
    },
  },
} as const;

export function localeFromLanguage(language: AppLanguage): Locale {
  return language === "en-US" ? "en" : "zh";
}

export function copyFor(language: AppLanguage) {
  return messages[localeFromLanguage(language)];
}

export function modeOptionsFor(language: AppLanguage) {
  const copy = copyFor(language);
  return (["video", "audio", "subtitles", "video+subtitles"] as const).map((value) => ({
    value,
    label: copy.modeLabels[value],
    hint: copy.modeHints[value],
  }));
}

export function authOptionsFor(language: AppLanguage) {
  const copy = copyFor(language);
  return (["none", "browser", "file"] as const).map((value) => ({
    value,
    label: copy.authLabels[value],
    hint: copy.authHints[value],
  }));
}

export function scopeOptionsFor(language: AppLanguage) {
  const copy = copyFor(language);
  return (["video", "playlist"] as const).map((value) => ({
    value,
    label: copy.scopeLabels[value],
    hint: copy.scopeHints[value],
  }));
}

export function browserOptionsFor(language: AppLanguage) {
  const copy = copyFor(language);
  return (
    ["chrome", "chromium", "edge", "firefox", "safari", "brave", "opera", "vivaldi", "whale"] as const
  ).map((value) => ({
    value,
    label: copy.browsers[value],
  }));
}

export function statusLabelFor(language: AppLanguage, status: TaskStatus) {
  return copyFor(language).statusLabels[status];
}

