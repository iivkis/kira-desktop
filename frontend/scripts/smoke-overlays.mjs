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
          ? 'window.dispatchEvent(new CustomEvent("kira:overlay:hide", {detail: {generation: ' +
            generation + '}}));'
          : 'window.dispatchEvent(new CustomEvent("kira:overlay:show", {detail: {kind: "' +
            kind + '", generation: ' + generation + '}}));') +
        '}, ' + delay + ');'
    ).join('') +
    // Test the live DOM, not strings in the bundled Svelte templates.
    'setTimeout(() => {' +
      'const roots = document.querySelectorAll("main[data-kira-ui-root]");' +
      'const label = roots.length === 1 ? roots[0].getAttribute("aria-label") : "invalid-root-count";' +
      'document.documentElement.setAttribute("data-smoke-result", label || "none");' +
    '}, 1150);' +
    '});</script>';
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
  if (!output.includes('data-smoke-result="' + expected + '"')) {
    const found = output.match(/data-smoke-result="([^"]+)"/)?.[1] ?? 'no result';
    throw new Error(name + ': expected live Svelte ' + expected + ', found ' + found);
  }
  console.log('OK: ' + name + ' mounted ' + expected);
}

try {
  await smoke('calendar', [{ delay: 25, kind: 'calendar', generation: 1 }], 'Kira Calendar');
  await smoke('system', [{ delay: 25, kind: 'system', generation: 1 }], 'Kira System Monitor');
  await smoke('switch-to-system', [
    { delay: 25, kind: 'calendar', generation: 1 },
    { delay: 120, kind: 'hide', generation: 1 },
    { delay: 140, kind: 'system', generation: 2 },
    // Must not destroy System when an older hide arrives out of order.
    { delay: 175, kind: 'hide', generation: 1 },
  ], 'Kira System Monitor');
  await smoke('switch-back-to-calendar', [
    { delay: 25, kind: 'calendar', generation: 1 },
    { delay: 120, kind: 'hide', generation: 1 },
    { delay: 145, kind: 'system', generation: 2 },
    { delay: 245, kind: 'hide', generation: 2 },
    { delay: 265, kind: 'calendar', generation: 3 },
    { delay: 280, kind: 'hide', generation: 2 },
  ], 'Kira Calendar');
} finally {
  await rm(dir, { recursive: true, force: true });
}
