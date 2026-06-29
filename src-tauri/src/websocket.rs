use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::sync::Mutex;
use tauri::async_runtime::JoinHandle;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use url::Url;
use lazy_static::lazy_static;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppPushSetting {
    pub enabled: bool,
    pub min_priority: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PushSettings {
    pub global_enabled: bool,
    pub receive_all_apps: bool,
    pub global_min_priority: u32,
    pub apps: HashMap<String, AppPushSetting>,
}

impl Default for PushSettings {
    fn default() -> Self {
        Self {
            global_enabled: true,
            receive_all_apps: true,
            global_min_priority: 0,
            apps: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GotifyMessage {
    pub id: u64,
    pub title: Option<String>,
    pub message: String,
    pub priority: u32,
    pub date: String,
    pub appid: Option<u64>,
}

lazy_static! {
    static ref MESSAGE_CACHE: Arc<Mutex<HashMap<u64, GotifyMessage>>> = Arc::new(Mutex::new(HashMap::new()));
    static ref CURRENT_WS_STATUS: Arc<Mutex<String>> = Arc::new(Mutex::new("disconnected".to_string()));
}

pub struct WsState {
    pub task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

pub async fn start_websocket_loop(app: AppHandle) {
    loop {
        let (url, token) = {
            if let Ok(store) = app.store("settings.json") {
                let u = store.get("gotify_url").and_then(|v| v.as_str().map(|s| s.to_string()));
                let t = store.get("gotify_token").and_then(|v| v.as_str().map(|s| s.to_string()));
                (u, t)
            } else {
                (None, None)
            }
        };

        if let (Some(url_str), Some(token_str)) = (url, token) {
            if !url_str.is_empty() && !token_str.is_empty() {
                let ws_url = if url_str.starts_with("https://") {
                    url_str.replace("https://", "wss://")
                } else if url_str.starts_with("http://") {
                    url_str.replace("http://", "ws://")
                } else {
                    format!("wss://{}", url_str)
                };

                let stream_url = format!("{}/stream?token={}", ws_url.trim_end_matches('/'), token_str);

                match Url::parse(&stream_url) {
                    Ok(parsed_url) => {
                        println!("Connecting to Gotify WebSocket...");
                        *CURRENT_WS_STATUS.lock().await = "connecting".to_string();
                        app.emit("ws-status", "connecting").ok();
                        match connect_async(parsed_url.as_str()).await {
                            Ok((mut ws_stream, _)) => {
                                println!("WebSocket connected");
                                *CURRENT_WS_STATUS.lock().await = "connected".to_string();
                                app.emit("ws-status", "connected").ok();
                                
                                while let Some(msg) = ws_stream.next().await {
                                    match msg {
                                        Ok(tokio_tungstenite::tungstenite::protocol::Message::Text(text)) => {
                                            if let Ok(gotify_msg) = serde_json::from_str::<GotifyMessage>(&text) {
                                                app.emit("gotify-message", gotify_msg.clone()).ok();
                                                
                                                {
                                                    let mut cache = MESSAGE_CACHE.lock().await;
                                                    cache.insert(gotify_msg.id, gotify_msg.clone());
                                                }

                                                #[cfg(target_os = "windows")]
                                                {
                                                    use tauri_winrt_notification::{Duration, Sound, Toast};
                                                    
                                                    let push_settings = if let Ok(store) = app.store("settings.json") {
                                                        if let Some(val) = store.get("push_settings") {
                                                            serde_json::from_value::<PushSettings>(val.clone()).unwrap_or_else(|_| PushSettings::default())
                                                        } else {
                                                            PushSettings::default()
                                                        }
                                                    } else {
                                                        PushSettings::default()
                                                    };
                                                    
                                                    let mut should_push = false;
                                                    
                                                    if push_settings.global_enabled {
                                                        if push_settings.receive_all_apps {
                                                            should_push = gotify_msg.priority >= push_settings.global_min_priority;
                                                        } else {
                                                            if let Some(appid) = gotify_msg.appid {
                                                                let appid_str = appid.to_string();
                                                                if let Some(app_setting) = push_settings.apps.get(&appid_str) {
                                                                    let prio = app_setting.min_priority.unwrap_or(push_settings.global_min_priority);
                                                                    should_push = app_setting.enabled && gotify_msg.priority >= prio;
                                                                }
                                                            } else {
                                                                should_push = gotify_msg.priority >= push_settings.global_min_priority;
                                                            }
                                                        }
                                                    }
                                                    
                                                    if should_push {
                                                        let msg_id = gotify_msg.id;
                                                        let app_clone = app.clone();
                                                        
                                                        let title = gotify_msg.title.clone().unwrap_or_else(|| "GotiDesk".to_string());
                                                        let body = gotify_msg.message.clone();
                                                        
                                                        std::thread::spawn(move || {
                                                            let _ = Toast::new(Toast::POWERSHELL_APP_ID)
                                                                .title(&title)
                                                                .text1(&body)
                                                                .sound(Some(Sound::SMS))
                                                                .duration(Duration::Short)
                                                                .on_activated(move |_| {
                                                                    let _ = app_clone.emit("open-detail", msg_id);
                                                                    Ok(())
                                                                })
                                                                .show();
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                        Ok(tokio_tungstenite::tungstenite::protocol::Message::Close(_)) => {
                                            println!("WebSocket closed by server");
                                            break;
                                        }
                                        Err(e) => {
                                            println!("WebSocket error: {}", e);
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Err(e) => {
                                println!("WebSocket connection failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        println!("Invalid WebSocket URL: {}", e);
                    }
                }
            }
        }
        
        println!("WebSocket connection lost or closed. Retrying in 5 seconds...");
        *CURRENT_WS_STATUS.lock().await = "disconnected".to_string();
        app.emit("ws-status", "disconnected").ok();
        sleep(Duration::from_secs(5)).await;
    }
}

#[tauri::command]
pub async fn restart_websocket(app: AppHandle, state: tauri::State<'_, WsState>) -> Result<(), String> {
    let mut task_guard = state.task.lock().await;
    if let Some(task) = task_guard.take() {
        task.abort();
    }
    
    let app_clone = app.clone();
    *task_guard = Some(tauri::async_runtime::spawn(async move {
        start_websocket_loop(app_clone).await;
    }));
    
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Paging {
    pub limit: u32,
    pub since: u32,
    pub size: u32,
    pub next: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessagesResponse {
    pub messages: Vec<GotifyMessage>,
    pub paging: Paging,
}

#[tauri::command]
pub async fn fetch_messages(url: String, token: String) -> Result<Vec<GotifyMessage>, String> {
    let api_url = format!("{}/message?limit=100", url.trim_end_matches('/'));
    
    let client = reqwest::Client::new();
    let response = client.get(&api_url)
        .header("X-Gotify-Key", token)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
        
    if response.status().is_success() {
        let messages_resp: MessagesResponse = response.json().await
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;
        Ok(messages_resp.messages)
    } else {
        Err(format!("Server returned error: {}", response.status()))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GotifyApplication {
    pub id: u64,
    pub name: String,
    pub description: String,
}

#[tauri::command]
pub async fn fetch_applications(url: String, token: String) -> Result<Vec<GotifyApplication>, String> {
    let api_url = format!("{}/application", url.trim_end_matches('/'));
    
    let client = reqwest::Client::new();
    let response = client.get(&api_url)
        .header("X-Gotify-Key", token)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
        
    if response.status().is_success() {
        let apps: Vec<GotifyApplication> = response.json().await
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;
        Ok(apps)
    } else {
        Err(format!("Server returned error: {}", response.status()))
    }
}

#[tauri::command]
pub async fn get_message_by_id(id: u64) -> Result<GotifyMessage, String> {
    let cache = MESSAGE_CACHE.lock().await;
    if let Some(msg) = cache.get(&id) {
        Ok(msg.clone())
    } else {
        Err("Message not found in cache".to_string())
    }
}

#[tauri::command]
pub async fn create_detail_window(app: AppHandle, id: u64) -> Result<(), String> {
    let window_label = format!("detail_{}", id);
    let url = format!("/?view=detail&id={}", id);
    
    if let Some(window) = app.get_webview_window(&window_label) {
        let _ = window.set_focus();
        return Ok(());
    }
    
    let builder = tauri::WebviewWindowBuilder::new(
        &app,
        window_label,
        tauri::WebviewUrl::App(url.into()),
    )
    .title("Message Detail")
    .inner_size(500.0, 700.0)
    .visible(false)
    .center();
    
    builder.build().map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub async fn resize_window(app: AppHandle, label: String, width: f64, height: f64) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        Ok(())
    } else {
        Err("Window not found".to_string())
    }
}

#[tauri::command]
pub async fn show_window(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        let _ = window.set_focus();
        Ok(())
    } else {
        Err("Window not found".to_string())
    }
}

#[tauri::command]
pub async fn delete_message(url: String, token: String, id: u64) -> Result<(), String> {
    let api_url = format!("{}/message/{}", url.trim_end_matches('/'), id);
    
    let client = reqwest::Client::new();
    let response = client.delete(&api_url)
        .header("X-Gotify-Key", token)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
        
    if response.status().is_success() {
        let mut cache = MESSAGE_CACHE.lock().await;
        cache.remove(&id);
        Ok(())
    } else {
        Err(format!("Server returned error: {}", response.status()))
    }
}

#[tauri::command]
pub async fn delete_all_messages(url: String, token: String, app_id: u64) -> Result<(), String> {
    let api_url = format!("{}/application/{}/message", url.trim_end_matches('/'), app_id);
    
    let client = reqwest::Client::new();
    let response = client.delete(&api_url)
        .header("X-Gotify-Key", token)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
        
    if response.status().is_success() {
        let mut cache = MESSAGE_CACHE.lock().await;
        // Invalidate cache or wait for ws updates
        cache.retain(|_, msg| msg.appid != Some(app_id));
        Ok(())
    } else {
        Err(format!("Server returned error: {}", response.status()))
    }
}

#[tauri::command]
pub async fn get_ws_status() -> Result<String, String> {
    let status = CURRENT_WS_STATUS.lock().await;
    Ok(status.clone())
}
