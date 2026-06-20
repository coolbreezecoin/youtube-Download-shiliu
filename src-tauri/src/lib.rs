use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

const MAX_CONCURRENT_DOWNLOADS: usize = 3;
static TASK_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct AppState {
    tasks: Arc<Mutex<HashMap<String, DownloadTask>>>,
    task_requests: Arc<Mutex<HashMap<String, StartDownloadRequest>>>,
    task_pids: Arc<Mutex<HashMap<String, u32>>>,
    cancelled_tasks: Arc<Mutex<HashSet<String>>>,
    download_gate: Arc<(Mutex<usize>, Condvar)>,
    history: Arc<Mutex<Vec<HistoryItem>>>,
    settings: Arc<Mutex<AppSettings>>,
    state_path: PathBuf,
}

struct DownloadPermit {
    gate: Arc<(Mutex<usize>, Condvar)>,
}

impl Drop for DownloadPermit {
    fn drop(&mut self) {
        release_download_permit(&self.gate);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewFormat {
    format_id: String,
    download_selector: String,
    label: String,
    detail: String,
    size: String,
    kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewSubtitle {
    language: String,
    #[serde(rename = "type")]
    subtitle_type: String,
    format: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistEntry {
    index: usize,
    title: String,
    duration: String,
    source_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaPreview {
    title: String,
    creator: String,
    duration: String,
    platform: String,
    published_at: String,
    thumbnail: String,
    formats: Vec<PreviewFormat>,
    subtitles: Vec<PreviewSubtitle>,
    playlist_entries: Vec<PlaylistEntry>,
    source_url: String,
    is_playlist: bool,
    total_entries: usize,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DownloadTask {
    id: String,
    title: String,
    status: String,
    progress: f32,
    speed: String,
    eta: String,
    output: String,
    profile: String,
    source_url: String,
    error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct HistoryItem {
    title: String,
    finished_at: String,
    profile: String,
    output: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    output_dir: String,
    default_download_mode: String,
    default_playlist_scope: String,
    default_auth_mode: String,
    default_browser: String,
    default_cookie_file: String,
    language: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        default_settings()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParseUrlRequest {
    url: String,
    playlist_scope: String,
    auth_mode: String,
    browser: Option<String>,
    cookie_file: Option<String>,
    language: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StartDownloadRequest {
    url: String,
    title: Option<String>,
    mode: String,
    format_id: Option<String>,
    output_dir: String,
    playlist_scope: String,
    auth_mode: String,
    browser: Option<String>,
    cookie_file: Option<String>,
    language: String,
}

struct ProgressUpdate {
    progress: f32,
    speed: String,
    eta: String,
}

struct DownloadAttemptResult {
    success: bool,
    error: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PersistedState {
    tasks: HashMap<String, DownloadTask>,
    task_requests: HashMap<String, StartDownloadRequest>,
    history: Vec<HistoryItem>,
    #[serde(default = "default_settings")]
    settings: AppSettings,
}

fn load_app_state() -> AppState {
    let state_path = persistence_path();
    let persisted = fs::read_to_string(&state_path)
        .ok()
        .and_then(|content| serde_json::from_str::<PersistedState>(&content).ok())
        .unwrap_or_default();
    let sanitized_settings = sanitize_settings(persisted.settings);
    let language = sanitized_settings.language.clone();

    let tasks = persisted
        .tasks
        .into_iter()
        .map(|(id, mut task)| {
            if matches!(task.status.as_str(), "queued" | "running") {
                task.status = "failed".into();
                task.error = Some(message_unfinished_after_restart(&language));
                task.speed = "--".into();
                task.eta = "--".into();
            }

            (id, task)
        })
        .collect::<HashMap<_, _>>();

    AppState {
        tasks: Arc::new(Mutex::new(tasks)),
        task_requests: Arc::new(Mutex::new(persisted.task_requests)),
        task_pids: Arc::new(Mutex::new(HashMap::new())),
        cancelled_tasks: Arc::new(Mutex::new(HashSet::new())),
        download_gate: Arc::new((Mutex::new(MAX_CONCURRENT_DOWNLOADS), Condvar::new())),
        history: Arc::new(Mutex::new(persisted.history)),
        settings: Arc::new(Mutex::new(sanitized_settings)),
        state_path,
    }
}

fn default_settings() -> AppSettings {
    AppSettings {
        output_dir: recommended_output_dir(),
        default_download_mode: "video".into(),
        default_playlist_scope: "video".into(),
        default_auth_mode: "none".into(),
        default_browser: "chrome".into(),
        default_cookie_file: String::new(),
        language: "zh-CN".into(),
    }
}

fn persistence_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("ytDownloader").join("state.json");
    }

    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".config")
        .join("ytDownloader")
        .join("state.json")
}

fn persist_state(state: &AppState) {
    let snapshot = PersistedState {
        tasks: state
            .tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone(),
        task_requests: state
            .task_requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone(),
        history: state
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone(),
        settings: state
            .settings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone(),
    };

    if let Some(parent) = state.state_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(serialized) = serde_json::to_string_pretty(&snapshot) {
        let _ = fs::write(&state.state_path, serialized);
    }
}

fn sanitize_settings(settings: AppSettings) -> AppSettings {
    AppSettings {
        output_dir: if settings.output_dir.trim().is_empty() {
            recommended_output_dir()
        } else {
            normalize_output_dir(&settings.output_dir)
        },
        default_download_mode: match settings.default_download_mode.as_str() {
            "video" | "audio" | "subtitles" | "video+subtitles" => settings.default_download_mode,
            _ => "video".into(),
        },
        default_playlist_scope: match settings.default_playlist_scope.as_str() {
            "video" | "playlist" => settings.default_playlist_scope,
            _ => "video".into(),
        },
        default_auth_mode: match settings.default_auth_mode.as_str() {
            "none" | "browser" | "file" => settings.default_auth_mode,
            _ => "none".into(),
        },
        default_browser: match settings.default_browser.as_str() {
            "chrome" | "chromium" | "edge" | "firefox" | "safari" | "brave" | "opera"
            | "vivaldi" | "whale" => settings.default_browser,
            _ => "chrome".into(),
        },
        default_cookie_file: settings.default_cookie_file.trim().to_string(),
        language: sanitize_language(&settings.language).into(),
    }
}

fn sanitize_language(language: &str) -> &'static str {
    match language.trim().to_ascii_lowercase().as_str() {
        "en" | "en-us" | "english" => "en-US",
        _ => "zh-CN",
    }
}

fn is_english(language: &str) -> bool {
    sanitize_language(language) == "en-US"
}

fn message_unfinished_after_restart(language: &str) -> String {
    if is_english(language) {
        "The app was restarted before this task finished. Incomplete tasks are not resumed automatically. Please retry manually.".into()
    } else {
        "应用重新启动后，未完成任务不会自动恢复。请手动重试。".into()
    }
}

#[tauri::command]
async fn parse_url(payload: ParseUrlRequest) -> Result<MediaPreview, String> {
    let language = payload.language.clone();
    tauri::async_runtime::spawn_blocking(move || parse_url_blocking(payload))
        .await
        .map_err(|error| {
            if is_english(&language) {
                format!("Failed to wait for the parsing task: {error}")
            } else {
                format!("等待解析任务结束失败：{error}")
            }
        })?
}

fn parse_url_blocking(payload: ParseUrlRequest) -> Result<MediaPreview, String> {
    let normalized = normalize_url(&payload.url, &payload.language)?;
    let root = match execute_parse_request(&normalized, &payload.playlist_scope, &payload) {
        Ok(root) => root,
        Err(error) => {
            if let Some(reference_root) = retry_parse_without_auth(&normalized, &payload, &error) {
                reference_root
            } else {
                return Err(error);
            }
        }
    };
    let mut preview = build_preview(&root, normalized.clone(), &payload.language);

    if preview.is_playlist && preview.formats.is_empty() {
        if let Ok(reference_root) = execute_parse_request(&normalized, "video", &payload) {
            let reference_formats = collect_formats(&reference_root, &payload.language);
            if !reference_formats.is_empty() {
                preview.formats = reference_formats;
            }
        } else if let Some(reference_root) = retry_parse_without_auth(&normalized, &payload, "") {
            let reference_formats = collect_formats(&reference_root, &payload.language);
            if !reference_formats.is_empty() {
                preview.formats = reference_formats;
            }
        }
    }

    Ok(preview)
}

fn execute_parse_request(
    normalized_url: &str,
    playlist_scope: &str,
    payload: &ParseUrlRequest,
) -> Result<Value, String> {
    let mut args = vec![
        "--dump-single-json".into(),
        "--skip-download".into(),
        "--no-warnings".into(),
        "--playlist-end".into(),
        "8".into(),
    ];

    apply_runtime_support_args(&mut args, normalized_url);
    apply_playlist_scope_args(&mut args, playlist_scope);
    apply_auth_args(
        &mut args,
        &payload.auth_mode,
        payload.browser.as_deref(),
        payload.cookie_file.as_deref(),
        &payload.language,
    )?;

    let mut command = binary_command("yt-dlp");
    command.args(&args);
    command.arg(normalized_url);
    let output = command.output().map_err(|error| {
        if is_english(&payload.language) {
            format!("Failed to start yt-dlp: {error}")
        } else {
            format!("无法启动 yt-dlp：{error}")
        }
    })?;

    if !output.status.success() {
        return Err(normalize_parse_error(
            command_error(&output.stderr, &output.stdout),
            &payload.auth_mode,
            normalized_url,
            &payload.language,
        ));
    }

    let json = String::from_utf8(output.stdout).map_err(|error| {
        if is_english(&payload.language) {
            format!("yt-dlp returned invalid JSON: {error}")
        } else {
            format!("yt-dlp 返回了无效 JSON：{error}")
        }
    })?;

    serde_json::from_str(&json).map_err(|error| {
        if is_english(&payload.language) {
            format!("Failed to parse yt-dlp JSON: {error}")
        } else {
            format!("解析 yt-dlp JSON 失败：{error}")
        }
    })
}

fn retry_parse_without_auth(
    normalized_url: &str,
    payload: &ParseUrlRequest,
    previous_error: &str,
) -> Option<Value> {
    if !should_retry_without_auth(normalized_url, &payload.auth_mode, previous_error) {
        return None;
    }

    let no_auth_payload = without_auth_parse_payload(payload);
    execute_parse_request(normalized_url, &payload.playlist_scope, &no_auth_payload).ok()
}

#[tauri::command]
fn get_tasks(state: State<AppState>) -> Vec<DownloadTask> {
    let tasks = state
        .tasks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut values: Vec<DownloadTask> = tasks.values().cloned().collect();

    values.sort_by(|left, right| right.id.cmp(&left.id));
    values
}

#[tauri::command]
fn get_history(state: State<AppState>) -> Vec<HistoryItem> {
    state
        .history
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> AppSettings {
    state
        .settings
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

#[tauri::command]
fn save_settings(state: State<AppState>, payload: AppSettings) -> Result<AppSettings, String> {
    let sanitized = sanitize_settings(payload);

    {
        let mut settings = state
            .settings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *settings = sanitized.clone();
    }

    persist_state(&state);
    Ok(sanitized)
}

#[tauri::command]
fn start_download(
    app: AppHandle,
    state: State<AppState>,
    payload: StartDownloadRequest,
) -> Result<DownloadTask, String> {
    enqueue_download(app, &state, payload)
}

#[tauri::command]
fn retry_download(
    app: AppHandle,
    state: State<AppState>,
    task_id: String,
) -> Result<DownloadTask, String> {
    let request = {
        let requests = state
            .task_requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        requests.get(&task_id).cloned().ok_or_else(|| {
            if is_english(&request_language_from_state(&state, &task_id)) {
                "No retryable task configuration was found".to_string()
            } else {
                "未找到可重试的任务配置".to_string()
            }
        })?
    };

    let next_task = enqueue_download(app.clone(), &state, request)?;
    remove_task_entries(&state, &task_id);
    persist_state(&state);
    emit_task_removed(&app, &task_id);

    Ok(next_task)
}

#[tauri::command]
fn cancel_download(state: State<AppState>, task_id: String) -> Result<DownloadTask, String> {
    let language = request_language_from_state(&state, &task_id);
    {
        let mut cancelled = state
            .cancelled_tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cancelled.insert(task_id.clone());
    }

    if let Some(pid) = take_task_pid(&state.task_pids, &task_id) {
        let terminated = terminate_process(pid).map_err(|error| {
            if is_english(&language) {
                format!("Failed to cancel the task: {error}")
            } else {
                format!("取消任务失败：{error}")
            }
        })?;

        if !terminated {
            return Err(if is_english(&language) {
                "Failed to cancel the task because the download process could not be terminated"
                    .into()
            } else {
                "取消任务失败，无法终止下载进程".into()
            });
        }
    }

    let mut task = get_task(&state.tasks, &task_id).ok_or_else(|| {
        if is_english(&language) {
            "Task not found".to_string()
        } else {
            "未找到任务".to_string()
        }
    })?;
    task.status = "cancelled".into();
    task.error = None;
    upsert_task(&state.tasks, &task);
    persist_state(&state);

    Ok(task)
}

#[tauri::command]
fn clear_tasks(state: State<AppState>, scope: String) -> Result<Vec<DownloadTask>, String> {
    let removable_tasks = {
        let tasks = state
            .tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        tasks
            .values()
            .filter(|task| match scope.as_str() {
                "completed" => task.status == "done",
                "failed" => task.status == "failed" || task.status == "cancelled",
                "all" => true,
                _ => false,
            })
            .cloned()
            .collect::<Vec<_>>()
    };

    if scope != "completed" && scope != "failed" && scope != "all" {
        return Err("未知的清理范围".into());
    }

    for task in removable_tasks
        .iter()
        .filter(|task| matches!(task.status.as_str(), "queued" | "running"))
    {
        {
            let mut cancelled = state
                .cancelled_tasks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            cancelled.insert(task.id.clone());
        }

        if let Some(pid) = take_task_pid(&state.task_pids, &task.id) {
            let _ = terminate_process(pid);
        }
    }

    let removable_ids = removable_tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();

    {
        let mut tasks = state
            .tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for id in &removable_ids {
            tasks.remove(id);
        }
    }

    {
        let mut requests = state
            .task_requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for id in &removable_ids {
            requests.remove(id);
        }
    }

    {
        let mut pids = state
            .task_pids
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for id in &removable_ids {
            pids.remove(id);
        }
    }

    {
        let mut cancelled = state
            .cancelled_tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for id in &removable_ids {
            cancelled.remove(id);
        }
    }

    persist_state(&state);
    Ok(get_tasks(state))
}

fn enqueue_download(
    app: AppHandle,
    state: &State<AppState>,
    payload: StartDownloadRequest,
) -> Result<DownloadTask, String> {
    let url = normalize_url(&payload.url, &payload.language)?;
    let output_dir = normalize_output_dir(&payload.output_dir);

    fs::create_dir_all(&output_dir).map_err(|error| {
        if is_english(&payload.language) {
            format!("Failed to create output directory `{output_dir}`: {error}")
        } else {
            format!("无法创建下载目录 `{output_dir}`：{error}")
        }
    })?;

    let generated_task_id = task_id();

    let task = DownloadTask {
        id: generated_task_id.clone(),
        title: payload
            .title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| infer_title_from_url(&url, &payload.language)),
        status: "queued".into(),
        progress: 0.0,
        speed: "--".into(),
        eta: "--".into(),
        output: output_dir.clone(),
        profile: build_profile_label(
            &payload.mode,
            payload.format_id.as_deref(),
            &payload.auth_mode,
            &payload.language,
        ),
        source_url: url.clone(),
        error: None,
    };

    upsert_task(&state.tasks, &task);
    emit_task_update(&app, &task);
    store_task_request(&state.task_requests, &generated_task_id, &payload);
    clear_task_cancelled(&state.cancelled_tasks, &generated_task_id);
    persist_state(state);

    let primary_args = build_download_args(&payload, &output_dir, &url)?;
    let fallback_args = build_fallback_download_args(&payload, &output_dir, &url)?;
    let no_auth_payload = without_auth_download_payload(&payload);
    let no_auth_primary_args = build_download_args(&no_auth_payload, &output_dir, &url)?;
    let no_auth_fallback_args = build_fallback_download_args(&no_auth_payload, &output_dir, &url)?;
    let original_auth_mode = payload.auth_mode.clone();
    let payload_language = payload.language.clone();

    let app_handle = app.clone();
    let app_state = state.inner().clone();
    let task_store = state.tasks.clone();
    let task_pids = state.task_pids.clone();
    let cancelled_tasks = state.cancelled_tasks.clone();
    let download_gate = state.download_gate.clone();
    let thread_url = url.clone();
    let thread_task_id = generated_task_id.clone();
    thread::spawn(move || {
        let mut current_task = task;
        let _permit = acquire_download_permit(&download_gate);

        if get_task(&task_store, &thread_task_id).is_none() {
            remove_task_runtime_entries(&app_state, &thread_task_id);
            return;
        }

        if take_task_cancelled(&cancelled_tasks, &thread_task_id) {
            current_task.status = "cancelled".into();
            current_task.error = None;
            current_task.speed = "--".into();
            current_task.eta = "--".into();
            if upsert_task_if_present(&task_store, &current_task) {
                emit_task_update(&app_handle, &current_task);
                persist_state(&app_state);
            }
            return;
        }

        current_task.status = "running".into();
        if upsert_task_if_present(&task_store, &current_task) {
            emit_task_update(&app_handle, &current_task);
        } else {
            remove_task_runtime_entries(&app_state, &thread_task_id);
            return;
        }

        let first_attempt = run_download_attempt(
            &app_handle,
            &task_store,
            &task_pids,
            &mut current_task,
            &thread_url,
            &thread_task_id,
            &payload_language,
            primary_args,
        );

        let final_result = if should_retry_with_fallback(&first_attempt.error) {
            current_task.error = Some(message_primary_fallback(&payload_language));
            if !upsert_task_if_present(&task_store, &current_task) {
                remove_task_runtime_entries(&app_state, &thread_task_id);
                return;
            }
            emit_task_update(&app_handle, &current_task);

            current_task.progress = 0.0;
            current_task.speed = "--".into();
            current_task.eta = "--".into();

            run_download_attempt(
                &app_handle,
                &task_store,
                &task_pids,
                &mut current_task,
                &thread_url,
                &thread_task_id,
                &payload_language,
                fallback_args,
            )
        } else {
            first_attempt
        };

        let final_result = if should_retry_without_auth(
            &thread_url,
            &original_auth_mode,
            final_result.error.as_deref().unwrap_or(""),
        ) {
            current_task.error = Some(message_retry_without_auth(&payload_language));
            current_task.progress = 0.0;
            current_task.speed = "--".into();
            current_task.eta = "--".into();
            if !upsert_task_if_present(&task_store, &current_task) {
                remove_task_runtime_entries(&app_state, &thread_task_id);
                return;
            }
            emit_task_update(&app_handle, &current_task);

            let retried = run_download_attempt(
                &app_handle,
                &task_store,
                &task_pids,
                &mut current_task,
                &thread_url,
                &thread_task_id,
                &payload_language,
                no_auth_primary_args,
            );

            if should_retry_with_fallback(&retried.error) {
                current_task.error = Some(message_no_auth_fallback(&payload_language));
                current_task.progress = 0.0;
                current_task.speed = "--".into();
                current_task.eta = "--".into();
                if !upsert_task_if_present(&task_store, &current_task) {
                    remove_task_runtime_entries(&app_state, &thread_task_id);
                    return;
                }
                emit_task_update(&app_handle, &current_task);

                run_download_attempt(
                    &app_handle,
                    &task_store,
                    &task_pids,
                    &mut current_task,
                    &thread_url,
                    &thread_task_id,
                    &payload_language,
                    no_auth_fallback_args,
                )
            } else {
                retried
            }
        } else {
            final_result
        };

        let was_cancelled = take_task_cancelled(&cancelled_tasks, &thread_task_id);

        if get_task(&task_store, &thread_task_id).is_none() {
            remove_task_runtime_entries(&app_state, &thread_task_id);
            return;
        }

        if was_cancelled {
            current_task.status = "cancelled".into();
            current_task.error = None;
            current_task.speed = "--".into();
            current_task.eta = "--".into();
            if upsert_task_if_present(&task_store, &current_task) {
                emit_task_update(&app_handle, &current_task);
                persist_state(&app_state);
            }
        } else if final_result.success {
            current_task.status = "done".into();
            current_task.progress = 100.0;
            current_task.speed = "--".into();
            current_task.eta = "00:00".into();
            current_task.error = None;
            if upsert_task_if_present(&task_store, &current_task) {
                emit_task_update(&app_handle, &current_task);
                record_history(&app_state, &app_handle, &current_task);
                persist_state(&app_state);
            }
        } else {
            current_task.status = "failed".into();
            if current_task.error.is_none() {
                current_task.error = final_result.error;
            }
            if upsert_task_if_present(&task_store, &current_task) {
                emit_task_update(&app_handle, &current_task);
                persist_state(&app_state);
            }
        }
    });

    Ok(
        get_task(&state.tasks, &generated_task_id).unwrap_or_else(|| DownloadTask {
            id: generated_task_id,
            title: payload
                .title
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| infer_title_from_url(&url, &payload.language)),
            status: "queued".into(),
            progress: 0.0,
            speed: "--".into(),
            eta: "--".into(),
            output: output_dir,
            profile: build_profile_label(
                &payload.mode,
                payload.format_id.as_deref(),
                &payload.auth_mode,
                &payload.language,
            ),
            source_url: url,
            error: None,
        }),
    )
}

fn build_preview(root: &Value, source_url: String, language: &str) -> MediaPreview {
    let playlist_entries = root
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .enumerate()
                .map(|(index, entry)| PlaylistEntry {
                    index: index + 1,
                    title: string_from(entry, &["title", "id"], "Untitled playlist item".into()),
                    duration: duration_label(entry.get("duration")),
                    source_url: playlist_entry_url(entry),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let is_playlist = !playlist_entries.is_empty();

    MediaPreview {
        title: string_from(root, &["title", "id"], "Untitled media".into()),
        creator: string_from(
            root,
            &["channel", "uploader", "playlist_uploader", "extractor"],
            "Unknown creator".into(),
        ),
        duration: duration_label(root.get("duration")),
        platform: string_from(
            root,
            &["extractor_key", "extractor", "webpage_url_domain"],
            "Unknown".into(),
        ),
        published_at: publish_label(root),
        thumbnail: thumbnail_url(root),
        formats: collect_formats(root, language),
        subtitles: collect_subtitles(root),
        playlist_entries,
        source_url,
        is_playlist,
        total_entries: root
            .get("playlist_count")
            .and_then(Value::as_u64)
            .map(|count| count as usize)
            .or_else(|| {
                root.get("entries")
                    .and_then(Value::as_array)
                    .map(|entries| entries.len())
            })
            .unwrap_or(1),
    }
}

fn collect_formats(root: &Value, language: &str) -> Vec<PreviewFormat> {
    root.get("formats")
        .and_then(Value::as_array)
        .map(|formats| {
            let mut items = formats
                .iter()
                .filter(|item| item.get("ext").and_then(Value::as_str).is_some())
                .filter_map(|item| {
                    let format_id = item.get("format_id").and_then(Value::as_str)?.to_string();
                    let ext = item.get("ext").and_then(Value::as_str)?;
                    let acodec = item
                        .get("acodec")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let height = item.get("height").and_then(Value::as_u64);
                    let resolution = height
                        .map(|value| format!("{value}p"))
                        .or_else(|| {
                            item.get("format_note")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .unwrap_or_else(|| {
                            if is_english(language) {
                                "Original".into()
                            } else {
                                "原始格式".into()
                            }
                        });
                    let vcodec = item
                        .get("vcodec")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let kind = if vcodec == "none" {
                        "audio"
                    } else if acodec == "none" {
                        "video"
                    } else {
                        "combined"
                    };

                    if vcodec == "images" || format_id.starts_with("sb") {
                        return None;
                    }

                    let detail = match kind {
                        "audio" => {
                            if is_english(language) {
                                format!("Audio Only / {}", acodec.to_uppercase())
                            } else {
                                format!("仅音频 / {}", acodec.to_uppercase())
                            }
                        }
                        "video" => {
                            if is_english(language) {
                                format!(
                                    "Video Only / {} / Auto-pair best audio",
                                    ext.to_uppercase()
                                )
                            } else {
                                format!("仅视频 / {} / 自动搭配最佳音频", ext.to_uppercase())
                            }
                        }
                        _ => {
                            if is_english(language) {
                                format!("Muxed / {}", ext.to_uppercase())
                            } else {
                                format!("音画合流 / {}", ext.to_uppercase())
                            }
                        }
                    };

                    let download_selector = match kind {
                        "video" => format!(
                            "{format_id}+bestaudio[acodec^=mp4a]/bestaudio[ext=m4a]/bestaudio/best"
                        ),
                        _ => format_id.clone(),
                    };

                    Some(PreviewFormat {
                        format_id,
                        download_selector,
                        label: format!("{resolution} {}", ext.to_uppercase()),
                        detail,
                        size: byte_label(
                            item.get("filesize")
                                .and_then(Value::as_u64)
                                .or_else(|| item.get("filesize_approx").and_then(Value::as_u64)),
                            language,
                        ),
                        kind: kind.into(),
                    })
                })
                .collect::<Vec<_>>();

            items.sort_by(|left, right| compare_preview_formats(left, right));
            items
        })
        .unwrap_or_default()
}

fn collect_subtitles(root: &Value) -> Vec<PreviewSubtitle> {
    let manual = root
        .get("subtitles")
        .and_then(Value::as_object)
        .map(|object| subtitle_entries(object, "manual"))
        .unwrap_or_default();
    let automatic = root
        .get("automatic_captions")
        .and_then(Value::as_object)
        .map(|object| subtitle_entries(object, "auto"))
        .unwrap_or_default();

    manual.into_iter().chain(automatic).take(8).collect()
}

fn compare_preview_formats(left: &PreviewFormat, right: &PreviewFormat) -> std::cmp::Ordering {
    preview_format_rank(right)
        .cmp(&preview_format_rank(left))
        .then_with(|| preview_resolution(right).cmp(&preview_resolution(left)))
        .then_with(|| left.label.cmp(&right.label))
}

fn preview_format_rank(format: &PreviewFormat) -> i32 {
    match format.kind.as_str() {
        "video" => 3,
        "combined" => 2,
        "audio" => 1,
        _ => 0,
    }
}

fn preview_resolution(format: &PreviewFormat) -> i32 {
    format
        .label
        .split_whitespace()
        .find_map(|part| part.strip_suffix('p'))
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0)
}

fn subtitle_entries(
    map: &serde_json::Map<String, Value>,
    subtitle_type: &str,
) -> Vec<PreviewSubtitle> {
    map.iter()
        .filter_map(|(language, value)| {
            let first = value.as_array()?.first()?;
            let format = first
                .get("ext")
                .and_then(Value::as_str)
                .unwrap_or("vtt")
                .to_string();

            Some(PreviewSubtitle {
                language: language.to_string(),
                subtitle_type: subtitle_type.into(),
                format,
            })
        })
        .collect()
}

fn publish_label(root: &Value) -> String {
    ["upload_date", "release_date", "modified_date"]
        .iter()
        .find_map(|key| root.get(*key).and_then(Value::as_str))
        .map(format_date)
        .unwrap_or_else(|| "Unknown".into())
}

fn thumbnail_url(root: &Value) -> String {
    root.get("thumbnail")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            root.get("thumbnails")
                .and_then(Value::as_array)
                .and_then(|items| items.last())
                .and_then(|item| item.get("url"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| {
            "https://images.unsplash.com/photo-1498050108023-c5249f4df085?auto=format&fit=crop&w=1200&q=80"
                .into()
        })
}

fn playlist_entry_url(entry: &Value) -> String {
    entry
        .get("webpage_url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            entry.get("url").and_then(Value::as_str).map(|value| {
                if value.starts_with("http://") || value.starts_with("https://") {
                    value.to_string()
                } else {
                    format!("https://www.youtube.com/watch?v={value}")
                }
            })
        })
        .unwrap_or_default()
}

fn normalize_url(url: &str, language: &str) -> Result<String, String> {
    let normalized = url.trim();

    if normalized.is_empty() {
        return Err(if is_english(language) {
            "Enter at least one valid link first".into()
        } else {
            "请先输入至少一个有效链接".into()
        });
    }

    Ok(normalized.into())
}

fn normalize_output_dir(path: &str) -> String {
    let trimmed = path.trim();

    if trimmed.is_empty() {
        return recommended_output_dir();
    }

    expand_home_path(trimmed)
}

fn build_download_args(
    payload: &StartDownloadRequest,
    output_dir: &str,
    url: &str,
) -> Result<Vec<String>, String> {
    let mut args = base_download_args(output_dir, url);

    apply_auth_args(
        &mut args,
        &payload.auth_mode,
        payload.browser.as_deref(),
        payload.cookie_file.as_deref(),
        &payload.language,
    )?;
    apply_playlist_scope_args(&mut args, &payload.playlist_scope);

    match payload.mode.as_str() {
        "audio" => {
            args.extend([
                "-f".into(),
                payload
                    .format_id
                    .clone()
                    .unwrap_or_else(|| "bestaudio/best".into()),
                "--extract-audio".into(),
                "--audio-format".into(),
                "mp3".into(),
                "--audio-quality".into(),
                "0".into(),
            ]);
        }
        "subtitles" => {
            args.extend([
                "--skip-download".into(),
                "--write-subs".into(),
                "--write-auto-subs".into(),
                "--sub-langs".into(),
                "all".into(),
                "--convert-subs".into(),
                "srt".into(),
            ]);
        }
        "video+subtitles" => {
            args.extend(selected_or_default_format(
                payload.format_id.as_deref(),
                "bv*+bestaudio[acodec^=mp4a]/bestaudio[ext=m4a]/bestaudio/best",
            ));
            args.extend([
                "--write-subs".into(),
                "--write-auto-subs".into(),
                "--sub-langs".into(),
                "all".into(),
                "--embed-subs".into(),
            ]);
        }
        _ => {
            args.extend(selected_or_default_format(
                payload.format_id.as_deref(),
                "bv*+bestaudio[acodec^=mp4a]/bestaudio[ext=m4a]/bestaudio/best",
            ));
        }
    }

    Ok(args)
}

fn build_fallback_download_args(
    payload: &StartDownloadRequest,
    output_dir: &str,
    url: &str,
) -> Result<Vec<String>, String> {
    let mut args = base_download_args(output_dir, url);

    apply_auth_args(
        &mut args,
        &payload.auth_mode,
        payload.browser.as_deref(),
        payload.cookie_file.as_deref(),
        &payload.language,
    )?;
    apply_playlist_scope_args(&mut args, &payload.playlist_scope);

    match payload.mode.as_str() {
        "audio" => {
            args.extend([
                "-f".into(),
                "bestaudio/best".into(),
                "--extract-audio".into(),
                "--audio-format".into(),
                "mp3".into(),
                "--audio-quality".into(),
                "0".into(),
            ]);
        }
        "subtitles" => {
            args.extend([
                "--skip-download".into(),
                "--write-subs".into(),
                "--write-auto-subs".into(),
                "--sub-langs".into(),
                "all".into(),
                "--convert-subs".into(),
                "srt".into(),
            ]);
        }
        "video+subtitles" => {
            args.extend([
                "-f".into(),
                "bv*+bestaudio[acodec^=mp4a]/bestaudio[ext=m4a]/bestaudio/best".into(),
                "--write-subs".into(),
                "--write-auto-subs".into(),
                "--sub-langs".into(),
                "all".into(),
            ]);
        }
        _ => {
            args.extend([
                "-f".into(),
                "bv*+bestaudio[acodec^=mp4a]/bestaudio[ext=m4a]/bestaudio/best".into(),
            ]);
        }
    }

    Ok(args)
}

fn base_download_args(output_dir: &str, url: &str) -> Vec<String> {
    let mut args = vec![
        "--newline".into(),
        "-P".into(),
        output_dir.into(),
        "-o".into(),
        "%(title)s [%(id)s].%(ext)s".into(),
        "--progress".into(),
        "--no-warnings".into(),
    ];

    apply_runtime_support_args(&mut args, url);
    args
}

fn selected_or_default_format(selected: Option<&str>, fallback: &str) -> Vec<String> {
    vec!["-f".into(), selected.unwrap_or(fallback).to_string()]
}

fn build_profile_label(
    mode: &str,
    format_id: Option<&str>,
    auth_mode: &str,
    language: &str,
) -> String {
    let mode_label = if is_english(language) {
        match mode {
            "audio" => "Audio Only",
            "subtitles" => "Subtitles Only",
            "video+subtitles" => "Video + Subtitles",
            _ => "Video",
        }
    } else {
        match mode {
            "audio" => "仅音频",
            "subtitles" => "仅字幕",
            "video+subtitles" => "视频 + 字幕",
            _ => "视频",
        }
    };
    let auth_label = if is_english(language) {
        match auth_mode {
            "browser" => "Browser Cookies",
            "file" => "Cookie File",
            _ => "No Cookies",
        }
    } else {
        match auth_mode {
            "browser" => "浏览器 Cookie",
            "file" => "Cookie 文件",
            _ => "无 Cookie",
        }
    };
    let format_label = if is_english(language) {
        format_id.unwrap_or("Auto Best")
    } else {
        format_id.unwrap_or("自动最佳")
    };

    format!("{mode_label} / {format_label} / {auth_label}")
}

fn run_download_attempt(
    app: &AppHandle,
    store: &Arc<Mutex<HashMap<String, DownloadTask>>>,
    task_pids: &Arc<Mutex<HashMap<String, u32>>>,
    task: &mut DownloadTask,
    url: &str,
    task_id: &str,
    language: &str,
    args: Vec<String>,
) -> DownloadAttemptResult {
    let mut command = binary_command("yt-dlp");
    command.args(args);
    command.arg(url);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return DownloadAttemptResult {
                success: false,
                error: Some(if is_english(language) {
                    format!("Failed to start the download task: {error}")
                } else {
                    format!("无法启动下载任务：{error}")
                }),
            };
        }
    };

    store_task_pid(task_pids, task_id, child.id());

    let shared_task = Arc::new(Mutex::new(task.clone()));
    let stdout_handle = child.stdout.take().map(|stdout| {
        spawn_download_output_reader(app.clone(), store.clone(), Arc::clone(&shared_task), stdout)
    });
    let stderr_handle = child.stderr.take().map(|stderr| {
        spawn_download_output_reader(app.clone(), store.clone(), Arc::clone(&shared_task), stderr)
    });

    let wait_result = child.wait();

    if let Some(handle) = stdout_handle {
        let _ = handle.join();
    }

    if let Some(handle) = stderr_handle {
        let _ = handle.join();
    }

    remove_task_pid(task_pids, task_id);

    *task = shared_task
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();

    match wait_result {
        Ok(status) if status.success() => DownloadAttemptResult {
            success: true,
            error: None,
        },
        Ok(status) => DownloadAttemptResult {
            success: false,
            error: task.error.clone().or_else(|| {
                Some(if is_english(language) {
                    format!(
                        "yt-dlp exited with an unexpected status code: {}",
                        status.code().unwrap_or(-1)
                    )
                } else {
                    format!("yt-dlp 退出码异常：{}", status.code().unwrap_or(-1))
                })
            }),
        },
        Err(error) => DownloadAttemptResult {
            success: false,
            error: Some(if is_english(language) {
                format!("Failed to wait for the download process: {error}")
            } else {
                format!("等待下载进程结束失败：{error}")
            }),
        },
    }
}

fn should_retry_with_fallback(error: &Option<String>) -> bool {
    error
        .as_ref()
        .map(|message| {
            message.contains("Requested format is not available")
                || message.contains("requested format not available")
        })
        .unwrap_or(false)
}

fn spawn_download_output_reader<R: Read + Send + 'static>(
    app: AppHandle,
    store: Arc<Mutex<HashMap<String, DownloadTask>>>,
    task: Arc<Mutex<DownloadTask>>,
    reader: R,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let reader = BufReader::new(reader);

        for line in reader.lines().map_while(Result::ok) {
            process_download_output_line(&app, &store, &task, &line);
        }
    })
}

fn process_download_output_line(
    app: &AppHandle,
    store: &Arc<Mutex<HashMap<String, DownloadTask>>>,
    task: &Arc<Mutex<DownloadTask>>,
    line: &str,
) {
    let trimmed = line.trim().to_string();

    if trimmed.is_empty() {
        return;
    }

    let mut current = task.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(path) = extract_output_path(&trimmed) {
        current.output = path;
        if let Some(title) = title_from_output_path(&current.output) {
            current.title = title;
        }
        if upsert_task_if_present(store, &current) {
            emit_task_update(app, &current);
        }
        return;
    }

    if let Some(progress) = parse_progress(&trimmed) {
        current.progress = progress.progress;
        current.speed = progress.speed;
        current.eta = progress.eta;
        if upsert_task_if_present(store, &current) {
            emit_task_update(app, &current);
        }
        return;
    }

    if trimmed.contains("ERROR:") {
        current.error = Some(
            trimmed
                .split("ERROR:")
                .nth(1)
                .unwrap_or(trimmed.as_str())
                .trim()
                .to_string(),
        );
        if upsert_task_if_present(store, &current) {
            emit_task_update(app, &current);
        }
    }
}

fn parse_progress(line: &str) -> Option<ProgressUpdate> {
    if !line.starts_with("[download]") || !line.contains('%') || line.contains("Destination") {
        return None;
    }

    let after_prefix = line.trim_start_matches("[download]").trim();
    let progress_text = after_prefix.split('%').next()?.trim();
    let progress = progress_text.parse::<f32>().ok()?;

    let speed = if let Some(at_part) = after_prefix.split(" at ").nth(1) {
        at_part
            .split_whitespace()
            .next()
            .unwrap_or("--")
            .to_string()
    } else {
        "--".into()
    };

    let eta = if let Some(eta_part) = after_prefix.split(" ETA ").nth(1) {
        eta_part
            .split_whitespace()
            .next()
            .unwrap_or("--")
            .to_string()
    } else {
        "--".into()
    };

    Some(ProgressUpdate {
        progress,
        speed,
        eta,
    })
}

fn extract_output_path(line: &str) -> Option<String> {
    [
        "[download] Destination: ",
        "[ExtractAudio] Destination: ",
        "[Merger] Merging formats into ",
    ]
    .iter()
    .find_map(|prefix| line.strip_prefix(prefix))
    .map(|value| value.trim_matches('"').to_string())
}

fn title_from_output_path(path: &str) -> Option<String> {
    let file_name = std::path::Path::new(path).file_stem()?.to_str()?.trim();
    let cleaned = file_name
        .rsplit_once(" [")
        .map(|(title, _)| title)
        .unwrap_or(file_name)
        .trim();

    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn duration_label(value: Option<&Value>) -> String {
    value
        .and_then(|item| item.as_f64())
        .map(|seconds| {
            let total = seconds.round() as u64;
            let hours = total / 3600;
            let minutes = (total % 3600) / 60;
            let secs = total % 60;

            if hours > 0 {
                format!("{hours:02}:{minutes:02}:{secs:02}")
            } else {
                format!("{minutes:02}:{secs:02}")
            }
        })
        .unwrap_or_else(|| "--:--".into())
}

fn byte_label(value: Option<u64>, language: &str) -> String {
    value
        .map(|bytes| {
            const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
            let mut unit_index = 0usize;
            let mut size = bytes as f64;

            while size >= 1024.0 && unit_index < UNITS.len() - 1 {
                size /= 1024.0;
                unit_index += 1;
            }

            format!("{size:.1} {}", UNITS[unit_index])
        })
        .unwrap_or_else(|| {
            if is_english(language) {
                "Unknown size".into()
            } else {
                "大小未知".into()
            }
        })
}

fn format_date(raw: &str) -> String {
    if raw.len() == 8 && raw.chars().all(|char| char.is_ascii_digit()) {
        return format!("{}-{}-{}", &raw[0..4], &raw[4..6], &raw[6..8]);
    }

    raw.into()
}

fn string_from(value: &Value, keys: &[&str], fallback: String) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or(fallback)
}

fn first_non_empty_line(buffer: &[u8]) -> Option<String> {
    String::from_utf8_lossy(buffer)
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

fn command_error(stderr: &[u8], stdout: &[u8]) -> String {
    first_non_empty_line(stderr)
        .or_else(|| first_non_empty_line(stdout))
        .unwrap_or_else(|| "yt-dlp 执行失败，但没有返回可读错误".into())
}

fn normalize_parse_error(error: String, auth_mode: &str, url: &str, language: &str) -> String {
    if error.contains("Sign in to confirm you’re not a bot") {
        return if is_english(language) {
            "This link requires a signed-in session or browser cookies. Switch to Browser Cookies or import a cookie file and try again.".into()
        } else {
            "当前链接需要登录态或浏览器 Cookie。请切换到“从浏览器读取”或导入 Cookie 文件后重试。"
                .into()
        };
    }

    if auth_mode != "none" && error.contains("Requested format is not available") {
        if preferred_js_runtime() == Some("node") {
            return if is_english(language) {
                "Browser-cookie parsing triggered a YouTube login challenge, but only Node.js was detected on this machine and no downloadable formats could be resolved. Install Deno or Bun so the app can prefer it automatically, or retry without cookies / with a cookie file.".into()
            } else {
                "当前浏览器 Cookie 解析触发了 YouTube 登录挑战，但本机仅检测到 Node.js，未能解出可下载格式。请先安装 Deno 或 Bun，应用会自动优先使用；或者改用不使用 Cookie / 导入 Cookie 文件再试。"
                    .into()
            };
        }

        if is_youtube_url(url) {
            return if is_english(language) {
                "Browser cookies are enabled and yt-dlp EJS remote components were also attempted, but the YouTube login challenge still did not resolve into downloadable formats. This is more likely a yt-dlp challenge compatibility issue for this link than a GUI parameter problem.".into()
            } else {
                "当前已启用浏览器 Cookie，并已尝试加载 yt-dlp 的 EJS 远程组件，但 YouTube 登录挑战仍未解出可下载格式。更可能是 yt-dlp 当前对这条链接的 challenge 兼容问题，而不是界面参数配置错误。"
                    .into()
            };
        }
    }

    error
}

fn should_retry_without_auth(url: &str, auth_mode: &str, error: &str) -> bool {
    if auth_mode == "none" || !is_youtube_url(url) {
        return false;
    }

    error.is_empty()
        || error.contains("Requested format is not available")
        || error.contains("requested format not available")
        || error.contains("Sign in to confirm you’re not a bot")
        || error.contains("当前已启用浏览器 Cookie")
        || error.contains("当前浏览器 Cookie 解析触发了 YouTube 登录挑战")
        || error.contains("当前链接需要登录态或浏览器 Cookie")
}

fn without_auth_parse_payload(payload: &ParseUrlRequest) -> ParseUrlRequest {
    ParseUrlRequest {
        url: payload.url.clone(),
        playlist_scope: payload.playlist_scope.clone(),
        auth_mode: "none".into(),
        browser: None,
        cookie_file: None,
        language: payload.language.clone(),
    }
}

fn without_auth_download_payload(payload: &StartDownloadRequest) -> StartDownloadRequest {
    StartDownloadRequest {
        url: payload.url.clone(),
        title: payload.title.clone(),
        mode: payload.mode.clone(),
        format_id: payload.format_id.clone(),
        output_dir: payload.output_dir.clone(),
        playlist_scope: payload.playlist_scope.clone(),
        auth_mode: "none".into(),
        browser: None,
        cookie_file: None,
        language: payload.language.clone(),
    }
}

fn apply_auth_args(
    args: &mut Vec<String>,
    auth_mode: &str,
    browser: Option<&str>,
    cookie_file: Option<&str>,
    language: &str,
) -> Result<(), String> {
    match auth_mode {
        "browser" => {
            let browser_name = browser
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    if is_english(language) {
                        "Browser-cookie authentication was selected, but no browser was chosen"
                            .to_string()
                    } else {
                        "认证模式为浏览器 Cookie，但没有选择浏览器".to_string()
                    }
                })?;

            args.push("--cookies-from-browser".into());
            args.push(browser_name.into());
            Ok(())
        }
        "file" => {
            let file_path = cookie_file
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(expand_home_path)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    if is_english(language) {
                        "Cookie-file authentication was selected, but no file path was provided"
                            .to_string()
                    } else {
                        "认证模式为 Cookie 文件，但没有填写文件路径".to_string()
                    }
                })?;

            args.push("--cookies".into());
            args.push(file_path);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn apply_playlist_scope_args(args: &mut Vec<String>, playlist_scope: &str) {
    if playlist_scope == "video" {
        args.push("--no-playlist".into());
    }
}

fn apply_runtime_support_args(args: &mut Vec<String>, url: &str) {
    apply_js_runtime_args(args);
    apply_remote_component_args(args, url);
}

fn apply_js_runtime_args(args: &mut Vec<String>) {
    if let Some(runtime) = preferred_js_runtime() {
        args.push("--js-runtimes".into());
        args.push(runtime.into());
    }
}

fn apply_remote_component_args(args: &mut Vec<String>, url: &str) {
    if is_youtube_url(url) {
        args.push("--remote-components".into());
        args.push("ejs:github".into());
    }
}

fn terminate_process(pid: u32) -> std::io::Result<bool> {
    terminate_process_command(pid).map(|status| status.success())
}

#[cfg(target_os = "windows")]
fn terminate_process_command(pid: u32) -> std::io::Result<std::process::ExitStatus> {
    Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status()
}

#[cfg(not(target_os = "windows"))]
fn terminate_process_command(pid: u32) -> std::io::Result<std::process::ExitStatus> {
    Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
}

fn preferred_js_runtime() -> Option<&'static str> {
    if command_exists("deno", &["--version"]) {
        return Some("deno");
    }

    if command_exists("bun", &["--version"]) {
        return Some("bun");
    }

    if command_exists("qjs", &["--version"]) {
        return Some("qjs");
    }

    if command_exists("node", &["--version"]) {
        return Some("node");
    }

    None
}

fn command_exists(binary: &str, args: &[&str]) -> bool {
    binary_output(binary, args)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn binary_command(binary: &str) -> Command {
    #[cfg(target_os = "macos")]
    if binary == "yt-dlp" {
        if let Some(script_path) = resolve_binary_path(binary) {
            if let Some(python_path) = resolve_python_path() {
                let mut command = Command::new(python_path);
                if let Some(runtime_root) = resolve_python_runtime_root() {
                    command.env("PYTHONHOME", runtime_root);
                    command.env("PYTHONNOUSERSITE", "1");
                }
                command.arg(script_path);
                return command;
            }
        }
    }

    let mut command = resolve_binary_path(binary)
        .map(Command::new)
        .unwrap_or_else(|| Command::new(binary));

    apply_binary_runtime_env(binary, &mut command);
    command
}

fn binary_output(binary: &str, args: &[impl AsRef<str>]) -> std::io::Result<Output> {
    let mut command = binary_command(binary);

    for arg in args {
        command.arg(arg.as_ref());
    }

    command.output()
}

fn resolve_binary_path(binary: &str) -> Option<PathBuf> {
    bundled_binary_candidates(binary)
        .into_iter()
        .find(|path| path.is_file())
}

fn apply_binary_runtime_env(binary: &str, command: &mut Command) {
    if !matches!(binary, "ffmpeg" | "ffprobe") {
        return;
    }

    if let Some(lib_dir) = resolve_ffmpeg_library_dir() {
        command.env("DYLD_FALLBACK_LIBRARY_PATH", &lib_dir);
        command.env("DYLD_LIBRARY_PATH", &lib_dir);
    }
}

fn bundled_binary_candidates(binary: &str) -> Vec<PathBuf> {
    let bundled_name = bundled_binary_name(binary);
    let dev_name = dev_binary_name(binary);
    let mut candidates = Vec::new();

    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join(&bundled_name));
            candidates.push(parent.join("../Resources").join(&bundled_name));

            if let Some(contents_dir) = parent.parent() {
                candidates.push(contents_dir.join("Resources").join(&bundled_name));
            }
        }
    }

    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(&dev_name),
    );

    candidates
}

