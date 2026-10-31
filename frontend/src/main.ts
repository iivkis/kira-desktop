import { mount } from 'svelte';
import App from './App.svelte';
import './style.css';

Reflect.set(window, '__kiraEntryStarted', true);
window.webkit?.messageHandlers?.kiraDiagnostics?.postMessage('Svelte entrypoint evaluating');
const target = document.getElementById('app');
if (!target) throw new Error('Missing #app mount target');

// Remove the static HTML fallback only when the Svelte entrypoint executes.
target.replaceChildren();
mount(App, { target });
Reflect.set(window, '__kiraMountReturned', true);
window.webkit?.messageHandlers?.kiraDiagnostics?.postMessage('Svelte entrypoint mounted');
