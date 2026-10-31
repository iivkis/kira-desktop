import { mount } from 'svelte';
import App from './App.svelte';
import './style.css';

const target = document.getElementById('app');
if (!target) throw new Error('Missing #app mount target');

// Remove the static HTML fallback only when the Svelte entrypoint executes.
target.replaceChildren();
mount(App, { target });
window.webkit?.messageHandlers?.kiraDiagnostics?.postMessage('Svelte entrypoint mounted');
