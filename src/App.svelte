<script lang="ts">
  import { load } from '@tauri-apps/plugin-store';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { open } from '@tauri-apps/plugin-shell';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import { onMount, tick } from 'svelte';
  import { fade } from 'svelte/transition';
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import FontSelect from './lib/FontSelect.svelte';
  import { enable, isEnabled, disable } from '@tauri-apps/plugin-autostart';
  import { type LanguageCode, translations, getSystemLanguage } from './lib/i18n';
  import { TrayIcon } from '@tauri-apps/api/tray';
  import { Menu, MenuItem, PredefinedMenuItem, CheckMenuItem } from '@tauri-apps/api/menu';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  interface GotifyMessage {
    id: number;
    title: string | null;
    message: string;
    priority: number;
    date: string;
    appid: number | null;
  }

  interface GotifyApplication {
    id: number;
    name: string;
    description: string;
  }

  interface AppPushSetting {
    enabled: boolean;
    min_priority: number | null;
  }

  interface PushSettings {
    global_enabled: boolean;
    receive_all_apps: boolean;
    global_min_priority: number;
    apps: Record<string, AppPushSetting>;
  }

  let store: any = null;

  let currentView: 'loading' | 'login' | 'messages' | 'detail' = $state('loading');
  
  // States for messages view
  let url = $state('');
  let token = $state('');
  let dateFormat = $state('system');
  let fontPrimary = $state('');
  let fontFallback = $state('');
  let systemFonts: string[] = $state([]);
  let activePopover: { id: string, top: number, left: number } | null = $state(null);
  let language = $state<LanguageCode>('en');
  let t = $derived((key: string) => translations[language][key] || key);
  let autoStartEnabled = $state(false);
  let pushSettings = $state<PushSettings>({
    global_enabled: true,
    receive_all_apps: true,
    global_min_priority: 0,
    apps: {}
  });

  let tray: TrayIcon | null = null;
  $effect(() => {
    const currentLang = language;
    const currentAutostart = autoStartEnabled;
    const buildTray = async () => {
      try {
        const appWindow = getCurrentWindow();
        const items = [
          await MenuItem.new({ id: 'show', text: t('tray.show'), action: () => { appWindow.show(); appWindow.setFocus(); } }),
          await MenuItem.new({ id: 'settings', text: t('tray.settings'), action: () => { showSettings = true; appWindow.show(); appWindow.setFocus(); } }),
          await PredefinedMenuItem.new({ item: 'Separator' }),
          await CheckMenuItem.new({ 
            id: 'autostart', 
            text: t('settings.autostart'), 
            checked: currentAutostart, 
            action: async () => { 
              autoStartEnabled = !autoStartEnabled; 
              if (autoStartEnabled) await enable(); else await disable();
            } 
          }),
          await PredefinedMenuItem.new({ item: 'Separator' }),
          await MenuItem.new({ id: 'quit', text: t('tray.quit'), action: () => { invoke('quit_app'); } })
        ];
        
        const menu = await Menu.new({ items });
        
        if (!tray) {
          try {
            tray = await TrayIcon.getById('main');
          } catch(e) {}
        }
        
        if (tray) {
          await tray.setMenu(menu);
        }
      } catch (e) {
        console.error('Failed to rebuild tray:', e);
      }
    };
    buildTray();
  });

  let isSaving = $state(false);
  let messages: GotifyMessage[] = $state([]);
  let apps: GotifyApplication[] = $state([]);
  let selectedAppId: number | null = $state(null);
  let isLoadingData = $state(false);
  let errorMessage = $state('');

  // States for detail view
  let detailMessageId: number | null = $state(null);
  let detailMessage: GotifyMessage | null = $state(null);
  
  let confirmDeleteId: number | null = $state(null);

  function extractVerificationCode(text: string | null | undefined): string | null {
    if (!text) return null;
    const regex = /(?:驗證碼|验证码|code|pin|otp|passcode)(?:\s*:|：|\s+)?\s*([A-Za-z0-9]{4,8})\b/i;
    const match = text.match(regex);
    return match ? match[1] : null;
  }

  let verificationCode = $derived(detailMessage ? (extractVerificationCode((detailMessage as GotifyMessage).title) || extractVerificationCode((detailMessage as GotifyMessage).message)) : null);
  let copySuccess = $state(false);
  let listCopiedId: number | null = $state(null);

  async function copyFromList(id: number, code: string) {
    try {
      await navigator.clipboard.writeText(code);
      listCopiedId = id;
      setTimeout(() => { if (listCopiedId === id) listCopiedId = null; }, 2000);
    } catch (err) {
      console.error('Failed to copy: ', err);
    }
  }

  async function copyVerificationCode() {
    if (verificationCode) {
      try {
        await navigator.clipboard.writeText(verificationCode);
        copySuccess = true;
        setTimeout(() => copySuccess = false, 2000);
      } catch (err) {
        console.error('Failed to copy: ', err);
      }
    }
  }
  let deleteAllConfirmState = $state(0);
  let isDeletingAll = $state(false);
  let showSettings = $state(false);
  let wsStatus = $state('disconnected');
  let justEnabledPush = $state(false);
  let mobileView = $state<'master' | 'detail'>('master');
  let recentServers: { url: string, token: string }[] = $state([]);

  function renderMarkdown(text: string) {
    if (!text) return '';
    try {
      const rawHtml = marked.parse(text, { breaks: true, gfm: true }) as string;
      return DOMPurify.sanitize(rawHtml);
    } catch (e) {
      console.warn("Markdown parsing error", e);
      return text;
    }
  }

  let filteredMessages = $derived(
    selectedAppId === null 
      ? messages 
      : messages.filter(m => m.appid === selectedAppId)
  );

  $effect(() => {
    if (selectedAppId !== undefined) {
      deleteAllConfirmState = 0;
    }
  });

  onMount(() => {
    // Fetch system fonts
    invoke<string[]>('get_system_fonts').then(fonts => {
      systemFonts = fonts;
    }).catch(e => console.error("Failed to load system fonts", e));

    const urlParams = new URLSearchParams(window.location.search);
    const viewParam = urlParams.get('view');
    
    if (viewParam === 'detail') {
      const msgIdStr = urlParams.get('id');
      if (msgIdStr) {
        detailMessageId = parseInt(msgIdStr);
        currentView = 'detail';
        invoke<GotifyMessage>('get_message_by_id', { id: detailMessageId })
          .then(async msg => {
            detailMessage = msg;
            await tick();
            adjustWindowSize();
          })
          .catch(e => errorMessage = String(e));
      } else {
        errorMessage = "Invalid Message ID";
        currentView = 'detail';
      }
      return; // Skip normal main view init
    }

    // Main window initialization
    load('settings.json').then(async s => {
      store = s;
      const savedUrl = await store.get('gotify_url');
      const savedToken = await store.get('gotify_token');
      const savedDateFormat = await store.get('date_format');
      const savedPushSettings = await store.get('push_settings');
      const savedFontPrimary = await store.get('font_primary');
      const savedFontFallback = await store.get('font_fallback');
      const savedLanguage = await store.get('language');
      const savedRecentServers = await store.get('recent_servers');
      const savedAutostartInit = await store.get('autostart_init');

      if (!savedAutostartInit) {
        try {
          await enable();
          autoStartEnabled = true;
          await store.set('autostart_init', true);
          await store.save();
        } catch (e) {
          console.error('Failed to init autostart:', e);
        }
      } else {
        try {
          autoStartEnabled = await isEnabled();
        } catch (e) {}
      }
      
      if (savedLanguage) {
        language = savedLanguage as LanguageCode;
      } else {
        language = getSystemLanguage();
      }

      if (savedRecentServers && Array.isArray(savedRecentServers)) {
        recentServers = savedRecentServers;
      }
      
      if (savedDateFormat) {
        dateFormat = savedDateFormat as string;
      }
      if (savedFontPrimary) fontPrimary = savedFontPrimary as string;
      if (savedFontFallback) fontFallback = savedFontFallback as string;
      if (savedPushSettings) {
        pushSettings = savedPushSettings as PushSettings;
      }
      
      if (savedUrl && savedToken) {
        url = savedUrl as string;
        token = savedToken as string;
        currentView = 'messages';
        loadData();
      } else {
        currentView = 'login';
      }
    }).catch(e => {
      console.warn("載入設定失敗:", e);
      currentView = 'login';
    });

    let unlistenMessage: (() => void) | undefined;
    listen<GotifyMessage>('gotify-message', (event) => {
      messages = [event.payload, ...messages];
    }).then(unlisten => {
      unlistenMessage = unlisten;
    });

    let unlistenDetail: (() => void) | undefined;
    listen<number>('open-detail', (event) => {
      const msgId = event.payload;
      if (msgId) {
        invoke('create_detail_window', { id: msgId }).catch(e => {
            console.error("Failed to open window:", e);
        });
      }
    }).then(unlisten => {
      unlistenDetail = unlisten;
    });

    listen<string>('ws-status', (event) => {
      wsStatus = event.payload;
    });

    invoke<string>('get_ws_status').then(status => {
      if (status) wsStatus = status;
    }).catch(e => console.error("Failed to fetch initial ws status", e));

    return () => {
      if (unlistenMessage) unlistenMessage();
      if (unlistenDetail) unlistenDetail();
    };
  });

  async function loadData() {
    isLoadingData = true;
    errorMessage = '';
    try {
      const [fetchedMessages, fetchedApps] = await Promise.all([
        invoke<GotifyMessage[]>('fetch_messages', { url, token }),
        invoke<GotifyApplication[]>('fetch_applications', { url, token })
      ]);
      messages = fetchedMessages;
      apps = fetchedApps;
    } catch (e) {
      console.error('Fetch data failed:', e);
      errorMessage = String(e);
    } finally {
      isLoadingData = false;
    }
  }

  let deleteTimeoutId: ReturnType<typeof setTimeout> | null = null;

  async function deleteMessage(id: number) {
    if (confirmDeleteId !== id) {
      confirmDeleteId = id;
      if (deleteTimeoutId) clearTimeout(deleteTimeoutId);
      deleteTimeoutId = setTimeout(() => {
        if (confirmDeleteId === id) {
          confirmDeleteId = null;
        }
      }, 3000);
      return;
    }
    if (deleteTimeoutId) clearTimeout(deleteTimeoutId);
    try {
      await invoke('delete_message', { url, token, id });
      messages = messages.filter(m => m.id !== id);
      confirmDeleteId = null;
      if (currentView === 'detail' && detailMessageId === id) {
        getCurrentWebviewWindow().close();
      }
    } catch (e) {
      console.error('Failed to delete message:', e);
      errorMessage = `${t('error.deleteFailed')} ${e}`;
      confirmDeleteId = null;
    }
  }

  let urlError = $state('');

  function validateUrl() {
    if (!url) return true;
    urlError = '';
    let processedUrl = url.trim();
    if (!/^https?:\/\//i.test(processedUrl)) {
      processedUrl = 'https://' + processedUrl;
      url = processedUrl;
    }
    try {
      new URL(processedUrl);
      return true;
    } catch (e) {
      urlError = t('form.invalidUrl');
      return false;
    }
  }

  async function saveSettings() {
    if (!validateUrl()) return;
    isSaving = true;
    errorMessage = '';
    try {
      // Verify connection first
      await invoke('fetch_applications', { url, token });
      
      if (!store) {
        store = await load('settings.json');
      }
      
      // Update recent servers
      recentServers = [{ url, token }, ...recentServers.filter(s => s.url !== url)].slice(0, 5);
      await store.set('recent_servers', recentServers);
      
      await store.set('gotify_url', url);
      await store.set('gotify_token', token);
      await store.save();
      
      await invoke('restart_websocket');
      
      currentView = 'messages';
      loadData();
    } catch (e) {
      console.error('連線驗證失敗:', e);
      errorMessage = `${t('form.authFailed')} ${e}`;
    } finally {
      isSaving = false;
    }
  }

  async function savePushSettings() {
    if (store) {
      await store.set('push_settings', pushSettings);
      await store.save();
    }
  }

  async function handleDeleteAll() {
    if (deleteAllConfirmState < 2) {
      deleteAllConfirmState++;
      return;
    }
    
    if (selectedAppId === null) return;
    
    isDeletingAll = true;
    try {
      await invoke('delete_all_messages', { url, token, appId: selectedAppId });
      messages = messages.filter(m => m.appid !== selectedAppId);
    } catch (e: any) {
      errorMessage = `Error deleting messages: ${e}`;
    } finally {
      isDeletingAll = false;
      deleteAllConfirmState = 0;
    }
  }

  async function saveSettingsInline() {
    isSaving = true;
    errorMessage = '';
    try {
      if (!store) {
        store = await load('settings.json');
      }
      await store.set('gotify_url', url);
      await store.set('gotify_token', token);
      await store.set('date_format', dateFormat);
      await store.set('font_primary', fontPrimary);
      await store.set('font_fallback', fontFallback);
      await store.set('language', language);
      await store.set('push_settings', pushSettings);
      await store.save();
      
      try {
        if (autoStartEnabled) {
          await enable();
        } else {
          await disable();
        }
      } catch (e) {
        console.error('Failed to toggle autostart:', e);
      }

      errorMessage = '';
      showSettings = false;
      loadData();
    } catch (e) {
      console.error('儲存失敗:', e);
      errorMessage = `${t('error.saveFailed')} ${e}`;
    } finally {
      isSaving = false;
    }
  }

  async function logout() {
    try {
      if (store) {
        await store.delete('gotify_url');
        await store.delete('gotify_token');
        await store.delete('date_format');
        await store.delete('font_primary');
        await store.delete('font_fallback');
        await store.delete('language');
        await store.delete('push_settings');
        await store.save();
      }
      url = '';
      token = '';
      messages = [];
      apps = [];
      selectedAppId = null;
      await store.set('gotify_url', '');
      await store.set('gotify_token', '');
      await invoke('restart_websocket');
      
      currentView = 'login';
    } catch (e) {
      console.error("Logout failed:", e);
    }
  }

  function formatDate(isoDate: string) {
    const d = new Date(isoDate);
    
    if (dateFormat === 'iso') {
      const pad = (n: number) => n.toString().padStart(2, '0');
      return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
    }
    
    if (dateFormat === 'system') {
      return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false });
    }
    
    return d.toLocaleString(dateFormat, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false });
  }

  function getRelativeTime(isoDate: string) {
    const d = new Date(isoDate);
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHour = Math.floor(diffMin / 60);
    const diffDay = Math.floor(diffHour / 24);

    if (diffDay >= 7) {
      return formatDate(isoDate);
    }
    
    if (diffDay > 0) {
      return `${diffDay} ${t('time.daysAgo')}`;
    }
    if (diffHour > 0) {
      return `${diffHour} ${t('time.hoursAgo')}`;
    }
    if (diffMin > 0) {
      return `${diffMin} ${t('time.minutesAgo')}`;
    }
    return t('time.justNow');
  }

  function getPriorityColor(priority: number) {
    if (priority > 5) return 'bg-red-500';
    if (priority > 2) return 'bg-amber-400';
    return 'bg-blue-500';
  }

  function getPriorityTextColor(priority: number) {
    if (priority > 5) return 'text-red-500';
    if (priority > 2) return 'text-amber-500';
    return 'text-blue-500';
  }

  async function adjustWindowSize() {
    try {
      const contentEl = document.getElementById('detail-content-inner');
      if (contentEl) {
        const contentHeight = contentEl.scrollHeight;
        const headerHeight = 60; // Approximate header height
        const verticalPadding = 64; // p-8 is 32px top and bottom = 64px
        let targetHeight = contentHeight + headerHeight + verticalPadding;
        
        const MAX_HEIGHT = 800;
        const MIN_HEIGHT = 200;
        
        if (targetHeight > MAX_HEIGHT) targetHeight = MAX_HEIGHT;
        if (targetHeight < MIN_HEIGHT) targetHeight = MIN_HEIGHT;
        
        await invoke('resize_window', { label: 'detail_' + detailMessageId, width: 500, height: targetHeight });
        await invoke('show_window', { label: 'detail_' + detailMessageId });
      }
    } catch (e) {
      console.warn("Failed to resize window", e);
    }
  }

  $effect(() => {
    const primary = fontPrimary ? `"${fontPrimary}"` : 'system-ui';
    const fallback = fontFallback ? `"${fontFallback}"` : 'sans-serif';
    document.body.style.setProperty('font-family', `${primary}, ${fallback}, sans-serif`, 'important');
  });