fn resolve_ffmpeg_library_dir() -> Option<PathBuf> {
    ffmpeg_library_dir_candidates()
        .into_iter()
        .find(|path| path.is_dir())
}

#[cfg(target_os = "macos")]
fn resolve_python_path() -> Option<PathBuf> {
    python_runtime_root_candidates()
        .into_iter()
        .map(|root| root.join("bin").join("python3"))
        .find(|path| path.is_file())
        .or_else(|| {
            if command_exists("python3", &["--version"]) {
                Some(PathBuf::from("python3"))
            } else {
                None
            }
        })
}

#[cfg(not(target_os = "macos"))]
fn resolve_python_path() -> Option<PathBuf> {
    None
}

#[cfg(target_os = "macos")]
fn resolve_python_runtime_root() -> Option<PathBuf> {
    python_runtime_root_candidates()
        .into_iter()
        .find(|path| path.is_dir())
}

#[cfg(not(target_os = "macos"))]
fn resolve_python_runtime_root() -> Option<PathBuf> {
    None
}

fn python_runtime_root_candidates() -> Vec<PathBuf> {
    let target_resource_dir = python_runtime_dir_name(current_target_triple());
    let mut candidates = Vec::new();

    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join(format!("../Resources/{target_resource_dir}/python")));
            candidates.push(parent.join(format!(
                "../Resources/resources/{target_resource_dir}/python"
            )));

            if let Some(contents_dir) = parent.parent() {
                candidates.push(
                    contents_dir
                        .join("Resources")
                        .join(&target_resource_dir)
                        .join("python"),
                );
                candidates.push(
                    contents_dir
                        .join("Resources")
                        .join("resources")
                        .join(&target_resource_dir)
                        .join("python"),
                );
            }
        }
    }

    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(&target_resource_dir)
            .join("python"),
    );

    candidates
}

