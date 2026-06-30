[English](./README.md) | [繁體中文](./README.zh-TW.md) | [简体中文](./README.zh-CN.md)

---

# GotiDesk

A sleek, lightweight, and native-feeling desktop client for Gotify, built with Tauri v2 and Svelte 5. GotiDesk is designed to seamlessly integrate into your Windows desktop environment, providing reliable WebSocket-based push notifications without the overhead of Electron.

![GotiDesk Screenshot](./Screenshot/Screenshot.png)

## Features

- **Native Windows Notifications:** Utilizes Windows 10/11 Toast notifications for a fully integrated system experience.
- **System Tray Integration:** Runs quietly in the background with a customizable system tray menu.
- **Granular App Controls:** Configure notification priorities, toggle notifications per-app, or apply global settings.
- **Auto-Start on Boot:** Optionally launch GotiDesk automatically when Windows starts.
- **Sleek Minimalist UI:** Designed with a monochrome aesthetic (black, white, and grays) and full dark mode support, built with Tailwind CSS.
- **Resource Efficient:** Built on Rust and Tauri v2, using a fraction of the memory required by traditional web-wrapper apps.

## Installation

You can download the latest Windows installer (`.msi` or `.exe`) from the [Releases](../../releases) page.

1. Run the installer and follow the standard setup process.
2. Launch GotiDesk from your Start Menu.
3. Enter your Gotify server URL and Client Token in the settings menu to connect.

## Development Setup

To build or run GotiDesk locally, you will need [Node.js (v20+)](https://nodejs.org/), [pnpm](https://pnpm.io/), and the [Rust toolchain](https://rustup.rs/).

### 1. Clone the repository
```bash
git clone https://github.com/ChiesiMario/GotiDesk.git
cd GotiDesk
```

### 2. Install dependencies
```bash
pnpm install
```

### 3. Run in development mode
```bash
pnpm tauri dev
```

### 4. Build for production (Windows)
```bash
pnpm tauri build
```

## Architecture & Technologies

- **Frontend:** Svelte 5, TypeScript, Vite, Tailwind CSS v4.
- **Backend:** Rust, Tauri v2.
- **Platform Specifics:** `winreg` for registry-based auto-start, `tauri-winrt-notification` for native Windows Toast bindings.

## License

MIT License. See `LICENSE` for more information.
