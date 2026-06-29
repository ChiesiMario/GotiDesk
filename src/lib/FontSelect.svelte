<script lang="ts">
  let { value = $bindable(), options, placeholder, label } = $props<{
    value: string;
    options: string[];
    placeholder?: string;
    label: string;
  }>();

  let isOpen = $state(false);
  let searchQuery = $state('');

  let filteredOptions = $derived(
    options.filter((o: string) => o.toLowerCase().includes(searchQuery.toLowerCase()))
  );

  function toggleOpen() {
    isOpen = !isOpen;
    if (isOpen) {
      searchQuery = '';
    }
  }

  function selectOption(option: string) {
    value = option;
    isOpen = false;
  }
  
  function focusNode(node: HTMLInputElement) {
    node.focus();
  }
</script>

<div class="relative w-full">
  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">{label}</label>
  
  <button 
    type="button"
    onclick={toggleOpen}
    class="w-full flex items-center justify-between h-10 px-3 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-700 rounded-md text-sm shadow-sm focus:outline-none focus:border-black dark:focus:border-gray-500 focus:ring-1 focus:ring-black dark:focus:ring-gray-500 transition-colors"
  >
    <span class={value ? 'text-black dark:text-gray-100 truncate' : 'text-gray-400 dark:text-gray-500 truncate'}>{value || placeholder || 'Select font...'}</span>
    <svg class="w-4 h-4 text-gray-500 dark:text-gray-400 shrink-0 ml-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"></path>
    </svg>
  </button>

  {#if isOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="fixed inset-0 z-40" onclick={() => isOpen = false}></div>
    
    <div class="absolute z-50 w-full mt-1 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg overflow-hidden flex flex-col max-h-64 animate-slide-up">
      <div class="p-2 border-b border-gray-100 dark:border-gray-700 bg-gray-50 dark:bg-gray-900 shrink-0">
        <input 
          type="text" 
          use:focusNode
          bind:value={searchQuery} 
          placeholder="Search fonts..." 
          class="w-full h-8 px-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-700 rounded text-xs text-black dark:text-gray-100 focus:outline-none focus:border-black dark:focus:border-gray-500"
        />
      </div>
      <div class="overflow-y-auto custom-scrollbar flex-1 py-1">
        {#if filteredOptions.length === 0}
          <div class="px-3 py-2 text-sm text-gray-500 dark:text-gray-400 text-center">No fonts found</div>
        {:else}
          <!-- Use virtual rendering or simple each block, 300+ fonts is fast enough for each -->
          {#each filteredOptions as option}
            <button 
              type="button"
              onclick={() => selectOption(option)}
              class={`w-full text-left px-3 py-1.5 text-sm transition-colors ${value === option ? 'bg-black dark:bg-gray-700 text-white font-medium' : 'text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'}`}
            >
              {option}
            </button>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