fn ffmpeg_library_dir_candidates() -> Vec<PathBuf> {
    let target_resource_dir = ffmpeg_resource_dir_name(current_target_triple());
    let mut candidates = Vec::new();

    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join(format!("../Resources/{target_resource_dir}")));
            candidates.push(parent.join("../Resources/ffmpeg-libs"));

            if let Some(contents_dir) = parent.parent() {
                candidates.push(contents_dir.join("Resources").join(&target_resource_dir));
                candidates.push(contents_dir.join("Resources").join("ffmpeg-libs"));
                candidates.push(
                    contents_dir
                        .join("Resources")
                        .join("resources")
                        .join(&target_resource_dir),
                );
                candidates.push(
                    contents_dir
                        .join("Resources")
                        .join("resources")
                        .join("ffmpeg-libs"),
                );
            }
        }
    }

    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(&target_resource_dir),
    );
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("ffmpeg-libs"),
    );

    candidates
}

fn ffmpeg_resource_dir_name(target_triple: &str) -> String {
    match target_triple {
        "x86_64-apple-darwin" => "ffmpeg-libs-x86_64-apple-darwin".into(),
        _ => "ffmpeg-libs".into(),
    }
}

fn python_runtime_dir_name(target_triple: &str) -> String {
    match target_triple {
        "x86_64-apple-darwin" => "python-runtime-x86_64-apple-darwin".into(),
        _ => "python-runtime-aarch64-apple-darwin".into(),
    }
}

