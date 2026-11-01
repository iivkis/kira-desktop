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
const testIcon = 'data:image/svg+xml;base64,' + Buffer.from(
  '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><rect width="40" height="40" fill="#ef7d1a"/></svg>',
).toString('base64');

function scriptFor(steps) {
  const telemetry = JSON.stringify({
    core: true, niri: true,
    metrics: {
      cpu_percent: 5, cpu_temperature_c: 32.4,
      gpus: [{ label: 'GPU / NVIDIA (card1)', utilization_percent: null, temperature_c: null }],
      memory_total_kib: 15_500_000, memory_available_kib: 9_400_000,
      swap_total_kib: 7_700_000, swap_free_kib: 7_679_000,
      root_total_kib: 95_600_000, root_available_kib: 57_400_000,
      uptime_seconds: 132000, load: [0.2, 0.3, 0.15],
      battery_percent: null, battery_charging: null,
    },
  });
  return '<script>' +
    'window.webkit = {messageHandlers: {kiraCalendarUi: {postMessage(message) {' +
      'if (String(message).startsWith("size:")) {' +
        'document.documentElement.setAttribute("data-smoke-size", String(message));' +
        'const count = Number(document.documentElement.getAttribute("data-smoke-resizes") || 0) + 1;' +
        'document.documentElement.setAttribute("data-smoke-resizes", count);' +
      '}' +
      'if (String(message).startsWith("icons-request:")) {' +
        'const ids = String(message).split(":")[2].split(",").map(Number);' +
        'setTimeout(() => window.dispatchEvent(new CustomEvent("kira:launcher:icons", {detail: {icons: ids.map(id => ({id, data_url: ' +
          '(id === 0 ? null : ' + JSON.stringify(testIcon) + ')}))}})), 5);' +
      '}' +
    '}}}};' +
    'window.addEventListener("load", () => {' +
    steps.map(({ delay, kind, generation }) =>
      'setTimeout(() => {' +
        (kind === 'hide'
          ? 'window.dispatchEvent(new CustomEvent("kira:overlay:hide", {detail: {generation: ' +
            generation + '}}));'
          : kind === 'catalog'
            ? 'window.dispatchEvent(new CustomEvent("kira:launcher:apps", {detail: {apps: Array.from({length: 80}, (_, id) => ({id, name: "Application " + id, description: "Test entry"}))}}));'
            : kind === 'filter'
              ? 'const input = document.querySelector("input[aria-label=\\"Search applications\\"]");' +
                'if (input) { input.value = "Application 79"; input.dispatchEvent(new Event("input", {bubbles: true})); }'
              : kind === 'metrics'
                ? 'window.dispatchEvent(new CustomEvent("kira:system:data", {detail: ' + telemetry + '}));'
              : kind === 'pulse'
                ? 'window.dispatchEvent(new CustomEvent("kira:system:pulse", {detail: {cpu_percent: 42, memory_total_kib: 1000, memory_available_kib: 500}}));'
              : 'window.dispatchEvent(new CustomEvent("kira:overlay:show", {detail: {kind: "' +
                kind + '", generation: ' + generation + ', maxHeight: 420}}));') +
        '}, ' + delay + ');'
    ).join('') +
    // Test the live DOM, not strings in the bundled Svelte templates.
    'setTimeout(() => {' +
      'const roots = document.querySelectorAll("main[data-kira-ui-root]");' +
      'const label = roots.length === 1 ? roots[0].getAttribute("aria-label") : "invalid-root-count";' +
      'document.documentElement.setAttribute("data-smoke-result", label || "none");' +
      'const height = roots.length === 1 ? roots[0].getBoundingClientRect().height : 0;' +
      'document.documentElement.setAttribute("data-smoke-height", Math.ceil(height));' +
      'document.documentElement.setAttribute("data-smoke-overflow", roots.length === 1 && roots[0].scrollHeight > roots[0].clientHeight);' +
      'const list = document.querySelector("[role=listbox]");' +
      'document.documentElement.setAttribute("data-smoke-list-scroll", !!list && list.scrollHeight > list.clientHeight);' +
      'document.documentElement.setAttribute("data-smoke-scrollbar-hidden", !!list && getComputedStyle(list).scrollbarWidth === "none");' +
      'if (list && list.scrollHeight > list.clientHeight) list.scrollTop = 80;' +
      'document.documentElement.setAttribute("data-smoke-scrolled", !!list && list.scrollTop > 0);' +
      'const firstIcon = document.querySelector("[role=listbox] img[src^=\\"data:image/\\"]");' +
      'document.documentElement.setAttribute("data-smoke-icon-width", firstIcon ? Math.round(firstIcon.getBoundingClientRect().width) : 0);' +
      'const firstRow = document.querySelector("[role=listbox] [role=option]");' +
      'document.documentElement.setAttribute("data-smoke-row-square", !!firstRow && getComputedStyle(firstRow).borderTopRightRadius === "0px");' +
      'document.documentElement.setAttribute("data-smoke-icon-count", document.querySelectorAll("img[src^=\\"data:image/\\"]").length);' +
      'document.documentElement.setAttribute("data-smoke-square", roots.length === 1 && getComputedStyle(roots[0]).borderTopLeftRadius === "0px" && getComputedStyle(roots[0]).borderTopWidth === "0px");' +
      'document.documentElement.setAttribute("data-smoke-system-charts", document.querySelectorAll("[aria-label=\\\"Live system telemetry\\\"] svg").length);' +
      'document.documentElement.setAttribute("data-smoke-system-pulse", !!roots[0] && !!roots[0].querySelector("[aria-label=\\\"CPU utilization 42 percent\\\"]") && roots[0].textContent.includes("LIVE · 100MS"));' +
      'document.documentElement.setAttribute("data-smoke-system-swap", !!roots[0] && roots[0].textContent.includes("0.3%"));' +
      'document.documentElement.setAttribute("data-smoke-system-unknown", !!roots[0] && roots[0].textContent.includes("TEMP N/A"));' +
      'document.documentElement.setAttribute("data-smoke-system-no-scrollbar", !!roots[0] && getComputedStyle(roots[0]).scrollbarWidth === "none");' +
      'document.documentElement.setAttribute("data-smoke-unselectable", roots.length === 1 && getComputedStyle(roots[0]).userSelect === "none");' +
      'const editable = document.querySelector("input[aria-label=\\"Search applications\\"]");' +
      'document.documentElement.setAttribute("data-smoke-editable-selectable", !editable || getComputedStyle(editable).userSelect === "text");' +
      'const painted = document.querySelector("[data-kira-icon-slot]:has(img)");' +
      'document.documentElement.setAttribute("data-smoke-real-icon-transparent", !!painted && getComputedStyle(painted).backgroundColor === "rgba(0, 0, 0, 0)");' +
      'const placeholder = document.querySelector("[data-kira-icon-slot]:has([data-kira-icon-placeholder])");' +
      'document.documentElement.setAttribute("data-smoke-placeholder-background", !!placeholder && getComputedStyle(placeholder).backgroundColor !== "rgba(0, 0, 0, 0)");' +
    '}, 1150);' +
    '});</script>';
}

