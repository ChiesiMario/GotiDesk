mod websocket;

use tauri::{
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};
#[tauri::command]
fn get_system_fonts() -> Vec<String> {
    use font_kit::source::SystemSource;
    
    let mut fonts = Vec::new();
    if let Ok(families) = SystemSource::new().all_families() {
        fonts = families;
    }
    
    fonts.sort();
    fonts.dedup();
    fonts
}

#[tauri::command]
fn quit_app() {
    std::process::exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_window_state::Builder::new().build())
    .plugin(tauri_plugin_shell::init())
    .plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        Some(vec!["--hidden"]),
    ))
    .plugin(tauri_plugin_store::Builder::new().build())
    .plugin(tauri_plugin_notification::init())
    .manage(websocket::WsState {
        task: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        
    })
    .invoke_handler(tauri::generate_handler![
        websocket::restart_websocket,
        websocket::fetch_messages,
        websocket::fetch_applications,
        websocket::get_message_by_id,
        websocket::create_detail_window,
        websocket::resize_window,
        websocket::show_window,
        websocket::delete_message,
        websocket::delete_all_messages,
        websocket::get_ws_status,
        get_system_fonts,
        quit_app
    ])
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      let _tray = TrayIconBuilder::with_id("main")
          .icon(app.default_window_icon().unwrap().clone())
          .on_tray_icon_event(|tray, event| match event {
              tauri::tray::TrayIconEvent::Click {
                  button: tauri::tray::MouseButton::Left,
                  button_state: tauri::tray::MouseButtonState::Up,
                  ..
              } => {
                  let app = tray.app_handle();
                  if let Some(window) = app.get_webview_window("main") {
                      let _ = window.show();
                      let _ = window.set_focus();
                  }
              }
              _ => {}
          })
          .build(app)?;

      let app_handle = app.handle().clone();

      let args: Vec<String> = std::env::args().collect();
      if !args.contains(&"--hidden".to_string()) {
          if let Some(window) = app.get_webview_window("main") {
              let _ = window.show();
          }
      }

      tauri::async_runtime::spawn(async move {
          // Trigger initial connection
          let _ = websocket::restart_websocket(
              app_handle.clone(),
              app_handle.state::<websocket::WsState>(),
          ).await;
      });

      Ok(())
    })
    .on_window_event(|window, event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            if window.label() == "main" {
                window.hide().unwrap();
                api.prevent_close();
            }
        }
        _ => {}
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
