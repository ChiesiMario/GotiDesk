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
    options.filter(o => o.toLowerCase().includes(searchQuery.toLowerCase()))
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
  <label class="block text-sm font-medium text-gray-700 mb-1.5">{label}</label>
  
  <button 
    type="button"
    onclick={toggleOpen}
    class="w-full flex items-center justify-between h-10 px-3 bg-white border border-gray-300 rounded-md text-sm shadow-sm focus:outline-none focus:border-black focus:ring-1 focus:ring-black transition-colors"
  >
    <span class={value ? 'text-black truncate' : 'text-gray-400 truncate'}>{value || placeholder || 'Select font...'}</span>
    <svg class="w-4 h-4 text-gray-500 shrink-0 ml-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"></path>
    </svg>
  </button>

  {#if isOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="fixed inset-0 z-40" onclick={() => isOpen = false}></div>
    
    <div class="absolute z-50 w-full mt-1 bg-white border border-gray-200 rounded-md shadow-lg overflow-hidden flex flex-col max-h-64 animate-slide-up">
      <div class="p-2 border-b border-gray-100 bg-gray-50 shrink-0">
        <input 
          type="text" 
          use:focusNode
          bind:value={searchQuery} 
          placeholder="Search fonts..." 
          class="w-full h-8 px-2 bg-white border border-gray-300 rounded text-xs focus:outline-none focus:border-black"
        />
      </div>
      <div class="overflow-y-auto custom-scrollbar flex-1 py-1">
        {#if filteredOptions.length === 0}
          <div class="px-3 py-2 text-sm text-gray-500 text-center">No fonts found</div>
        {:else}
          <!-- Use virtual rendering or simple each block, 300+ fonts is fast enough for each -->
          {#each filteredOptions as option}
            <button 
              type="button"
              onclick={() => selectOption(option)}
              class={`w-full text-left px-3 py-1.5 text-sm transition-colors ${value === option ? 'bg-black text-white font-medium' : 'text-gray-700 hover:bg-gray-100'}`}
            >
              {option}
            </button>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
