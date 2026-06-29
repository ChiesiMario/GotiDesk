import { mount } from 'svelte'
import '@fontsource/geist-sans'
import '@fontsource/geist-mono'
import './app.css'
import App from './App.svelte'

const app = mount(App, {
  target: document.getElementById('app')!,
})

// Prevent default browser context menu globally
document.addEventListener('contextmenu', e => {
  // In dev mode, we might want to inspect elements if we hold Shift, but the user requested strict blocking.
  // To allow emergency debugging, we only let Shift+RightClick pass through.
  if (!e.shiftKey) {
    e.preventDefault();
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
