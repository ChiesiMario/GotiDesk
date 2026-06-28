<script lang="ts">
  import { load } from '@tauri-apps/plugin-store';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import { onMount, tick } from 'svelte';

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

  let store: any = null;

  let currentView: 'loading' | 'login' | 'messages' | 'detail' = $state('loading');
  
  // States for messages view
  let url = $state('');
  let token = $state('');
  let isSaving = $state(false);
  let messages: GotifyMessage[] = $state([]);
  let apps: GotifyApplication[] = $state([]);
  let selectedAppId: number | null = $state(null);
  let isLoadingData = $state(false);
  let errorMessage = $state('');

  // States for detail view
  let detailMessageId: number | null = $state(null);
  let detailMessage: GotifyMessage | null = $state(null);

  let filteredMessages = $derived(
    selectedAppId === null 
      ? messages 
      : messages.filter(m => m.appid === selectedAppId)
  );

  onMount(() => {
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

  async function logout() {
    try {
      if (store) {
        await store.delete('gotify_url');
        await store.delete('gotify_token');
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
    return d.toLocaleString('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false });
  }

  function getPriorityColor(priority: number) {
    if (priority > 5) return 'bg-red-500';
    if (priority > 2) return 'bg-amber-400';
    return 'bg-blue-500';
  }

  function closeWindow() {
    getCurrentWebviewWindow().close();
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

<main class="h-screen bg-white text-black relative overflow-hidden font-sans flex flex-col selection:bg-black selection:text-white antialiased">
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
      <button 
        onclick={closeWindow}
        class="text-xs font-medium text-gray-500 hover:text-black bg-white hover:bg-gray-100 px-3 py-1.5 rounded-md transition-colors"
      >
        Close
      </button>
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
          
          <div class="text-base text-gray-700 leading-relaxed whitespace-pre-wrap break-words">
            {detailMessage.message}
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
      <button 
        onclick={logout}
        class="text-xs font-medium text-gray-500 hover:text-black border border-gray-200 bg-white hover:bg-gray-50 px-3 py-1.5 rounded-md transition-colors"
      >
        Logout
      </button>
    </header>

    <div class="flex flex-1 overflow-hidden">
      <!-- Sidebar -->
      <aside class="w-64 border-r border-gray-200 bg-[#FAFAFA] flex flex-col overflow-y-auto custom-scrollbar shrink-0">
        <div class="p-4">
          <h2 class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-3">Applications</h2>
          <nav class="space-y-1">
            <button 
              onclick={() => selectedAppId = null}
              class={`w-full text-left px-3 py-2 rounded-md text-sm transition-colors ${selectedAppId === null ? 'bg-black text-white font-medium' : 'text-gray-600 hover:bg-gray-100 hover:text-black'}`}
            >
              All Messages
            </button>
            {#each apps as app}
              <button 
                onclick={() => selectedAppId = app.id}
                class={`w-full text-left px-3 py-2 rounded-md text-sm transition-colors ${selectedAppId === app.id ? 'bg-black text-white font-medium' : 'text-gray-600 hover:bg-gray-100 hover:text-black'}`}
              >
                {app.name}
              </button>
            {/each}
          </nav>
        </div>
      </aside>

      <!-- Main Content -->
      <main class="flex-1 overflow-y-auto p-6 bg-white custom-scrollbar relative">
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
                  <span class="text-[11px] text-gray-400 font-mono tracking-tighter shrink-0 ml-4">
                    {formatDate(msg.date)}
                  </span>
                </div>
                <div class="text-sm text-gray-600 leading-relaxed pl-4">
                  {msg.message}
                </div>
              </div>
            {/each}
          {/if}
        </div>
      </main>
    </div>
  {/if}
</main>

<style>
  :global(body) {
    font-family: 'Geist Sans', 'Inter', -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
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
</style>
