mod websocket;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};
use winreg::enums::*;
use winreg::RegKey;

#[tauri::command]
fn get_system_fonts() -> Vec<String> {
    let mut fonts = Vec::new();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(font_key) = hklm.open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts") {
        for (name, _) in font_key.enum_values().filter_map(|v| v.ok()) {
            let mut font_name = name;
            // Clean up common suffixes
            font_name = font_name.replace(" (TrueType)", "");
            font_name = font_name.replace(" (OpenType)", "");
            font_name = font_name.replace(" & ", " and ");
            fonts.push(font_name);
        }
    }
    fonts.sort();
    fonts.dedup();
    fonts
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
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
        get_system_fonts
    ])
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      
      let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
      let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
      let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
      
      let menu = Menu::with_items(app, &[&show_i, &settings_i, &quit_i])?;

      let _tray = TrayIconBuilder::new()
          .menu(&menu)
          .icon(app.default_window_icon().unwrap().clone())
          .on_menu_event(|app, event| match event.id.as_ref() {
              "quit" => {
                  std::process::exit(0);
              }
              "show" | "settings" => {
                  if let Some(window) = app.get_webview_window("main") {
                      let _ = window.show();
                      let _ = window.set_focus();
                  }
              }
              _ => {}
          })
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