fn bundled_binary_name(binary: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{binary}.exe")
    } else {
        binary.to_string()
    }
}

fn dev_binary_name(binary: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{binary}-{}.exe", current_target_triple())
    } else {
        format!("{binary}-{}", current_target_triple())
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn current_target_triple() -> &'static str {
    "aarch64-apple-darwin"
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn current_target_triple() -> &'static str {
    "x86_64-apple-darwin"
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn current_target_triple() -> &'static str {
    "x86_64-pc-windows-msvc"
}

#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
fn current_target_triple() -> &'static str {
    "aarch64-pc-windows-msvc"
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn current_target_triple() -> &'static str {
    "x86_64-unknown-linux-gnu"
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn current_target_triple() -> &'static str {
    "aarch64-unknown-linux-gnu"
}

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64")
)))]
fn current_target_triple() -> &'static str {
    "unsupported-target"
}

fn is_youtube_url(url: &str) -> bool {
    let lowercased = url.to_lowercase();
    lowercased.contains("youtube.com/") || lowercased.contains("youtu.be/")
}

fn expand_home_path(path: &str) -> String {
    if path == "~" {
        return env::var("HOME").unwrap_or_else(|_| ".".into());
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return env::var("HOME")
            .map(|home| format!("{home}/{rest}"))
            .unwrap_or_else(|_| path.into());
    }

    path.into()
}

