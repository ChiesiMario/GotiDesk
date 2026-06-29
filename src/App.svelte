<script lang="ts">
  import { load } from '@tauri-apps/plugin-store';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import { onMount, tick } from 'svelte';
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';

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
    min_priority: number;
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
  let activePopover: { id: string, top: number, left: number } | null = $state(null);
  let pushSettings = $state<PushSettings>({
    global_enabled: true,
    receive_all_apps: true,
    global_min_priority: 0,
    apps: {}
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
  let showSettings = $state(false);
  
  let isFontLoading = $state(true);

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

  onMount(() => {
    // Background font loading
    const fontLink = document.createElement('link');
    fontLink.href = 'https://fonts.googleapis.com/css2?family=Noto+Sans+SC:wght@400;500;600;700&family=Noto+Sans+TC:wght@400;500;600;700&display=swap';
    fontLink.rel = 'stylesheet';
    fontLink.onload = () => { 
      document.fonts.ready.then(() => {
        // Ensure it shows for at least a brief moment so it doesn't flicker invisibly if cached
        setTimeout(() => { isFontLoading = false; }, 300);
      });
    };
    fontLink.onerror = () => { isFontLoading = false; };
    document.head.appendChild(fontLink);

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
      
      if (savedDateFormat) {
        dateFormat = savedDateFormat as string;
      }
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
      errorMessage = `刪除失敗: ${e}`;
      confirmDeleteId = null;
    }
  }

  async function saveSettings() {
    isSaving = true;
    errorMessage = '';
    try {
      if (!store) {
        store = await load('settings.json');
      }
      await store.set('gotify_url', url);
      await store.set('gotify_token', token);
      await store.save();
      
      await invoke('restart_websocket');
      
      currentView = 'messages';
      loadData();
    } catch (e) {
      console.error('儲存失敗:', e);
      alert('儲存失敗：' + e);
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
      await store.set('push_settings', pushSettings);
      await store.save();
      
      await invoke('restart_websocket');
      
      showSettings = false;
      loadData();
    } catch (e) {
      console.error('儲存失敗:', e);
      errorMessage = '儲存失敗：' + e;
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

  function getPriorityColor(priority: number) {
    if (priority > 5) return 'bg-red-500';
    if (priority > 2) return 'bg-amber-400';
    return 'bg-blue-500';
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
</script>

<main class="h-screen bg-white text-black relative overflow-hidden flex flex-col selection:bg-black selection:text-white antialiased">
  {#snippet fontStatusIndicator()}
    <div class="flex items-center space-x-1.5 px-2.5 py-1 rounded-md bg-gray-50 border border-gray-200 transition-colors">
      {#if isFontLoading}
        <svg class="animate-spin h-3.5 w-3.5 text-gray-500" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <span class="text-[11px] font-medium text-gray-600">加載中...</span>
      {:else}
        <svg class="h-3.5 w-3.5 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
        </svg>
        <span class="text-[11px] font-medium text-gray-600">已加載</span>
      {/if}
    </div>
  {/snippet}

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
      <h1 class="text-sm font-semibold tracking-tight text-gray-400 uppercase">Message Details</h1>
      <div class="flex items-center space-x-3">
        {#if detailMessage}
          <button 
            class={`px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${confirmDeleteId === detailMessage.id ? 'bg-red-500 text-white hover:bg-red-600' : 'bg-gray-100 text-gray-600 hover:bg-gray-200'}`}
            onclick={() => deleteMessage(detailMessage.id)}
          >
            {confirmDeleteId === detailMessage.id ? '確認刪除' : '刪除'}
          </button>
        {/if}
        {@render fontStatusIndicator()}
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
              <div class={`w-2.5 h-2.5 rounded-full ${getPriorityColor(detailMessage.priority)}`}></div>
              <span class="text-xs font-mono text-gray-400 tracking-tighter">
                {formatDate(detailMessage.date)}
              </span>
            </div>
            <h1 class="text-3xl font-bold tracking-tight text-black leading-tight">
              {detailMessage.title || 'Notification'}
            </h1>
          </div>
          
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
          <h1 class="text-2xl font-bold tracking-tight text-black mb-2">Login to GotiDesk</h1>
          <p class="text-gray-500 text-sm">Enter your Gotify server details below</p>
        </div>

        <form class="space-y-4" onsubmit={(e) => { e.preventDefault(); saveSettings(); }}>
          <div class="space-y-1.5">
            <label for="url" class="block text-sm font-medium text-black">Server URL</label>
            <input 
              type="url" 
              id="url"
              bind:value={url}
              placeholder="https://gotify.example.com"
              required
              class="w-full bg-white border border-gray-200 rounded-md px-3 py-2 text-sm text-black placeholder-gray-400 focus:outline-none focus:border-black focus:ring-1 focus:ring-black transition-colors"
            />
          </div>

          <div class="space-y-1.5">
            <label for="token" class="block text-sm font-medium text-black">Client Token</label>
            <input 
              type="password" 
              id="token"
              bind:value={token}
              placeholder="Client Token"
              required
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
                Connecting...
              {:else}
                Login
              {/if}
            </button>
          </div>
        </form>
      </div>
    </div>
  {:else if currentView === 'messages'}
    <header class="bg-white border-b border-gray-200 px-6 h-14 flex items-center justify-between shrink-0 z-20">
      <div class="flex items-center space-x-2">
        <div class="w-6 h-6 bg-black rounded flex items-center justify-center">
          <svg class="w-3.5 h-3.5 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"></path>
          </svg>
        </div>
        <h1 class="text-sm font-semibold tracking-tight text-black">GotiDesk</h1>
      </div>
      <div class="flex items-center space-x-3">
        {@render fontStatusIndicator()}
        <button 
          onclick={() => showSettings = true}
          class="text-gray-400 hover:text-black transition-colors p-1"
          title="Settings"
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
          Logout
        </button>
      </div>
    </header>

    {#if activePopover !== null}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="fixed inset-0 z-40" onclick={() => activePopover = null}></div>
      
      {#if activePopover.id === 'global'}
        <div class="fixed bg-white border border-gray-200 rounded-md shadow-lg z-50 p-4 text-gray-800 animate-slide-up" style="top: {activePopover.top}px; left: {activePopover.left}px; width: 256px;" onclick={(e) => e.stopPropagation()}>
          <h3 class="text-xs font-bold uppercase tracking-wider text-gray-500 mb-3">Global Push Settings</h3>
          <div class="space-y-4">
            <div class="flex items-center justify-between">
              <label class="text-sm font-medium">Enable Push</label>
              <input type="checkbox" bind:checked={pushSettings.global_enabled} onchange={savePushSettings} class="h-4 w-4 text-black focus:ring-black border-gray-300 rounded" />
            </div>
            <div>
              <label class="block text-sm font-medium mb-1">Global Min Priority</label>
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
                <label class="text-sm font-medium">Enable Push</label>
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
                  <span>Set Custom Priority</span>
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

    <div class="flex flex-1 overflow-hidden">
      <!-- Sidebar -->
      <aside class="w-64 border-r border-gray-200 bg-[#FAFAFA] flex flex-col overflow-y-auto custom-scrollbar shrink-0">
        {#if showSettings}
          <div class="p-4">
            <button 
              onclick={() => showSettings = false}
              class="w-full text-left px-3 py-2 mb-6 rounded-md text-sm font-medium text-gray-600 hover:text-black hover:bg-gray-100 transition-colors flex items-center space-x-2"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 19l-7-7m0 0l7-7m-7 7h18"></path>
              </svg>
              <span>Back to Messages</span>
            </button>
            <h2 class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-3 px-3">Settings</h2>
            <nav class="space-y-1">
              <a href="#settings-general" class="block w-full text-left px-3 py-2 rounded-md text-sm transition-colors text-gray-600 hover:bg-gray-100 hover:text-black">
                General
              </a>
              <a href="#settings-notifications" class="block w-full text-left px-3 py-2 rounded-md text-sm transition-colors text-gray-600 hover:bg-gray-100 hover:text-black">
                Notifications
              </a>
              <a href="#settings-connection" class="block w-full text-left px-3 py-2 rounded-md text-sm transition-colors text-gray-600 hover:bg-gray-100 hover:text-black">
                Connection
              </a>
            </nav>
          </div>
        {:else}
          <div class="p-4">
            <h2 class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-3">Applications</h2>
            <nav class="space-y-1 relative">
              <div class="relative group">
                <div class="relative group w-full">
                  <button 
                    onclick={() => selectedAppId = null}
                    class={`w-full text-left px-3 py-2 rounded-md text-sm transition-colors ${selectedAppId === null ? 'bg-black text-white font-medium' : 'text-gray-600 hover:bg-gray-100 hover:text-black'}`}
                  >
                    <span class="block pr-6 truncate">All Messages</span>
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
                    class={`absolute right-1 top-1/2 -translate-y-1/2 p-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity hover:bg-black/10 shrink-0 ${activePopover?.id === 'global' ? 'opacity-100 bg-black/10 text-black' : (selectedAppId === null ? 'text-white hover:bg-white/20' : '')}`}
                    title="Global Notification Settings"
                  >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"></path><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"></path></svg>
                  </button>
                </div>

              {#each apps as app}
                <div class="relative group w-full">
                  <button 
                    onclick={() => selectedAppId = app.id}
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
                    class={`absolute right-1 top-1/2 -translate-y-1/2 p-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity hover:bg-black/10 shrink-0 ${activePopover?.id === app.id.toString() ? 'opacity-100 bg-black/10 text-black' : (selectedAppId === app.id ? 'text-white hover:bg-white/20' : '')}`}
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
      <main class="flex-1 flex flex-col bg-white overflow-hidden relative">
        {#if showSettings}
          <div class="flex-1 overflow-y-auto p-8 custom-scrollbar">
            <div class="max-w-2xl w-full space-y-8 scroll-smooth">
              
              <form onsubmit={(e) => { e.preventDefault(); saveSettingsInline(); }} class="space-y-12 max-w-md pb-10">
                <!-- General Section -->
                <div class="space-y-6">
                  <h2 id="settings-general" class="text-xl font-bold tracking-tight text-black border-b border-gray-100 pb-2 pt-4">General</h2>
                  
                  <div>
                    <label for="settings-dateformat" class="block text-sm font-medium text-gray-700 mb-1.5">Date Format</label>
                    <select 
                      id="settings-dateformat"
                      bind:value={dateFormat}
                      class="w-full h-10 px-3 py-2 bg-white border border-gray-300 rounded-md text-sm shadow-sm focus:outline-none focus:border-black focus:ring-1 focus:ring-black transition-colors"
                    >
                      <option value="system">System Default</option>
                      <option value="en-US">US English (Oct 1, 14:30)</option>
                      <option value="en-GB">UK English (1 Oct, 14:30)</option>
                      <option value="zh-TW">Traditional Chinese (10月1日 14:30)</option>
                      <option value="iso">ISO-8601 (2023-10-01 14:30)</option>
                    </select>
                  </div>
                </div>

                <!-- Notifications Section -->
                <div class="space-y-6">
                  <h2 id="settings-notifications" class="text-xl font-bold tracking-tight text-black border-b border-gray-100 pb-2 pt-4">Notifications</h2>
                  
                  <div class="space-y-5">
                    <div class="flex items-center justify-between">
                      <label for="settings-global-push" class="block text-sm font-medium text-gray-700">Enable Push Notifications</label>
                      <input type="checkbox" id="settings-global-push" bind:checked={pushSettings.global_enabled} class="h-4 w-4 text-black focus:ring-black border-gray-300 rounded" />
                    </div>
                  </div>
                </div>

                <!-- Connection Section -->
                <div class="space-y-6">
                  <h2 id="settings-connection" class="text-xl font-bold tracking-tight text-black border-b border-gray-100 pb-2 pt-4">Connection</h2>
                  
                  <div class="space-y-5">
                    <div>
                      <label for="settings-url" class="block text-sm font-medium text-gray-700 mb-1.5">Gotify Server URL</label>
                  <input 
                    id="settings-url"
                    type="url" 
                    bind:value={url} 
                    placeholder="https://gotify.example.com" 
                    required
                    class="w-full h-10 px-3 py-2 bg-white border border-gray-300 rounded-md text-sm shadow-sm placeholder-gray-400 focus:outline-none focus:border-black focus:ring-1 focus:ring-black transition-colors"
                  />
                </div>
                
                <div>
                  <label for="settings-token" class="block text-sm font-medium text-gray-700 mb-1.5">Client Token</label>
                  <input 
                    id="settings-token"
                    type="password" 
                    bind:value={token} 
                    placeholder="Cxxxxxxxxxxxxxx" 
                    required
                    class="w-full h-10 px-3 py-2 bg-white border border-gray-300 rounded-md text-sm shadow-sm placeholder-gray-400 focus:outline-none focus:border-black focus:ring-1 focus:ring-black transition-colors"
                  />
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
                      Saving...
                    {:else}
                      Save & Restart
                    {/if}
                  </button>
                </div>
              </form>
            </div>
          </div>
        {:else}
          <div class="flex-1 overflow-y-auto p-6 bg-white custom-scrollbar relative">
          <div class="max-w-2xl mx-auto w-full space-y-4 pb-10">
            <div class="mb-6">
            <h2 class="text-xl font-bold tracking-tight text-black">
              {selectedAppId === null ? 'All Messages' : apps.find(a => a.id === selectedAppId)?.name || 'Application'}
            </h2>
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
              <button onclick={loadData} class="text-xs font-medium bg-white border border-red-200 px-2 py-1 rounded hover:bg-red-50 transition-colors ml-4">Retry</button>
            </div>
          {:else if filteredMessages.length === 0}
            <div class="flex flex-col items-center justify-center h-40 text-gray-400 space-y-2">
              <svg class="w-6 h-6 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4"></path>
              </svg>
              <p class="text-sm">No messages yet.</p>
            </div>
          {:else}
            {#each filteredMessages as msg (msg.id)}
              <div class="bg-white border border-gray-200 rounded-lg p-5 hover:shadow-[0_2px_8px_rgba(0,0,0,0.04)] transition-all animate-slide-up group">
                <div class="flex justify-between items-start mb-1.5">
                  <div class="flex items-center space-x-2">
                    <div class={`w-2 h-2 rounded-full ${getPriorityColor(msg.priority)}`}></div>
                    <h3 class="font-semibold text-sm text-black tracking-tight leading-none">
                      {msg.title || 'Notification'}
                    </h3>
                  </div>
                  <div class="flex items-center space-x-3 ml-4">
                    <button 
                      class={`px-2 py-1 rounded text-[10px] font-medium transition-all ${confirmDeleteId === msg.id ? 'bg-red-500 text-white hover:bg-red-600 opacity-100' : 'bg-gray-50 border border-gray-200 text-gray-500 hover:bg-gray-100 opacity-0 group-hover:opacity-100'}`}
                      onclick={() => deleteMessage(msg.id)}
                    >
                      {confirmDeleteId === msg.id ? '確認刪除' : '刪除'}
                    </button>
                  </div>
                </div>
                <div class="text-sm text-gray-600 leading-relaxed markdown-content">
                  {@html renderMarkdown(msg.message)}
                </div>
                <div class="mt-1 text-right">
                  <span class="text-[11px] text-gray-400 font-mono tracking-tighter shrink-0">
                    {formatDate(msg.date)}
                  </span>
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