</script>

<main class="h-screen bg-white text-black relative overflow-hidden flex flex-col selection:bg-black selection:text-white antialiased">

  {#if currentView === 'loading'}
    <div class="flex-1 flex items-center justify-center relative z-10">
      <div class="animate-spin h-5 w-5 text-gray-400">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
      </div>
    </div>
  {:else if currentView === 'detail'}
    <header class="bg-white border-b border-gray-200 px-6 py-4 flex items-center justify-between sticky top-0 z-20">
      <h1 class="text-sm font-semibold tracking-tight text-gray-400 uppercase">{t('detail.title')}</h1>
      <div class="flex items-center space-x-3">
        {#if detailMessage}
          <button 
            class={`px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${confirmDeleteId === detailMessage?.id ? 'bg-red-500 text-white hover:bg-red-600' : 'bg-gray-100 text-gray-600 hover:bg-gray-200'}`}
            onclick={() => detailMessage && deleteMessage(detailMessage.id)}
          >
            {confirmDeleteId === detailMessage.id ? t('common.confirmDelete') : t('common.delete')}
          </button>
        {/if}
      </div>
    </header>
    <div class="flex-1 overflow-y-auto p-8 custom-scrollbar">
      {#if errorMessage}
        <div class="bg-red-50 border border-red-200 text-red-600 rounded-md p-4 text-sm">
          {errorMessage}
        </div>
      {:else if detailMessage}
        <div id="detail-content-inner" class="max-w-2xl mx-auto w-full space-y-6">
          <div>
            <div class="flex items-center space-x-2 mb-3">
              <div class="w-3 h-3 flex items-center justify-center group/dot cursor-default" title={`Priority: ${detailMessage.priority}`}>
                <div class={`w-2.5 h-2.5 rounded-full ${getPriorityColor(detailMessage.priority)} group-hover/dot:hidden transition-all`}></div>
                <span class={`hidden group-hover/dot:block text-xs font-bold leading-none ${getPriorityTextColor(detailMessage.priority)}`}>{detailMessage.priority}</span>
              </div>
              <div class="group/time cursor-default text-xs text-gray-400 tracking-tighter">
                <span class="group-hover/time:hidden">{getRelativeTime(detailMessage.date)}</span>
                <span class="hidden group-hover/time:block">{formatDate(detailMessage.date)}</span>
              </div>
            </div>
            <h1 class="text-3xl font-bold tracking-tight text-black leading-tight">
              {detailMessage.title || t('common.notification')}
            </h1>
          </div>

          {#if verificationCode}
            <div class="p-4 bg-gray-50 border border-gray-200 rounded-lg flex items-center justify-between">
              <div class="flex items-center space-x-3">
                <div class="w-8 h-8 rounded-full bg-white border border-gray-200 flex items-center justify-center text-gray-500">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"></path></svg>
                </div>
                <div>
                  <div class="text-xs text-gray-500 font-medium uppercase tracking-wider mb-0.5">{t('detail.verificationCode')}</div>
                  <div class="font-mono text-lg font-bold text-black tracking-widest">{verificationCode}</div>
                </div>
              </div>
              <button 
                onclick={copyVerificationCode}
                class={`px-4 py-2 rounded-md text-sm font-medium transition-colors flex items-center space-x-2 ${copySuccess ? 'bg-green-500 text-white' : 'bg-black text-white hover:bg-gray-800'}`}
              >
                {#if copySuccess}
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path></svg>
                  <span>{t('detail.copied')}</span>
                {:else}
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"></path></svg>
                  <span>{t('detail.copyCode')}</span>
                {/if}
              </button>
            </div>
          {/if}
          
          <div class="h-px w-full bg-gray-200"></div>
          
          <div class="text-base text-gray-700 leading-relaxed whitespace-pre-wrap break-words markdown-content">
            {@html renderMarkdown(detailMessage.message)}
          </div>
        </div>
      {:else}
        <div class="flex-1 flex items-center justify-center h-full">
          <div class="animate-spin h-5 w-5 text-gray-400">...</div>
        </div>
      {/if}
    </div>
  {:else if currentView === 'login'}
    <div class="flex-1 flex flex-col p-6 relative z-10 items-center justify-center bg-white">
      <div class="w-full max-w-[360px]">
        <div class="text-center mb-10">
          <h1 class="text-2xl font-bold tracking-tight text-black mb-2">{t('login.title')}</h1>
          <p class="text-gray-500 text-sm">{t('login.subtitle')}</p>
        </div>

        {#if errorMessage && currentView === 'login'}
          <div class="mb-6 bg-red-50 border border-red-200 text-red-600 rounded-md p-3 text-sm">
            {errorMessage}
          </div>
        {/if}

        <form class="space-y-4" onsubmit={(e) => { e.preventDefault(); saveSettings(); }}>
          <div class="space-y-1.5">
            <label for="url" class="block text-sm font-medium text-black">{t('login.serverUrl')}</label>
            <input 
              type="url" 
              id="url"
              bind:value={url}
              onblur={validateUrl}
              placeholder="https://gotify.example.com"
              required
              autocomplete="off"
              spellcheck="false"
              class={`w-full bg-white border ${urlError ? 'border-red-500 focus:border-red-500 focus:ring-red-500' : 'border-gray-200 focus:border-black focus:ring-black'} rounded-md px-3 py-2 text-sm text-black placeholder-gray-400 focus:outline-none focus:ring-1 transition-colors`}
            />
            {#if urlError}
              <p class="text-xs text-red-500 mt-1">{urlError}</p>
            {/if}
          </div>

          <div class="space-y-1.5">
            <label for="token" class="block text-sm font-medium text-black">{t('login.clientToken')}</label>
            <input 
              type="password" 
              id="token"
              bind:value={token}
              placeholder="Client Token"
              required
              autocomplete="off"
              spellcheck="false"
              class="w-full bg-white border border-gray-200 rounded-md px-3 py-2 text-sm text-black placeholder-gray-400 focus:outline-none focus:border-black focus:ring-1 focus:ring-black transition-colors"
            />
          </div>

          <div class="pt-2">
            <button 
              type="submit" 
              disabled={isSaving}
              class="w-full h-9 bg-black text-white rounded-md px-4 py-2 text-sm font-medium hover:bg-gray-800 transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center"
            >
              {#if isSaving}
                <svg class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                  <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                  <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                {t('login.connecting')}
              {:else}
                {t('login.loginBtn')}
              {/if}
            </button>
          </div>
        </form>

        {#if recentServers.length > 0}
          <div class="mt-8 pt-6 border-t border-gray-100">
            <h2 class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-3">{t('login.recentServers')}</h2>
            <div class="space-y-2">
              {#each recentServers as server}
                <div 
                  class="flex items-center justify-between bg-gray-50 border border-gray-100 hover:border-gray-300 rounded-md px-3 py-2 transition-colors cursor-pointer group" 
                  onclick={() => { url = server.url; token = server.token; }}
                >
                  <div class="flex-1 min-w-0 pr-3">
                    <div class="text-sm font-medium text-gray-700 truncate">{server.url}</div>
                  </div>
                  <button 
                    class="p-1 rounded text-gray-300 opacity-0 group-hover:opacity-100 hover:text-red-500 hover:bg-white transition-all"
                    title="Remove from history"
                    onclick={(e) => {
                      e.stopPropagation();
                      recentServers = recentServers.filter(s => s.url !== server.url);
                      if (store) {
                        store.set('recent_servers', recentServers);
                        store.save();
                      }
                    }}
                  >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path></svg>
                  </button>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </div>
  {:else if currentView === 'messages'}
    <header class="bg-white border-b border-gray-200 px-6 h-14 flex items-center justify-between shrink-0 z-20">
      <div class="flex items-center space-x-2">
        <div class="w-6 h-6 shrink-0 hidden sm:flex">
          <img src="/logo.png" alt="GotiDesk Logo" class="w-full h-full object-contain rounded" />
        </div>
        <h1 class="text-sm font-semibold tracking-tight text-black shrink-0 hidden sm:block">GotiDesk</h1>
        <div class="sm:ml-3 sm:pl-3 sm:border-l border-gray-200 flex items-center space-x-2 text-xs font-medium px-1 sm:px-2">
          <div class={`w-2 h-2 rounded-full shrink-0 hidden sm:block ${wsStatus === 'connected' ? 'bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.6)]' : wsStatus === 'connecting' ? 'bg-yellow-500 animate-pulse' : 'bg-red-500'}`}></div>
          <button 
            class="text-gray-500 max-w-[150px] truncate hidden sm:block cursor-pointer hover:text-black hover:underline transition-colors focus:outline-none" 
            title={url}
            onclick={() => { if (url) open(url); }}
          >
            {url ? url.replace(/^https?:\/\//, '').replace(/\/$/, '') : 'Not Connected'}
          </button>
          <span class="text-gray-300 px-1 hidden sm:block">|</span>
          <span class={`flex items-center space-x-1 ${pushSettings.global_enabled ? 'text-green-600' : 'text-gray-400'}`}>
            {#if pushSettings.global_enabled}
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"></path></svg>
              <span>Push ON</span>
            {:else}
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"></path>
                <line x1="4" y1="4" x2="20" y2="20" stroke="currentColor" stroke-width="2" stroke-linecap="round"></line>
              </svg>
              <span>Push OFF</span>
            {/if}
          </span>
        </div>
      </div>
      <div class="flex items-center space-x-3">
        <button 
          title="Settings"
          onclick={() => showSettings = true}
          class={`p-2 rounded-md transition-colors ${showSettings ? 'bg-gray-200 text-black' : 'text-gray-400 hover:text-black hover:bg-gray-100'}`}
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"></path>
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"></path>
          </svg>
        </button>
        <button 
          onclick={logout}
          class="text-xs font-medium text-gray-500 hover:text-black border border-gray-200 bg-white hover:bg-gray-50 px-3 py-1.5 rounded-md transition-colors"
        >
          {t('common.logout')}
        </button>
      </div>
    </header>

    {#if activePopover !== null}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="fixed inset-0 z-40" onclick={() => activePopover = null}></div>
      
      {#if activePopover.id === 'global'}
        <div class="fixed bg-white border border-gray-200 rounded-md shadow-lg z-50 p-4 text-gray-800 animate-slide-up" style="top: {activePopover.top}px; left: {activePopover.left}px; width: 256px;" onclick={(e) => e.stopPropagation()}>
          <h3 class="text-xs font-bold uppercase tracking-wider text-gray-500 mb-3">{t('sidebar.globalNotifSettings')}</h3>
          <div class="space-y-4">
            <div class="flex items-center justify-between">
              <label class="text-sm font-medium">{t('settings.enablePush')}</label>
              <input type="checkbox" bind:checked={pushSettings.global_enabled} onchange={savePushSettings} class="h-4 w-4 text-black focus:ring-black border-gray-300 rounded" />
            </div>
            <div class="space-y-2">
              <label class="block text-sm text-gray-600">{t('settings.globalMinPriority')}</label>
              <input type="number" min="0" max="10" bind:value={pushSettings.global_min_priority} onchange={savePushSettings} class="w-full h-8 px-2 border border-gray-300 rounded text-sm focus:outline-none focus:border-black" />
            </div>
          </div>
        </div>
      {:else}
        {@const app = apps.find(a => a.id.toString() === activePopover?.id)}
        {#if app}
          <div class="fixed bg-white border border-gray-200 rounded-md shadow-lg z-50 p-4 text-gray-800 animate-slide-up" style="top: {activePopover.top}px; left: {activePopover.left}px; width: 256px;" onclick={(e) => e.stopPropagation()}>
            <h3 class="text-xs font-bold uppercase tracking-wider text-gray-500 mb-3 truncate">{app.name} Settings</h3>
            <div class="space-y-4">
              <div class="flex items-center justify-between">
                <label class="text-sm font-medium">{t('settings.enablePush')}</label>
                <input type="checkbox" 
                  checked={pushSettings.apps[app.id.toString()]?.enabled ?? true}
                  onchange={(e) => {
                    const checked = e.currentTarget.checked;
                    if (!pushSettings.apps[app.id.toString()]) {
                      pushSettings.apps[app.id.toString()] = { enabled: true, min_priority: null };
                    }
                    pushSettings.apps[app.id.toString()].enabled = checked;
                    savePushSettings();
                  }}
                  class="h-4 w-4 text-black focus:ring-black border-gray-300 rounded" />
              </div>
              <div class="space-y-2">
                <label class="flex items-center space-x-2 text-sm text-gray-600">
                  <input type="checkbox" 
                    checked={pushSettings.apps[app.id.toString()]?.min_priority !== null && pushSettings.apps[app.id.toString()]?.min_priority !== undefined}
                    onchange={(e) => {
                      const checked = e.currentTarget.checked;
                      if (!pushSettings.apps[app.id.toString()]) {
                        pushSettings.apps[app.id.toString()] = { enabled: true, min_priority: null };
                      }
                      if (checked) {
                        pushSettings.apps[app.id.toString()].min_priority = pushSettings.global_min_priority;
                      } else {
                        pushSettings.apps[app.id.toString()].min_priority = null;
                      }
                      savePushSettings();
                    }}
                    class="h-4 w-4 text-black focus:ring-black border-gray-300 rounded" />
                  <span>{t('settings.setCustomPriority')}</span>
                </label>
                {#if pushSettings.apps[app.id.toString()]?.min_priority !== null && pushSettings.apps[app.id.toString()]?.min_priority !== undefined}
                  <input type="number" min="0" max="10" 
                    bind:value={pushSettings.apps[app.id.toString()].min_priority} 
                    onchange={savePushSettings}
                    class="w-full h-8 px-2 border border-gray-300 rounded text-sm focus:outline-none focus:border-black animate-slide-up" />
                {/if}
              </div>
            </div>
          </div>
        {/if}
      {/if}
    {/if}

    <div class="flex flex-1 overflow-hidden relative">
      <!-- Sidebar -->
      <aside class={`border-r border-gray-200 bg-[#FAFAFA] flex-col overflow-y-auto custom-scrollbar shrink-0 ${mobileView === 'master' ? 'flex w-full z-10' : 'hidden'} sm:flex sm:w-fit sm:min-w-[200px] sm:max-w-[320px] sm:static`}>
        {#if showSettings}
          <div class="p-4">
            <button 
              onclick={() => showSettings = false}
              class="w-full text-left px-3 py-2 mb-6 rounded-md text-sm font-medium text-gray-600 hover:text-black hover:bg-gray-100 transition-colors flex items-center space-x-2"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 19l-7-7m0 0l7-7m-7 7h18"></path>
              </svg>
              <span>{t('common.back')}</span>
            </button>
            <h2 class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-3 px-3">{t('sidebar.settings')}</h2>
            <nav class="space-y-1">
              <a href="#settings-general" class="block w-full text-left px-3 py-2 rounded-md text-sm transition-colors text-gray-600 hover:bg-gray-100 hover:text-black">
                {t('settings.general')}
              </a>
              <a href="#settings-notifications" class="block w-full text-left px-3 py-2 rounded-md text-sm transition-colors text-gray-600 hover:bg-gray-100 hover:text-black">
                {t('settings.notifications')}
              </a>
            </nav>
          </div>
        {:else}
          <div class="p-4">
            <h2 class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-3">{t('sidebar.applications')}</h2>
            <nav class="space-y-1 relative">
              <div class="relative group w-full">
                <button 
                  onclick={() => { selectedAppId = null; mobileView = 'detail'; }}
                  class={`w-full text-left px-3 py-2 rounded-md text-sm transition-colors ${selectedAppId === null ? 'bg-black text-white font-medium' : 'text-gray-600 hover:bg-gray-100 hover:text-black'}`}
                >
                  <span class="block pr-6 truncate">{t('sidebar.allMessages')}</span>
                </button>
                <button 
                  onclick={(e) => { 
                    e.stopPropagation(); 
                    if (activePopover?.id === 'global') {
                      activePopover = null;
                    } else {
                      const rect = e.currentTarget.getBoundingClientRect();
                      activePopover = { id: 'global', top: rect.bottom + 8, left: rect.left };
                    }
                  }}
                  class={`absolute right-1 top-1/2 -translate-y-1/2 p-1 rounded-md transition-all shrink-0 ${
                    activePopover?.id === 'global' ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'
                  } ${
                    selectedAppId === null 
                      ? (activePopover?.id === 'global' ? 'bg-white/20 text-white' : 'text-white hover:bg-white/20')
                      : (activePopover?.id === 'global' ? 'bg-black/10 text-black' : 'text-gray-400 hover:text-black hover:bg-black/10')
                  }`}
                  title="Global Notification Settings"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"></path><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"></path></svg>
                </button>
              </div>
              {#each apps as app}
                <div class="relative group w-full">
                  <button 
                    onclick={() => { selectedAppId = app.id; mobileView = 'detail'; }}
                    class={`w-full text-left px-3 py-2 rounded-md text-sm transition-colors ${selectedAppId === app.id ? 'bg-black text-white font-medium' : 'text-gray-600 hover:bg-gray-100 hover:text-black'}`}
                  >
                    <span class="block pr-6 truncate">{app.name}</span>
                  </button>
                  <button 
                    onclick={(e) => { 
                      e.stopPropagation(); 
                      if (activePopover?.id === app.id.toString()) {
                        activePopover = null;
                      } else {
                        const rect = e.currentTarget.getBoundingClientRect();
                        activePopover = { id: app.id.toString(), top: rect.bottom + 8, left: rect.left };
                      }
                    }}
                    class={`absolute right-1 top-1/2 -translate-y-1/2 p-1 rounded-md transition-all shrink-0 ${
                      activePopover?.id === app.id.toString() ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'
                    } ${
                      selectedAppId === app.id 
                        ? (activePopover?.id === app.id.toString() ? 'bg-white/20 text-white' : 'text-white hover:bg-white/20')
                        : (activePopover?.id === app.id.toString() ? 'bg-black/10 text-black' : 'text-gray-400 hover:text-black hover:bg-black/10')
                    }`}
                    title="Notification Settings"
                  >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"></path><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"></path></svg>
                  </button>
                </div>
              {/each}
            </nav>
          </div>
        {/if}
      </aside>

      <!-- Main Content -->
      <main class={`flex-1 flex-col bg-white overflow-hidden relative ${mobileView === 'detail' ? 'flex w-full z-10' : 'hidden'} sm:flex sm:static`}>
        {#if showSettings}
          <div class="flex-1 overflow-y-auto p-8 custom-scrollbar">
            <div class="max-w-2xl w-full space-y-8 scroll-smooth">
              
              <form onsubmit={(e) => { e.preventDefault(); saveSettingsInline(); }} class="space-y-12 max-w-md pb-10">
                <!-- General Section -->
                <div class="space-y-6">
                  <h2 id="settings-general" class="text-xl font-bold tracking-tight text-black border-b border-gray-100 pb-2 pt-4">{t('settings.general')}</h2>
                  
                  <div class="flex items-center justify-between pb-2">
                    <label for="settings-autostart" class="block text-sm font-medium text-gray-700">{t('settings.autostart')}</label>
                    <input type="checkbox" id="settings-autostart" bind:checked={autoStartEnabled} class="h-4 w-4 text-black focus:ring-black border-gray-300 rounded" />
                  </div>

                  <div>
                    <label for="settings-language" class="block text-sm font-medium text-gray-700 mb-1.5">{t('settings.language')}</label>
                    <select 
                      id="settings-language"
                      bind:value={language}
                      class="w-full h-10 px-3 py-2 bg-white border border-gray-300 rounded-md text-sm shadow-sm focus:outline-none focus:border-black focus:ring-1 focus:ring-black transition-colors"
                    >
                      <option value="en">English</option>
                      <option value="zh-TW">繁體中文</option>
                      <option value="zh-CN">简体中文</option>
                    </select>
                  </div>

                  <div>
                    <label for="settings-dateformat" class="block text-sm font-medium text-gray-700 mb-1.5">{t('settings.dateFormat')}</label>
                    <select 
                      id="settings-dateformat"
                      bind:value={dateFormat}
                      class="w-full h-10 px-3 py-2 bg-white border border-gray-300 rounded-md text-sm shadow-sm focus:outline-none focus:border-black focus:ring-1 focus:ring-black transition-colors"
                    >
                      <option value="system">{t('format.systemDefault')}</option>
                      <option value="en-US">{t('format.usEnglish')}</option>
                      <option value="en-GB">{t('format.ukEnglish')}</option>
                      <option value="zh-TW">{t('format.traditionalChinese')}</option>
                      <option value="iso">{t('format.iso8601')}</option>
                    </select>
                  </div>

                  <div class="space-y-4 pt-2">
                    <FontSelect 
                      label={t('settings.fontPrimary')}
                      bind:value={fontPrimary}
                      options={systemFonts}
                      placeholder="e.g., Segoe UI"
                    />
                    
                    <FontSelect 
                      label={t('settings.fontFallback')}
                      bind:value={fontFallback}
                      options={systemFonts}
                      placeholder="e.g., Microsoft YaHei"
                    />
                  </div>
                </div>

                <!-- Notifications Section -->
                <div class="space-y-6">
                  <h2 id="settings-notifications" class="text-xl font-bold tracking-tight text-black border-b border-gray-100 pb-2 pt-4">{t('settings.notifications')}</h2>
                  
                  <div class="space-y-5">
                    <div class="flex items-center justify-between">
                      <label for="settings-global-push" class="block text-sm font-medium text-gray-700">{t('settings.enablePush')}</label>
                      <input type="checkbox" id="settings-global-push" bind:checked={pushSettings.global_enabled} class="h-4 w-4 text-black focus:ring-black border-gray-300 rounded" />
                    </div>
                  </div>
                </div>

              {#if errorMessage}
                  <div class="text-red-500 text-sm font-medium">
                    {errorMessage}
                  </div>
                {/if}

                <div class="pt-2">
                  <button 
                    type="submit" 
                    disabled={isSaving}
                    class="w-full h-9 bg-black text-white rounded-md px-4 py-2 text-sm font-medium hover:bg-gray-800 transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center"
                  >
                    {#if isSaving}
                      <svg class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                      </svg>
                      {t('settings.saving')}
                    {:else}
                      {t('settings.save')}
                    {/if}
                  </button>
                </div>
              </form>
            </div>
          </div>
        {:else}
          <div class="flex-1 overflow-y-auto p-6 bg-white custom-scrollbar relative">
          <div class="max-w-2xl mx-auto w-full space-y-4 pb-10">
            <div class="mb-6 flex justify-between items-center">
              <div class="flex items-center space-x-2">
                <button 
                  class="sm:hidden p-1.5 -ml-2 rounded-md text-gray-500 hover:text-black hover:bg-gray-100 transition-colors"
                  onclick={() => mobileView = 'master'}
                  title="Back to Applications"
                >
                  <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7"></path></svg>
                </button>
                <h2 class="text-xl font-bold tracking-tight text-black">
                  {selectedAppId === null ? t('sidebar.allMessages') : apps.find(a => a.id === selectedAppId)?.name || 'Application'}
                </h2>
              </div>
              
              {#if selectedAppId !== null}
                <button 
                  class={`px-3 py-1.5 rounded-md text-xs font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
                    deleteAllConfirmState === 0 ? 'bg-red-50 text-red-600 hover:bg-red-100 border border-red-200' : 
                    deleteAllConfirmState === 1 ? 'bg-red-500 text-white hover:bg-red-600' : 
                    'bg-red-700 text-white hover:bg-red-800'
                  }`}
                  onclick={handleDeleteAll}
                  disabled={isDeletingAll || filteredMessages.length === 0}
                >
                    {#if isDeletingAll}
                    <span class="flex items-center">
                      <svg class="animate-spin -ml-1 mr-2 h-3 w-3 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                      </svg>
                      {t('master.deleting')}
                    </span>
                  {:else}
                    {deleteAllConfirmState === 0 ? t('master.deleteAll') : deleteAllConfirmState === 1 ? t('common.confirmDelete') : t('master.finalConfirm')}
                  {/if}
                </button>
              {/if}
            </div>

          {#if isLoadingData}
            <div class="flex justify-center items-center h-20">
              <svg class="animate-spin h-5 w-5 text-gray-400" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
            </div>
          {:else if errorMessage}
            <div class="bg-red-50 border border-red-200 text-red-600 rounded-md p-4 text-sm flex items-start justify-between">
              <span>{errorMessage}</span>
              <button onclick={loadData} class="text-xs font-medium bg-white border border-red-200 px-2 py-1 rounded hover:bg-red-50 transition-colors ml-4">{t('common.retry')}</button>
            </div>
          {:else if filteredMessages.length === 0}
            <div class="flex flex-col items-center justify-center h-40 text-gray-400 space-y-2">
              <svg class="w-6 h-6 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4"></path>
              </svg>
              <p class="text-sm">{t('master.noMessages')}</p>
            </div>
          {:else}
            {#each filteredMessages as msg (msg.id)}
              {@const code = extractVerificationCode(msg.title) || extractVerificationCode(msg.message)}
              <div class="bg-white border border-gray-200 rounded-lg p-5 hover:shadow-[0_2px_8px_rgba(0,0,0,0.04)] transition-all animate-slide-up group">
                <div class="flex justify-between items-start mb-1.5">
                  <div class="flex flex-col space-y-1.5">
                    <div class="flex items-center space-x-2">
                      <div class="w-3 h-3 flex items-center justify-center group/dot cursor-default" title={`Priority: ${msg.priority}`}>
                        <div class={`w-2 h-2 rounded-full ${getPriorityColor(msg.priority)} group-hover/dot:hidden transition-all`}></div>
                        <span class={`hidden group-hover/dot:block text-[11px] font-bold leading-none ${getPriorityTextColor(msg.priority)}`}>{msg.priority}</span>
                      </div>
                      <h3 class="font-semibold text-sm text-black tracking-tight leading-none">
                        {msg.title || t('common.notification')}
                      </h3>
                    </div>
                  </div>
                  <div class="flex items-center space-x-3 ml-4">
                    <button 
                      class={`px-2 py-1 rounded text-[10px] font-medium transition-all ${confirmDeleteId === msg.id ? 'bg-red-500 text-white hover:bg-red-600 opacity-100' : 'bg-gray-50 border border-gray-200 text-gray-500 hover:bg-gray-100 opacity-0 group-hover:opacity-100'}`}
                      onclick={() => deleteMessage(msg.id)}
                    >
                      {confirmDeleteId === msg.id ? t('common.confirmDelete') : t('common.delete')}
                    </button>
                  </div>
                </div>
                <div class="text-sm text-gray-600 leading-relaxed markdown-content">
                  {@html renderMarkdown(msg.message)}
                </div>
                <div class="mt-3 flex items-center justify-between">
                  <div class="flex items-center space-x-2">
                    {#if selectedAppId === null && msg.appid !== null}
                      <button 
                        class="text-[10px] font-medium px-2 py-1 bg-gray-50 border border-gray-100 text-gray-400 hover:bg-gray-100 hover:text-black rounded transition-colors"
                        onclick={(e) => { e.stopPropagation(); selectedAppId = msg.appid; mobileView = 'detail'; }}
                      >
                        {apps.find(a => a.id === msg.appid)?.name || `App ID: ${msg.appid}`}
                      </button>
                    {/if}
                    {#if code}
                      <button 
                        class={`text-[10px] font-medium px-2 py-1 rounded transition-colors flex items-center space-x-1 border ${listCopiedId === msg.id ? 'bg-green-50 text-green-600 border-green-200' : 'bg-blue-50 text-blue-600 border-blue-200 hover:bg-blue-100'}`}
                        onclick={(e) => { e.stopPropagation(); copyFromList(msg.id, code); }}
                        title="Copy Verification Code"
                      >
                        {#if listCopiedId === msg.id}
                          <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path></svg>
                          <span>{t('detail.copiedList')}</span>
                        {:else}
                          <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"></path></svg>
                          <span>{code}</span>
                        {/if}
                      </button>
                    {/if}
                  </div>
                  <div class="group/time cursor-default text-[11px] text-gray-400 tracking-tighter shrink-0">
                    <span class="group-hover/time:hidden">{getRelativeTime(msg.date)}</span>
                    <span class="hidden group-hover/time:block">{formatDate(msg.date)}</span>
                  </div>
                </div>
              </div>
            {/each}
          {/if}
          </div>
        </div>
        {/if}
      </main>
    </div>
  {/if}
</main>

<style>
  :global(body) {
    font-family: 'Noto Sans SC', 'Noto Sans TC', "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "PingFang TC", "Microsoft JhengHei", 'Geist Sans', 'Inter', -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  }
  
  .custom-scrollbar::-webkit-scrollbar {
    width: 6px;
  }
  .custom-scrollbar::-webkit-scrollbar-track {
    background: transparent;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb {
    background-color: #E5E7EB;
    border-radius: 4px;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb:hover {
    background-color: #D1D5DB;
  }

  @keyframes slide-up {
    0% { opacity: 0; transform: translateY(4px); }
    100% { opacity: 1; transform: translateY(0); }
  }
  .animate-slide-up {
    animation: slide-up 0.2s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }

  :global(.markdown-content p) {
    margin-bottom: 0.5em;
  }
  :global(.markdown-content p:last-child) {
    margin-bottom: 0;
  }
  :global(.markdown-content a) {
    color: #2563eb;
    text-decoration: underline;
  }
  :global(.markdown-content ul) {
    list-style-type: disc;
    padding-left: 1.5em;
    margin-bottom: 0.5em;
  }
  :global(.markdown-content ol) {
    list-style-type: decimal;
    padding-left: 1.5em;
    margin-bottom: 0.5em;
  }
  :global(.markdown-content blockquote) {
    border-left: 3px solid #e5e7eb;
    padding-left: 1em;
    color: #6b7280;
    margin-bottom: 0.5em;
  }
  :global(.markdown-content code) {
    background-color: #f3f4f6;
    padding: 0.1em 0.3em;
    border-radius: 0.25em;
    font-size: 0.9em;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  }
  :global(.markdown-content pre) {
    background-color: #f3f4f6;
    padding: 0.75em;
    border-radius: 0.375em;
    overflow-x: auto;
    margin-bottom: 0.5em;
  }
  :global(.markdown-content pre code) {
    background-color: transparent;
    padding: 0;
    font-size: 0.85em;
  }
</style>