fn recommended_output_dir() -> String {
    if let Some(download_dir) = dirs::download_dir() {
        return download_dir
            .join("拾流下载器")
            .to_string_lossy()
            .to_string();
    }

    match env::var("HOME") {
        Ok(home) => format!("{home}/Downloads/拾流下载器"),
        Err(_) => "./Downloads/拾流下载器".into(),
    }
}

fn task_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let seq = TASK_SEQ.fetch_add(1, Ordering::Relaxed);

    format!("task-{millis:013}-{seq:06}")
}

fn infer_title_from_url(url: &str, language: &str) -> String {
    url.split('/')
        .next_back()
        .filter(|segment| !segment.is_empty())
        .unwrap_or(if is_english(language) {
            "New download"
        } else {
            "新建下载"
        })
        .to_string()
}

fn request_language_from_state(state: &State<AppState>, task_id: &str) -> String {
    state
        .task_requests
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(task_id)
        .map(|request| request.language.clone())
        .or_else(|| {
            Some(
                state
                    .settings
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .language
                    .clone(),
            )
        })
        .unwrap_or_else(|| "zh-CN".into())
}

fn message_primary_fallback(language: &str) -> String {
    if is_english(language) {
        "The selected format is unavailable. Retrying with a compatible fallback format.".into()
    } else {
        "首选格式不可用，正在自动回退到兼容格式重新尝试。".into()
    }
}

