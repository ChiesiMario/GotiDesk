use std::sync::Arc;
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GotifyMessage {
    pub id: u64,
    pub title: Option<String>,
    pub message: String,
    pub priority: u32,
    pub date: String,
    pub appid: Option<u64>,
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
                        app.emit("ws-status", "connecting").ok();
                        match connect_async(parsed_url.as_str()).await {
                            Ok((mut ws_stream, _)) => {
                                println!("WebSocket connected");
                                app.emit("ws-status", "connected").ok();
                                
                                while let Some(msg) = ws_stream.next().await {
                                    match msg {
                                        Ok(tokio_tungstenite::tungstenite::protocol::Message::Text(text)) => {
                                            if let Ok(gotify_msg) = serde_json::from_str::<GotifyMessage>(&text) {
                                                app.emit("gotify-message", gotify_msg.clone()).ok();
                                                
                                                // 觸發原生通知
                                                use tauri_plugin_notification::NotificationExt;
                                                let _ = app.notification().builder()
                                                    .title(gotify_msg.title.unwrap_or_else(|| "GotiDesk".to_string()))
                                                    .body(gotify_msg.message)
                                                    .show();
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
        
        app.emit("ws-status", "disconnected").ok();
        println!("WebSocket reconnecting in 5 seconds...");
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
    let api_url = format!("{}/message?token={}", url.trim_end_matches('/'), token);
    
    let client = reqwest::Client::new();
    let response = client.get(&api_url)
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
    let api_url = format!("{}/application?token={}", url.trim_end_matches('/'), token);
    
    let client = reqwest::Client::new();
    let response = client.get(&api_url)
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
