# 拾流下载器

拾流下载器是一个基于 `Tauri + React + yt-dlp` 的桌面端媒体下载器，面向需要下载单视频、批量链接和播放列表的用户。

The app is a desktop media downloader built with `Tauri + React + yt-dlp`, designed for single videos, batch links, and playlists.

## 功能特性

- 支持单视频、多链接批量下载、播放列表下载
- 支持视频、音频、字幕、视频+字幕四种模式
- 支持 YouTube / Bilibili 等 `yt-dlp` 可解析站点
- 支持浏览器 Cookie 和 Cookie 文件
- 支持任务队列、重试、取消、历史记录
- 支持中英文界面切换
- 打包产物内置 `yt-dlp / ffmpeg / ffprobe / deno`，用户无需额外安装基础依赖

## 当前状态

- 当前仓库内置的是 `macOS arm64` 目标文件
- 已验证可生成 `.app` 和 `.dmg`
- 设置、历史、任务状态都支持本地持久化
- YouTube 登录态场景仍受 `yt-dlp` 上游 challenge 兼容性影响，个别链接可能失败

## 技术栈

- Frontend: `React 19 + TypeScript + Vite`
- Desktop shell: `Tauri 2`
- Download engine: `yt-dlp`
- Media post-processing: `ffmpeg / ffprobe`
- YouTube challenge runtime: `deno`

## 本地开发

前置要求：

- `Node.js`
- `Rust`
- `Tauri` 构建环境

启动开发环境：

```bash
npm install
npm run tauri dev
```

前端单独构建：

```bash
npm run build
```

## 打包

```bash
npm run tauri build
```

默认产物位置：

- `src-tauri/target/release/bundle/macos/拾流下载器.app`
- `src-tauri/target/release/bundle/dmg/拾流下载器_0.1.1_aarch64.dmg`

## 内置依赖

打包产物已内置以下可执行文件：

- `yt-dlp`
- `ffmpeg`
- `ffprobe`
- `deno`

同时，`ffmpeg / ffprobe` 依赖的动态库也会一起打进安装包：

- `src-tauri/resources/ffmpeg-libs/`

这意味着普通用户安装 `.dmg` 后，可以直接启动使用，不需要自己再装 `yt-dlp` 或 `ffmpeg`。

## 使用说明

1. 打开应用，在“下载”页粘贴一个或多个链接。
2. 先解析第一条链接，选择需要的格式。
3. 如果是多链接或播放列表，当前选择会作为默认下载规则应用到整批任务。
4. 到“任务”页查看实时进度、取消或重试。
5. 到“设置”页修改默认目录、认证方式和界面语言。

## 目录结构

```text
src/                 React 前端
src-tauri/src/       Tauri / Rust 后端
src-tauri/binaries/  内置 sidecar 可执行文件
src-tauri/resources/ ffmpeg 动态库资源
docs/                需求文档
```

## 已知限制

- 当前仓库只内置 `macOS arm64` 二进制
- Windows / Intel Mac / Linux 还没有补齐对应 sidecar
- 个别 YouTube 登录态链接可能受上游 challenge 影响
- 浏览器 Cookie 相关能力依赖本机浏览器状态

## 法律与使用说明

- 本项目本身只是 `yt-dlp` 的桌面图形界面，不提供平台内容授权
- 请只在你有权访问、下载和保存内容的前提下使用
- 使用时请遵守所在地区法律、平台条款和版权要求

## License

本仓库当前未单独声明新的许可证；第三方依赖遵循各自许可证。