fn message_retry_without_auth(language: &str) -> String {
    if is_english(language) {
        "Browser-cookie download failed. Retrying without cookies.".into()
    } else {
        "浏览器 Cookie 下载失败，正在自动回退到无 Cookie 重试。".into()
    }
}

fn message_no_auth_fallback(language: &str) -> String {
    if is_english(language) {
        "The preferred no-cookie format is unavailable. Retrying with a compatible fallback format."
            .into()
    } else {
        "无 Cookie 首选格式不可用，正在自动回退到兼容格式重新尝试。".into()
    }
}

fn store_task_request(
    store: &Arc<Mutex<HashMap<String, StartDownloadRequest>>>,
    id: &str,
    request: &StartDownloadRequest,
) {
    let mut requests = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    requests.insert(id.to_string(), request.clone());
}

fn store_task_pid(store: &Arc<Mutex<HashMap<String, u32>>>, id: &str, pid: u32) {
    let mut pids = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    pids.insert(id.to_string(), pid);
}

fn take_task_pid(store: &Arc<Mutex<HashMap<String, u32>>>, id: &str) -> Option<u32> {
    let mut pids = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    pids.remove(id)
}

fn remove_task_pid(store: &Arc<Mutex<HashMap<String, u32>>>, id: &str) {
    let mut pids = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    pids.remove(id);
}

