import { mount } from 'svelte'
import '@fontsource/geist-sans'
import '@fontsource/geist-mono'
import './app.css'
import App from './App.svelte'

const app = mount(App, {
  target: document.getElementById('app')!,
})

// Prevent default browser context menu
document.addEventListener('contextmenu', e => {
  // Allow right click if we are clicking on an input or textarea
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
    return;
  }
  // Allow text selection to be copied, but prevent the menu itself
  const selection = window.getSelection();
  if (selection && selection.toString().length === 0) {
    e.preventDefault();
  } else if (!import.meta.env.DEV) {
     e.preventDefault(); // In prod, always prevent to feel native, user can use Ctrl+C
  }
});

// Prevent common browser shortcuts
document.addEventListener('keydown', e => {
  if (
    e.key === 'F5' || 
    (e.ctrlKey && ['r', 'R', 'p', 'P', 'f', 'F', 'g', 'G', 'u', 'U'].includes(e.key))
  ) {
    e.preventDefault();
  }
});

export default app