async function smoke(name, steps, expected, heightMode = 'any') {
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
  const size = Number(output.match(/data-smoke-height="(\d+)"/)?.[1] ?? 0);
  const nativeMessage = output.match(/data-smoke-size="size:(\d+):(\d+)"/);
  const resizeCount = Number(output.match(/data-smoke-resizes="(\d+)"/)?.[1] ?? 0);
  if (!nativeMessage || size <= 0 || size > 420 || Number(nativeMessage[2]) <= 0 ||
      Number(nativeMessage[2]) > 420 || resizeCount > 12) {
    throw new Error(name + ': popup size bridge/cap invalid (height=' +
      size + ', native=' + nativeMessage?.[2] + ', changes=' + resizeCount + ')');
  }
  if (!output.includes('data-smoke-unselectable="true"') ||
      !output.includes('data-smoke-editable-selectable="true"')) {
    throw new Error(name + ': widget text must be unselectable while search inputs remain editable/selectable');
  }
  if (heightMode === 'system') {
    const charts = Number(output.match(/data-smoke-system-charts="(\d+)"/)?.[1] ?? 0);
    if (charts < 2 || !output.includes('data-smoke-system-swap="true"') ||
        !output.includes('data-smoke-system-unknown="true"') ||
        !output.includes('data-smoke-system-no-scrollbar="true"') ||
        !output.includes('data-smoke-system-pulse="true"')) {
      const diagnostic = output.match(/data-smoke-system-[a-z-]+="[^"]*"/g) ?? [];
      const svgs = (output.match(/<svg\b/g) ?? []).length;
      const diagnostics = output.match(/data-smoke-result="[^"]*"/g) ?? [];
      throw new Error(name + ': ECharts SVG=' + charts + ', DOM SVG=' + svgs +
        ', flags=' + diagnostic.join(' / ') + ', roots=' + diagnostics.join(','));
    }
  }
  if (heightMode === 'scroll') {
    if (size !== 420 || !output.includes('data-smoke-list-scroll="true"') ||
        !output.includes('data-smoke-scrollbar-hidden="true"') ||
        !output.includes('data-smoke-scrolled="true"') ||
        !output.includes('data-smoke-row-square="true"')) {
      throw new Error(name + ': long launcher list should fill the cap and scroll internally');
    }
  } else if (heightMode === 'shrink' && size >= 420) {
    throw new Error(name + ': filtered launcher should shrink below the cap');
  }
  if (heightMode === 'icons' && (!output.includes('data-smoke-square="true"') ||
      !output.includes('data-smoke-real-icon-transparent="true"') ||
      !output.includes('data-smoke-placeholder-background="true"') ||
      !output.includes('data-smoke-icon-width="32"') ||
      Number(output.match(/data-smoke-icon-count="(\d+)"/)?.[1] ?? 0) < 1)) {
    const square = output.match(/data-smoke-square="([^"]+)"/)?.[1] ?? 'unset';
    const icons = output.match(/data-smoke-icon-count="([^"]+)"/)?.[1] ?? 'unset';
    const rows = (output.match(/role="option"/g) ?? []).length;
    throw new Error(name + ': square=' + square + ', icons=' + icons + ', rows=' + rows);
  }
  console.log('OK: ' + name + ' mounted ' + expected + ' at ' + size + 'px');
}