fn clear_task_cancelled(store: &Arc<Mutex<HashSet<String>>>, id: &str) {
    let mut cancelled = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cancelled.remove(id);
}

fn take_task_cancelled(store: &Arc<Mutex<HashSet<String>>>, id: &str) -> bool {
    let mut cancelled = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cancelled.remove(id)
}

fn acquire_download_permit(gate: &Arc<(Mutex<usize>, Condvar)>) -> DownloadPermit {
    let (lock, cvar) = &**gate;
    let mut available = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    while *available == 0 {
        available = cvar
            .wait(available)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    }

    *available -= 1;

    DownloadPermit {
        gate: Arc::clone(gate),
    }
}

fn release_download_permit(gate: &Arc<(Mutex<usize>, Condvar)>) {
    let (lock, cvar) = &**gate;
    let mut available = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *available += 1;
    cvar.notify_one();
}

fn upsert_task(store: &Arc<Mutex<HashMap<String, DownloadTask>>>, task: &DownloadTask) {
    store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(task.id.clone(), task.clone());
}

fn upsert_task_if_present(
    store: &Arc<Mutex<HashMap<String, DownloadTask>>>,
    task: &DownloadTask,
) -> bool {
    let mut tasks = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if !tasks.contains_key(&task.id) {
        return false;
    }

    tasks.insert(task.id.clone(), task.clone());
    true
}

