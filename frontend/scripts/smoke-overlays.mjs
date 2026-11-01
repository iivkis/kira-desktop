#!/usr/bin/env node
/**
 * Real browser smoke for the built, self-contained WebKit HTML document.
 * Catch host/DOM integration bugs that TypeScript and static contracts miss.
 * Runs after frontend:build in CI; requires Chrome on the runner.
 */
import { readFile, writeFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const html = await readFile(new URL('../dist/index.html', import.meta.url), 'utf8');
const chrome = process.env.CHROME_BIN || 'google-chrome';
const dir = await mkdtemp(join(tmpdir(), 'kira-overlay-smoke-'));

function scriptFor(steps) {
  return '<script>window.addEventListener("load", () => {' +
    steps.map(({ delay, kind, generation }) =>
      'setTimeout(() => {' +
        (kind === 'hide'
          ? 'window.dispatchEvent(new Event("kira:overlay:hide"));'
          : 'window.dispatchEvent(new CustomEvent("kira:overlay:show", {detail: {kind: "' +
            kind + '", generation: ' + generation + '}}));') +
        '}, ' + delay + ');'
    ).join('') + '});</script>';
}

async function smoke(name, steps, expected) {
  const document = html
    .replace('<html lang="en">', '<html lang="en" data-kira-surface="calendar">')
    .replace('</head>', scriptFor(steps) + '</head>');
  if (!document.includes('data-kira-surface="calendar"')) {
    throw new Error('Overlay marker was not inserted in the production HTML');
  }
  const path = join(dir, name + '.html');
  await writeFile(path, document);
  const output = execFileSync(chrome, [
    '--headless=new', '--no-sandbox', '--disable-gpu',
    '--disable-dev-shm-usage', '--disable-background-networking',
    '--virtual-time-budget=1800', '--dump-dom', 'file://' + path,
  ], { encoding: 'utf8', timeout: 15000, stdio: ['ignore', 'pipe', 'ignore'] });
  if (!output.includes('aria-label="' + expected + '"')) {
    throw new Error(name + ': expected Svelte ' + expected + ' DOM missing after show');
  }
  if (output.includes('id="kira-loading"')) {
    throw new Error(name + ': stale bootstrap fallback remained mounted');
  }
  console.log('OK: ' + name + ' mounted ' + expected);
}

try {
  await smoke('calendar', [{ delay: 25, kind: 'calendar', generation: 1 }], 'Kira Calendar');
  await smoke('system', [{ delay: 25, kind: 'system', generation: 1 }], 'Kira System Monitor');
  await smoke('switch', [
    { delay: 25, kind: 'calendar', generation: 1 },
    { delay: 120, kind: 'hide' },
    { delay: 140, kind: 'system', generation: 2 },
  ], 'Kira System Monitor');
} finally {
  await rm(dir, { recursive: true, force: true });
}
