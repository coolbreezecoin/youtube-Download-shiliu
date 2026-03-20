import type { EnvironmentSnapshot, HistoryItem, SettingsGroup } from "./types";

export const fallbackEnvironment: EnvironmentSnapshot = {
  recommendedOutputDir: "~/Downloads/拾流下载器",
  note: "当前为界面预览模式。进入拾流下载器桌面窗口后会自动检测 yt-dlp、ffmpeg 和 JavaScript runtime。",
  installerAvailable: false,
  installerName: null,
  checks: [
    {
      id: "yt-dlp",
      label: "yt-dlp",
      status: "warning",
      version: null,
      detail: "尚未从桌面运行时读取，后续会在启动时自动检测。",
      required: true,
      autoInstallAvailable: false,
      autoInstallLabel: null,
      manualInstallHint: "请先安装 Homebrew，再执行 brew install yt-dlp。",
    },
    {
      id: "ffmpeg",
      label: "ffmpeg",
      status: "warning",
      version: null,
      detail: "用于音视频合并、转码和封面嵌入。",
      required: true,
      autoInstallAvailable: false,
      autoInstallLabel: null,
      manualInstallHint: "请先安装 Homebrew，再执行 brew install ffmpeg。",
    },
    {
      id: "runtime",
      label: "JS Runtime",
      status: "warning",
      version: null,
      detail: "YouTube 完整支持需要 Node.js、Deno、Bun 或 QuickJS 之一。",
      required: false,
      autoInstallAvailable: false,
      autoInstallLabel: null,
      manualInstallHint:
        "建议安装 Bun：brew tap oven-sh/bun && brew install oven-sh/bun/bun。",
    },
  ],
};

export const historyData: HistoryItem[] = [
  {
    title: "Weekly Design Review - MP3",
    finishedAt: "2026-03-15 19:42",
    profile: "仅音频 / m4a -> mp3",
    output: "~/Downloads/拾流下载器/Audio",
  },
  {
    title: "Frontend Toolkit Playlist",
    finishedAt: "2026-03-15 10:18",
    profile: "播放列表 / 1080p / 字幕",
    output: "~/Downloads/拾流下载器/Playlists",
  },
  {
    title: "Interview Archive",
    finishedAt: "2026-03-14 23:09",
    profile: "最佳质量 / 嵌入封面",
    output: "~/Downloads/拾流下载器/Archive",
  },
];

export const settingsGroups: SettingsGroup[] = [
  {
    title: "下载默认值",
    description: "决定普通用户打开应用后的默认行为。",
    items: [
      { label: "默认模式", value: "视频" },
      { label: "质量策略", value: "平衡质量" },
      { label: "保存目录", value: "~/Downloads/拾流下载器" },
    ],
  },
  {
    title: "依赖路径",
    description: "后续会支持自动检测和手动覆盖。",
    items: [
      { label: "yt-dlp", value: "自动检测" },
      { label: "ffmpeg", value: "自动检测" },
      { label: "JavaScript runtime", value: "自动检测" },
    ],
  },
  {
    title: "高级能力",
    description: "对应需求文档中的渐进增强能力。",
    items: [
      { label: "播放列表并发", value: "2" },
      { label: "自动下载字幕", value: "关闭" },
      { label: "失败任务自动重试", value: "1 次" },
    ],
  },
];