try {
  await smoke('calendar', [{ delay: 25, kind: 'calendar', generation: 1 }], 'Kira Calendar');
  await smoke('system', [{ delay: 25, kind: 'system', generation: 1 }], 'Kira System Monitor');
  await smoke('system-live-telemetry', [
    { delay: 25, kind: 'system', generation: 1 },
    { delay: 170, kind: 'metrics' },
    { delay: 280, kind: 'pulse' },
  ], 'Kira System Monitor', 'system');
  await smoke('launcher', [{ delay: 25, kind: 'launcher', generation: 1 }], 'Kira Launcher');
  await smoke('launcher-square-with-themed-icons', [
    { delay: 25, kind: 'launcher', generation: 1 },
    { delay: 90, kind: 'catalog' },
  ], 'Kira Launcher', 'icons');
  await smoke('launcher-long-list', [
    { delay: 25, kind: 'launcher', generation: 1 },
    { delay: 90, kind: 'catalog' },
  ], 'Kira Launcher', 'scroll');
  await smoke('launcher-filter-shrinks-window', [
    { delay: 25, kind: 'launcher', generation: 1 },
    { delay: 90, kind: 'catalog' },
    { delay: 230, kind: 'filter' },
  ], 'Kira Launcher', 'shrink');
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
  await smoke('three-widget-cycle', [
    { delay: 25, kind: 'calendar', generation: 1 },
    { delay: 130, kind: 'system', generation: 2 },
    { delay: 240, kind: 'launcher', generation: 3 },
    { delay: 255, kind: 'hide', generation: 1 },
    { delay: 350, kind: 'calendar', generation: 4 },
    { delay: 365, kind: 'hide', generation: 2 },
    { delay: 480, kind: 'launcher', generation: 5 },
    { delay: 495, kind: 'hide', generation: 4 },
  ], 'Kira Launcher');
} finally {
  await rm(dir, { recursive: true, force: true });
}