fn get_task(store: &Arc<Mutex<HashMap<String, DownloadTask>>>, id: &str) -> Option<DownloadTask> {
    store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(id)
        .cloned()
}

fn remove_task_runtime_entries(state: &AppState, task_id: &str) {
    remove_task_pid(&state.task_pids, task_id);
    state
        .task_requests
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(task_id);
    state
        .cancelled_tasks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(task_id);
}

fn remove_task_entries(state: &AppState, task_id: &str) {
    state
        .tasks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(task_id);
    remove_task_runtime_entries(state, task_id);
}

fn emit_task_update(app: &AppHandle, task: &DownloadTask) {
    let _ = app.emit("download-task-updated", task);
}

fn emit_task_removed(app: &AppHandle, task_id: &str) {
    let _ = app.emit("download-task-removed", task_id);
}

fn emit_history_update(app: &AppHandle, item: &HistoryItem) {
    let _ = app.emit("history-item-added", item);
}

fn record_history(state: &AppState, app: &AppHandle, task: &DownloadTask) {
    let item = HistoryItem {
        title: task.title.clone(),
        finished_at: current_timestamp_label(),
        profile: task.profile.clone(),
        output: task.output.clone(),
    };

    {
        let mut history = state
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        history.insert(0, item.clone());
        if history.len() > 200 {
            history.truncate(200);
        }
    }

    emit_history_update(app, &item);
}

fn current_timestamp_label() -> String {
    Command::new("date")
        .args(["+%Y-%m-%d %H:%M:%S"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|value| value.trim().to_string())
            } else {
                None
            }
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs().to_string())
                .unwrap_or_else(|_| "unknown".into())
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(load_app_state())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            parse_url,
            get_tasks,
            get_history,
            get_settings,
            save_settings,
            start_download,
            retry_download,
            cancel_download,
            clear_tasks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_download_progress_lines() {
        let progress =
            parse_progress("[download]  42.0% of 10.00MiB at 1.50MiB/s ETA 00:05").unwrap();

        assert_eq!(progress.progress, 42.0);
        assert_eq!(progress.speed, "1.50MiB/s");
        assert_eq!(progress.eta, "00:05");
        assert!(parse_progress("[download] Destination: /tmp/file.mp4").is_none());
        assert!(parse_progress("not a progress line").is_none());
    }

    #[test]
    fn formats_byte_labels_with_localized_unknown() {
        assert_eq!(byte_label(Some(1536), "zh-CN"), "1.5 KB");
        assert_eq!(byte_label(None, "zh-CN"), "大小未知");
        assert_eq!(byte_label(None, "en-US"), "Unknown size");
    }

    #[test]
    fn formats_dates_when_compact_upload_date() {
        assert_eq!(format_date("20240115"), "2024-01-15");
        assert_eq!(format_date("2024-01-15"), "2024-01-15");
        assert_eq!(format_date("202401"), "202401");
    }

    #[test]
    fn extracts_titles_from_output_paths() {
        assert_eq!(
            title_from_output_path("/x/My Video [abc123].mp4").as_deref(),
            Some("My Video")
        );
        assert_eq!(
            title_from_output_path("/x/No Bracket.mp3").as_deref(),
            Some("No Bracket")
        );
    }

    #[test]
    fn formats_duration_labels() {
        let short = json!(65);
        let long = json!(3661.4);

        assert_eq!(duration_label(Some(&short)), "01:05");
        assert_eq!(duration_label(Some(&long)), "01:01:01");
        assert_eq!(duration_label(None), "--:--");
    }

    #[test]
    fn detects_youtube_urls() {
        assert!(is_youtube_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_youtube_url("https://youtu.be/abc"));
        assert!(!is_youtube_url("https://example.com/watch?v=abc"));
    }

    #[test]
    fn extracts_output_paths_from_known_prefixes() {
        assert_eq!(
            extract_output_path("[download] Destination: /tmp/video.mp4").as_deref(),
            Some("/tmp/video.mp4")
        );
        assert_eq!(
            extract_output_path("[ExtractAudio] Destination: /tmp/audio.mp3").as_deref(),
            Some("/tmp/audio.mp3")
        );
        assert_eq!(
            extract_output_path("[Merger] Merging formats into \"/tmp/merged.mp4\"").as_deref(),
            Some("/tmp/merged.mp4")
        );
    }

    #[test]
    fn detects_format_fallback_errors() {
        assert!(should_retry_with_fallback(&Some(
            "Requested format is not available".into()
        )));
        assert!(should_retry_with_fallback(&Some(
            "requested format not available".into()
        )));
        assert!(!should_retry_with_fallback(&Some("network error".into())));
        assert!(!should_retry_with_fallback(&None));
    }

    #[test]
    fn sanitizes_settings_and_language() {
        let sanitized = sanitize_settings(AppSettings {
            output_dir: "/tmp/downloads".into(),
            default_download_mode: "bad-mode".into(),
            default_playlist_scope: "bad-scope".into(),
            default_auth_mode: "bad-auth".into(),
            default_browser: "bad-browser".into(),
            default_cookie_file: "  /tmp/cookies.txt  ".into(),
            language: "english".into(),
        });

        assert_eq!(sanitized.output_dir, "/tmp/downloads");
        assert_eq!(sanitized.default_download_mode, "video");
        assert_eq!(sanitized.default_playlist_scope, "video");
        assert_eq!(sanitized.default_auth_mode, "none");
        assert_eq!(sanitized.default_browser, "chrome");
        assert_eq!(sanitized.default_cookie_file, "/tmp/cookies.txt");
        assert_eq!(sanitized.language, "en-US");
        assert_eq!(sanitize_language("zh-CN"), "zh-CN");
        assert_eq!(sanitize_language("en"), "en-US");
    }
}
