mod websocket;

use tauri::{
    tray::TrayIconBuilder,
    Manager, WindowEvent, Emitter,
};
use tauri_plugin_store::StoreExt;
use tauri_plugin_autostart::ManagerExt;
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

#[tauri::command]
fn app_ready(window: tauri::WebviewWindow) {
    let args: Vec<String> = std::env::args().collect();
    if window.label() == "main" {
        if !args.contains(&"--hidden".to_string()) {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(
        tauri_plugin_window_state::Builder::default()
            .with_state_flags(
                tauri_plugin_window_state::StateFlags::SIZE | 
                tauri_plugin_window_state::StateFlags::POSITION | 
                tauri_plugin_window_state::StateFlags::MAXIMIZED
            )
            .build()
    )
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
        quit_app,
        app_ready
    ])
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      let mut lang = "en".to_string();
      if let Ok(store) = app.handle().store("settings.json") {
          if let Some(val) = store.get("language") {
              if let Some(l) = val.as_str() {
                  lang = l.to_string();
              }
          }
      }

      let (show_txt, settings_txt, autostart_txt, quit_txt) = match lang.as_str() {
          "zh-TW" => ("顯示 GotiDesk", "設定", "開機自啟動", "離開"),
          "zh-CN" => ("显示 GotiDesk", "设置", "开机自启动", "退出"),
          _ => ("Show GotiDesk", "Settings", "Autostart", "Quit"),
      };

      let show_i = tauri::menu::MenuItem::with_id(app, "show", show_txt, true, None::<&str>)?;
      let settings_i = tauri::menu::MenuItem::with_id(app, "settings", settings_txt, true, None::<&str>)?;
      let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
      let autostart_i = tauri::menu::CheckMenuItem::with_id(app, "autostart", autostart_txt, true, autostart_enabled, None::<&str>)?;
      let quit_i = tauri::menu::MenuItem::with_id(app, "quit", quit_txt, true, None::<&str>)?;
      let sep1 = tauri::menu::PredefinedMenuItem::separator(app)?;
      let sep2 = tauri::menu::PredefinedMenuItem::separator(app)?;

      let menu = tauri::menu::Menu::with_items(app, &[&show_i, &settings_i, &sep1, &autostart_i, &sep2, &quit_i])?;

      let _tray = TrayIconBuilder::with_id("main")
          .icon(app.default_window_icon().unwrap().clone())
          .menu(&menu)
          .on_menu_event(|app, event| match event.id().as_ref() {
              "show" => {
                  if let Some(window) = app.get_webview_window("main") {
                      let _ = window.unminimize();
                      let _ = window.show();
                      let _ = window.set_focus();
                  }
              }
              "settings" => {
                  if let Some(window) = app.get_webview_window("main") {
                      let _ = window.unminimize();
                      let _ = window.show();
                      let _ = window.set_focus();
                      let _ = window.emit("open-settings", ());
                  }
              }
              "autostart" => {
                  let autolaunch = app.autolaunch();
                  let is_enabled = autolaunch.is_enabled().unwrap_or(false);
                  if is_enabled {
                      let _ = autolaunch.disable();
                  } else {
                      let _ = autolaunch.enable();
                  }
              }
              "quit" => {
                  std::process::exit(0);
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
                      let _ = window.unminimize();
                      let _ = window.show();
                      let _ = window.set_focus();
                  }
              }
              _ => {}
          })
          .build(app)?;

      let app_handle = app.handle().clone();

      // We no longer show the window immediately here.
      // We let the frontend call window.show() once the Svelte splash screen is ready to prevent OS white flash.

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
