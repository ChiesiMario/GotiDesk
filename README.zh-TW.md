[English](./README.md) | [繁體中文](./README.zh-TW.md) | [简体中文](./README.zh-CN.md)

---

# GotiDesk

一款基於 Tauri v2 與 Svelte 5 打造，簡潔、輕量且擁有原生體驗的 Gotify 桌面端接收器。GotiDesk 專為 Windows 桌面環境深度整合而設計，能提供穩定可靠的 WebSocket 推播通知，且無需承受傳統 Electron 應用的高資源佔用。

![GotiDesk Screenshot](./Screenshot/Screenshot.png)

## 核心功能

- **Windows 原生通知：** 呼叫 Windows 10/11 原生的 Toast API，提供完美契合系統體驗的通知彈窗。
- **系統托盤整合：** 安靜地在背景運行，提供支援多語系的托盤右鍵選單。
- **精細的應用控管：** 您可以為不同的 App 單獨設定通知優先級、單獨開啟或關閉推播，或是套用全域設定。
- **開機自啟動：** 支援透過 Windows 登錄檔 (Registry) 設定，在系統開機時自動於背景啟動 GotiDesk。
- **極簡 UI 設計：** 採用純粹的黑白灰單色調美學設計，由 Tailwind CSS 驅動，並完美支援深色模式 (Dark Mode)。
- **極致效能：** 底層基於 Rust 構建，記憶體佔用極低，運行極為流暢。

## 安裝指南

您可以前往 [Releases](../../releases) 頁面下載最新版本的 Windows 安裝檔（`.msi` 或 `.exe`）。

1. 執行安裝程式並完成一般安裝步驟。
2. 從開始功能表啟動 GotiDesk。
3. 在設定介面中輸入您的 Gotify 伺服器網址 (Server URL) 與客戶端權杖 (Client Token) 即可連線。

## 開發者設定

若要在本地編譯或執行 GotiDesk，您需要準備 [Node.js (v20+)](https://nodejs.org/)、[pnpm](https://pnpm.io/)，以及 [Rust 編譯工具鏈](https://rustup.rs/)。

### 1. 複製專案原始碼
```bash
git clone https://github.com/ChiesiMario/GotiDesk.git
cd GotiDesk
```

### 2. 安裝相依套件
```bash
pnpm install
```

### 3. 啟動開發伺服器
```bash
pnpm tauri dev
```

### 4. 編譯正式版 (Windows)
```bash
pnpm tauri build
```

## 技術架構

- **前端：** Svelte 5, TypeScript, Vite, Tailwind CSS v4.
- **後端：** Rust, Tauri v2.
- **平台專屬 API：** 使用 `winreg` 處理開機自啟動，使用 `tauri-winrt-notification` 觸發原生 Windows 通知。

## 授權條款

採用 MIT License 授權。詳情請參閱 `LICENSE` 檔案。
