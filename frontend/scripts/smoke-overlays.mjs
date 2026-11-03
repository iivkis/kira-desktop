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

function scriptFor(steps, observationMs = 1150) {
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
      'if (String(message).startsWith("closed:")) {' +
        'document.documentElement.dataset.smokeClosed=String(message);' +
        'const root=document.querySelector("main[data-kira-ui-root]");' +
        'document.documentElement.dataset.smokeReverseCompleted=!!root && root.dataset.tenetReveal==="reversed" && ' +
          'root.dataset.tenetSurface==="retracted" && root.style.clipPath.includes("100%") && ' +
          'root.querySelectorAll("[data-tenet-fragment=inverted]").length===4;' +
      '}' +
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
        (kind === 'tenet-close-reopen'
          ? 'window.dispatchEvent(new CustomEvent("kira:overlay:close", {detail:{generation:' + generation + '}}));' +
            'const root=document.querySelector("main[data-kira-ui-root]");' +
            'document.documentElement.dataset.smokeReverseInFlight=!!root && root.dataset.tenetReveal==="reversing";' +
            'window.dispatchEvent(new CustomEvent("kira:overlay:reopen", {detail:{generation:' + generation + '}}));'
          : kind === 'tenet-close'
          ? 'window.dispatchEvent(new CustomEvent("kira:overlay:close", {detail:{generation:' + generation + '}}));' +
            'const root=document.querySelector("main[data-kira-ui-root]");' +
            'document.documentElement.dataset.smokeReverseInFlight=!!root && root.dataset.tenetReveal==="reversing" && ' +
              'root.querySelectorAll("[data-tenet-fragment=inverted]").length===4;'
          : kind === 'tenet-reopen'
            ? 'window.dispatchEvent(new CustomEvent("kira:overlay:reopen", {detail:{generation:' + generation + '}}));'
          : kind === 'hide'
          ? 'window.dispatchEvent(new CustomEvent("kira:overlay:hide", {detail: {generation: ' +
            generation + '}}));'
          : kind === 'catalog'
            ? 'window.dispatchEvent(new CustomEvent("kira:launcher:apps", {detail: {apps: Array.from({length: 80}, (_, id) => ({id, name: "Application " + id, description: "Test entry"}))}}));'
            : kind === 'filter'
              ? 'const input = document.querySelector("input[aria-label=\\"Search applications\\"]");' +
                'if (input) { input.value = "Application 79"; input.dispatchEvent(new Event("input", {bubbles: true})); }'
              : kind === 'launcher-down'
                ? 'window.dispatchEvent(new KeyboardEvent("keydown", {key:"ArrowDown", bubbles:true}));'
              : kind === 'system-processes'
                ? 'document.querySelector("nav [data-kira-tab=processes]")?.click();'
              : kind === 'system-overview'
                ? 'document.querySelector("nav [data-kira-tab=overview]")?.click();'
              : kind === 'calendar-next'
                ? 'window.__kiraCalendarBefore = document.querySelector("[aria-label=\\\"Date picker\\\"] h2")?.textContent;' +
                  'document.querySelector("button[aria-label=\\\"Next month\\\"]")?.click();'
              : kind === 'calendar-pick'
                ? 'const candidate = document.querySelector("[aria-label=\\\"Calendar dates\\\"] button[data-calendar-day].text-kira-text:not([aria-pressed=\\\"true\\\"])");' +
                  'window.__kiraCalendarChosen = candidate?.getAttribute("aria-label");candidate?.click();'
              : kind === 'clock-chaos'
                ? // Clock flights can finish between Chromium virtual-time
                  // samples. Observe the real animated style rather than
                  // requiring a particular 180ms frame to be scheduled.
                  'const clock=document.querySelector("[data-calendar-clock-play]");' +
                  'for(const cell of clock?.querySelectorAll("[data-clock-cell]")||[]){' +
                    'new MutationObserver(()=>{' +
                      'if(cell.style.transform && cell.style.transform!=="none")window.__kiraClockFlew=true;' +
                    '}).observe(cell,{attributes:true,attributeFilter:["style"]});' +
                  '}' +
                  'clock?.click();' +
                  'setTimeout(() => {' +
                    'window.__kiraClockFlew = !!window.__kiraClockFlew || Array.from(document.querySelectorAll("[data-clock-cell]")).some(cell => getComputedStyle(cell).transform !== "none");' +
                  '}, 180);'
              : kind === 'settings-navigation'
                ? 'const side=document.querySelector("aside");' +
                  'if(side && getComputedStyle(side).display!=="none"){' +
                    'side.querySelector("button[data-kira-tab=displays]")?.click();' +
                  '}else{const select=document.getElementById("kira-settings-section");' +
                    'if(select){select.value="displays";select.dispatchEvent(new Event("change",{bubbles:true}));}}'
              : kind === 'settings-about'
                ? 'window.dispatchEvent(new CustomEvent("kira:settings:about", {detail: {generation: 1, about: {' +
                  'os_name:"Example Linux", kernel:"6.1.0", architecture:"x86_64", hostname:"example-host", ' +
                  'cpu_model:"Example CPU", cpu_threads:8, memory_total_kib:8388608, ' +
                  'session:"wayland", shell_version:"0.1.0"}}}));'
              : kind === 'settings-displays'
                ? 'const select=document.querySelector("select[aria-label=\\\"Раздел настроек\\\"]");' +
                  'if(select){select.value="displays";select.dispatchEvent(new Event("change",{bubbles:true}));}' +
                  'else {document.querySelector("aside button:nth-child(3)")?.click();}'
              : kind === 'settings-displays-data'
                ? 'window.dispatchEvent(new CustomEvent("kira:settings:displays",{detail:{' +
                  'generation:1,state:"ready",persistence:"manual",error:null,pending_id:null,outputs:[' +
                  '{connector:"eDP-1",name:"eDP-1",make:"Example",model:"Laptop",current_mode:0,' +
                  'modes:[{width:1920,height:1080,refresh_rate:60000,is_preferred:true}],vrr_supported:false,vrr_enabled:false,' +
                  'logical:{x:0,y:0,width:1920,height:1080,scale:1,transform:"Normal"}},' +
                  '{connector:"HDMI-A-1",name:"HDMI-A-1",make:"Example",model:"External",current_mode:0,' +
                  'modes:[{width:2560,height:1440,refresh_rate:144000,is_preferred:true}],vrr_supported:true,vrr_enabled:false,' +
                  'logical:{x:1920,y:0,width:2560,height:1440,scale:1,transform:"Normal"}}' +
                  ']}}));'
              : kind === 'settings-detach'
                ? 'document.documentElement.dataset.kiraDetached="true";' +
                  'window.dispatchEvent(new CustomEvent("kira:overlay:detached", {detail:{generation:1}}));'
              : kind === 'metrics'
                ? 'window.dispatchEvent(new CustomEvent("kira:system:data", {detail: ' + telemetry + '}));'
              : kind === 'pulse'
                ? 'window.dispatchEvent(new CustomEvent("kira:system:pulse", {detail: {cpu_percent: 42, memory_total_kib: 1000, memory_available_kib: 500}}));'
              : 'window.dispatchEvent(new CustomEvent("kira:overlay:show", {detail: {kind: "' +
                kind + '", generation: ' + generation + ', maxHeight: 420}}));' +
                // Real GTK mounts content behind opacity zero and sends
                // the generation-scoped visible event only AFTER reveal.
                'setTimeout(()=>{' +
                  'const root=document.querySelector("main[data-kira-ui-root]");' +
                  'const first=root?.querySelector("[data-kira-enter]");' +
                  'const staged=first ? Number(getComputedStyle(first).opacity) : 0;' +
                  'window.__kiraTenetStagedOpacity=staged;' +
                  // At 1.44x the content opacity tween can complete between
                  // coarse virtual-time samples. Observe actual GSAP writes.
                  'if(first){new MutationObserver(()=>{' +
                    'const opacity=Number(getComputedStyle(first).opacity);' +
                    'const transform=getComputedStyle(first).transform;' +
                    'if(transform!=="none" && opacity>staged && opacity<1)' +
                      'document.documentElement.dataset.smokeTenetContentMoving="true";' +
                  '}).observe(first,{attributes:true,attributeFilter:["style"]});}' +
                  'document.documentElement.dataset.smokeSurfaceStaged=!!root && ' +
                    '(matchMedia("(prefers-reduced-motion: reduce)").matches ? root.style.clipPath==="" : ' +
                    '(root.dataset.tenetSurface==="staged" && root.style.clipPath.includes("100%") && ' +
                     'root.querySelectorAll("[data-kira-surface-seam]").length===1));' +
                  // In a faster (450ms) scene Chrome's virtual clock can skip
                  // all coarse timeout snapshots. Observe real inline
                  // clip-path changes as the compositor makes them instead.
                  'if(root){new MutationObserver(()=>{' +
                    'const clip=root.style.clipPath||"";' +
                    'const materializing=root.dataset.tenetSurface==="materializing" && ' +
                      'clip.includes("inset(") && !clip.includes("100%") && clip!=="inset(0%)" && ' +
                      '!clip.includes("0% 0% 0% 0%");' +
                    'if(materializing)document.documentElement.dataset.smokeSurfaceMoving="true";' +
                  '}).observe(root,{attributes:true,attributeFilter:["style"]});}' +
                  'if(root)document.documentElement.dataset.smokeTenetPrevisible=' +
                    'root.dataset.tenetReveal==="ready" && root.querySelectorAll("[data-tenet-fragment]").length===0 && staged>=0.92 && staged<0.97;' +
                '},90);' +
                'setTimeout(()=>window.dispatchEvent(new CustomEvent("kira:overlay:visible", {detail:{generation:' +
                  generation + '}})),140);') +
        '}, ' + delay + ');'
    ).join('') +
    // Sample the in-flight DUAL chronology before observing settled DOM.
    'setTimeout(() => {' +
      'const root=document.querySelector("main[data-kira-ui-root]");' +
      'document.documentElement.dataset.smokeTenetStarted=!!root && root.dataset.tenetReveal==="started" && root.querySelectorAll("[data-tenet-fragment=inverted]").length===4;' +
      '},245);' +
    // The first WebKit frame can be delayed by ECharts SVG painting.
    // Verify *actual* opacity progression mid-flight, after the renderer
    // has had time to composite at least one frame.
    // Sample multiple compositor frames. WebKit/Chromium can delay a
    // single frame under the launcher catalog and ECharts SVG load.
    'for(const at of [210,250,290,330])setTimeout(() => {' +
      'const content=document.querySelector("main[data-kira-ui-root] [data-kira-enter]");' +
      'const transform=content ? getComputedStyle(content).transform : "none";' +
      'const opacity=content ? Number(getComputedStyle(content).opacity) : 0;' +
      'const moving=!!content && transform!=="none" && opacity>=window.__kiraTenetStagedOpacity && opacity<1;' +
      'document.documentElement.dataset.smokeTenetContentMoving=' +
      '(document.documentElement.dataset.smokeTenetContentMoving==="true" || moving);' +
      'const surface=document.querySelector("main[data-kira-ui-root]");' +
      'const clip=surface?.style.clipPath || "";' +
      'const clipped=surface?.dataset.tenetSurface==="materializing" && clip.includes("inset(") && ' +
        '!clip.includes("100%") && clip!=="inset(0%)" && !clip.includes("0% 0% 0% 0%");' +
      'document.documentElement.dataset.smokeSurfaceMoving=' +
        '(document.documentElement.dataset.smokeSurfaceMoving==="true" || clipped);' +
      '},at);' +
    // Test the live DOM, not strings in the bundled Svelte templates.
    'setTimeout(() => {' +
      'const roots = document.querySelectorAll("main[data-kira-ui-root]");' +
      'const label = roots.length === 1 ? roots[0].getAttribute("aria-label") : "invalid-root-count";' +
      'document.documentElement.setAttribute("data-smoke-result", label || "none");' +
      'document.documentElement.dataset.smokeSurfaceClip=roots[0]?.style.clipPath || "none";' +
      'document.documentElement.dataset.smokeSurfaceState=roots[0]?.dataset.tenetSurface || "missing";' +
      'document.documentElement.dataset.smokeSurfaceSettled=roots.length===1 && ' +
        '(matchMedia("(prefers-reduced-motion: reduce)").matches ? roots[0].style.clipPath==="" : ' +
        '(roots[0].dataset.tenetSurface==="settled" && ' +
         'roots[0].style.clipPath.startsWith("inset(0%") && !roots[0].style.clipPath.includes("100%")));' +
      'document.documentElement.setAttribute("data-smoke-tenet-settled", roots.length === 1 && roots[0].dataset.tenetReveal === "settled" && ' +
        'roots[0].querySelectorAll("[data-tenet-fragment=inverted]").length === (matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : 4) && ' +
        'Array.from(roots[0].querySelectorAll("[data-tenet-fragment]")).every(el => Number(getComputedStyle(el).opacity) < 0.02));' +
      'const height = roots.length === 1 ? roots[0].getBoundingClientRect().height : 0;' +
      'document.documentElement.setAttribute("data-smoke-height", Math.ceil(height));' +
      // Three layered shadows: hard black, dark orange, and a feathered
      // black ambient silhouette. All four sides must have WebKit gutter.
      'const outer=roots[0]?.parentElement;const outerStyle=outer?getComputedStyle(outer):null;' +
      'const edge=roots[0]?getComputedStyle(roots[0],"::after"):null;' +
      'const shadows=outerStyle?.filter.match(/drop-shadow\\(/g) || [];' +
      'document.documentElement.dataset.smokeWidgetShadow=!!outerStyle && ' +
        'shadows.length===3 && outerStyle.paddingTop==="8px" && outerStyle.paddingBottom==="8px" && ' +
        'outerStyle.paddingLeft==="8px" && outerStyle.paddingRight==="8px" && ' +
        '!!edge && edge.content==="none";' +
      'document.documentElement.dataset.smokeDetachedShadow=!!outerStyle && outerStyle.filter==="none" && ' +
        'outerStyle.paddingTop==="0px" && outerStyle.paddingBottom==="0px" && ' +
        'outerStyle.paddingLeft==="0px" && outerStyle.paddingRight==="0px" && ' +
        '!!edge && edge.content==="none";' +
      'document.documentElement.setAttribute("data-smoke-overflow", roots.length === 1 && roots[0].scrollHeight > roots[0].clientHeight);' +
      'const list = document.querySelector("[role=listbox]");' +
      'document.documentElement.setAttribute("data-smoke-list-scroll", !!list && list.scrollHeight > list.clientHeight);' +
      'document.documentElement.setAttribute("data-smoke-option-count", list?.querySelectorAll("[role=option]").length ?? 0);' +
      'const shared=roots[0]?.querySelector("[aria-label=\\\"Calendar dates\\\"] [data-kira-flow-indicator]") ||' +
        'roots[0]?.querySelector("[role=listbox] [data-kira-flow-indicator]") ||' +
        'roots[0]?.querySelector("nav [data-kira-flow-indicator]");' +
      'const area=shared?.parentElement;' +
      'const chosen=area?.querySelector("[data-calendar-selected=true], [role=option][aria-selected=true], [aria-current=page]");' +
      'const sr=shared?.getBoundingClientRect(), cr=chosen?.getBoundingClientRect();' +
      'document.documentElement.dataset.smokeFlowAligned=!!sr && !!cr &&' +
        'Math.abs(sr.left-cr.left)<2 && Math.abs(sr.width-cr.width)<2 &&' +
        '(area?.tagName==="NAV"?Math.abs(sr.bottom-cr.bottom):Math.abs(sr.top-cr.top))<2;' +
      'document.documentElement.dataset.smokeFlowGeometry=sr&&cr?' +
        '[sr.left,sr.top,sr.width,sr.height,cr.left,cr.top,cr.width,cr.height].map(n=>Math.round(n)).join(","):"missing";' +
      'document.documentElement.setAttribute("data-smoke-query-value", document.querySelector("input[aria-label=\\\"Search applications\\\"]")?.value ?? "absent");' +
      'document.documentElement.setAttribute("data-smoke-scrollbar-hidden", !!list && getComputedStyle(list).scrollbarWidth === "none");' +
      'if (list && list.scrollHeight > list.clientHeight) list.scrollTop = 80;' +
      'document.documentElement.setAttribute("data-smoke-scrolled", !!list && list.scrollTop > 0);' +
      'const firstIcon = document.querySelector("[role=listbox] img[src^=\\"data:image/\\"]");' +
      'document.documentElement.setAttribute("data-smoke-icon-width", firstIcon ? Math.round(firstIcon.getBoundingClientRect().width) : 0);' +
      'const firstRow = document.querySelector("[role=listbox] [role=option]");' +
      'document.documentElement.setAttribute("data-smoke-row-square", !!firstRow && getComputedStyle(firstRow).borderTopRightRadius === "0px");' +
      'const combo = document.querySelector("input[role=combobox][aria-controls=kira-launcher-results]");' +
      'const selectedOption = document.querySelector("[role=listbox] [role=option][aria-selected=true]");' +
      'document.documentElement.setAttribute("data-smoke-launcher-a11y", !!combo && !!selectedOption && combo.getAttribute("aria-activedescendant") === selectedOption.id && selectedOption.tabIndex === -1);' +
      'document.documentElement.setAttribute("data-smoke-icon-count", document.querySelectorAll("img[src^=\\"data:image/\\"]").length);' +
      'document.documentElement.setAttribute("data-smoke-square", roots.length === 1 && getComputedStyle(roots[0]).borderTopLeftRadius === "0px" && getComputedStyle(roots[0]).borderTopWidth === "0px");' +
      'const clock = document.querySelector("[data-calendar-clock]");' +
      'document.documentElement.setAttribute("data-smoke-calendar-odometer", !!clock && clock.querySelectorAll("[data-clock-digit]").length === 6 && !!clock.getAttribute("aria-label") && !!clock.getAttribute("datetime") && clock.querySelectorAll("[data-clock-cell]").length === 6);' +
      'const calGrid = document.querySelector("[aria-label=\\\"Calendar dates\\\"]");' +
      'document.documentElement.setAttribute("data-smoke-tenet-grid-events", calGrid?.dataset.tenetScenes || "0");' +
      'document.documentElement.setAttribute("data-smoke-tenet-grid-contact", !!calGrid?.dataset.tenetContact);' +
      // Four recorded fragments remain DOM-owned but invisible until exit.
      // Other calendar/clock interaction fragments must still be cleaned up.
      'document.documentElement.setAttribute("data-smoke-tenet-no-fragments", ' +
        'Array.from(document.querySelectorAll("[data-tenet-fragment]")).every(el => Number(getComputedStyle(el).opacity) < 0.02));' +
      'document.documentElement.setAttribute("data-smoke-tenet-reveal", document.querySelector("main[data-kira-ui-root]")?.dataset.tenetReveal || "missing");' +
      'document.documentElement.setAttribute("data-smoke-tenet-clock-events", document.querySelector("[data-calendar-clock-play]")?.dataset.tenetEvents || "0");' +
      'document.documentElement.setAttribute("data-smoke-tenet-clock-settled", document.querySelector("[data-calendar-clock-play]")?.dataset.tenetPlayback === "settled");' +
      'document.documentElement.setAttribute("data-smoke-clock-chaos-flew", !!window.__kiraClockFlew);' +
      'document.documentElement.setAttribute("data-smoke-clock-chaos-reset", !!clock && Array.from(clock.querySelectorAll("[data-clock-cell]")).every(cell => getComputedStyle(cell).transform === "none"));' +
      'document.documentElement.setAttribute("data-smoke-clock-chaos-transform", !!clock && Array.from(clock.querySelectorAll("[data-clock-cell]")).slice(0,3).map(cell => getComputedStyle(cell).transform).join("|"));' +
      'document.documentElement.setAttribute("data-smoke-calendar-navigated", !!window.__kiraCalendarBefore && window.__kiraCalendarBefore !== document.querySelector("[aria-label=\\\"Date picker\\\"] h2")?.textContent);' +
      'document.documentElement.setAttribute("data-smoke-calendar-picked", !!window.__kiraCalendarChosen && window.__kiraCalendarChosen === document.querySelector("[aria-label=\\\"Calendar dates\\\"] button[aria-pressed=\\\"true\\\"]")?.getAttribute("aria-label"));' +
      'document.documentElement.setAttribute("data-smoke-system-charts", document.querySelectorAll("[aria-label=\\\"Live system telemetry\\\"] svg").length);' +
      'document.documentElement.setAttribute("data-smoke-system-pulse", !!roots[0] && !!roots[0].querySelector("[aria-label=\\\"CPU utilization 42 percent\\\"]") && !roots[0].textContent.includes("PULSE STALE"));' +
      'document.documentElement.setAttribute("data-smoke-system-swap", !!roots[0] && roots[0].textContent.includes("0.3%"));' +
      'document.documentElement.setAttribute("data-smoke-system-unknown", !!roots[0] && roots[0].textContent.includes("TEMP N/A"));' +
      'document.documentElement.setAttribute("data-smoke-system-no-scrollbar", !!roots[0] && getComputedStyle(roots[0]).scrollbarWidth === "none");' +
      'const settings=document.querySelector("main[aria-label=\\\"Kira Settings\\\"]");' +
      'document.documentElement.dataset.smokeSettingsMode=settings?.dataset.settingsMode || "none";' +
      'document.documentElement.dataset.smokeSettingsData=!!settings && settings.textContent.includes("Example Linux") && settings.textContent.includes("Example CPU");' +
      'const settingsSelect=settings?.querySelector("select[aria-label=\\\"Раздел настроек\\\"]");' +
      'document.documentElement.dataset.smokeSettingsSelector=!!settingsSelect&&' +
        'getComputedStyle(settingsSelect.parentElement.parentElement).display!=="none";' +
      'document.documentElement.dataset.smokeSettingsSidebar=!!settings?.querySelector("aside[aria-label=\\\"Разделы настроек\\\"]");' +
      'document.documentElement.dataset.smokeDisplays=!!settings?.querySelector("section[aria-label=\\\"Настройки дисплеев\\\"]") && settings.textContent.includes("HDMI-A-1") && settings.textContent.includes("Example External");' +
      'document.documentElement.dataset.smokeDisplayBoxes=settings?.querySelectorAll("button[aria-label^=\\\"Переместить или выбрать\\\"]").length || 0;' +
      'document.documentElement.setAttribute("data-smoke-no-hover-warp", !document.querySelector("filter[id^=kira-temporal-warp], [data-tenet-warp-state], [data-tenet-warp-pixels], [data-tenet-lens-body]"));' +
      'document.documentElement.setAttribute("data-smoke-unselectable", roots.length === 1 && getComputedStyle(roots[0]).userSelect === "none");' +
      'const editable = document.querySelector("input[aria-label=\\"Search applications\\"]");' +
      'document.documentElement.setAttribute("data-smoke-editable-selectable", !editable || getComputedStyle(editable).userSelect === "text");' +
      'const painted = document.querySelector("[data-kira-icon-slot]:has(img)");' +
      'document.documentElement.setAttribute("data-smoke-real-icon-transparent", !!painted && getComputedStyle(painted).backgroundColor === "rgba(0, 0, 0, 0)");' +
      'const placeholder = document.querySelector("[data-kira-icon-slot]:has([data-kira-icon-placeholder])");' +
      'document.documentElement.setAttribute("data-smoke-placeholder-background", !!placeholder && getComputedStyle(placeholder).backgroundColor !== "rgba(0, 0, 0, 0)");' +
    '}, ' + observationMs + ');' +
    '});</script>';
}

async function smoke(name, steps, expected, heightMode = 'any') {
  const document = html
    .replace('<html lang="en">', '<html lang="en" data-kira-surface="calendar">')
    .replace('</head>', scriptFor(steps, heightMode === 'clock' ? 1900 : heightMode === 'calendar' ? 2050 : heightMode === 'tenet' ? 2050 : heightMode.startsWith('reverse') ? 1750 : 1150) + '</head>');
  if (!document.includes('data-kira-surface="calendar"')) {
    throw new Error('Overlay marker was not inserted in the production HTML');
  }
  const path = join(dir, name + '.html');
  await writeFile(path, document);
  const output = execFileSync(chrome, [
    '--headless=new', '--no-sandbox', '--disable-gpu',
    '--disable-dev-shm-usage', '--disable-background-networking',
    ...(heightMode === 'reduced' ? ['--force-prefers-reduced-motion=reduce'] : []),
    '--virtual-time-budget=' + (heightMode === 'clock' ? 2600 : heightMode === 'calendar' ? 2800 : heightMode === 'tenet' || heightMode.startsWith('reverse') ? 2800 : 1800),
    '--dump-dom', 'file://' + path,
  ], { encoding: 'utf8', timeout: 15000, stdio: ['ignore', 'pipe', 'ignore'] });
  if (!output.includes('data-smoke-result="' + expected + '"')) {
    const found = output.match(/data-smoke-result="([^"]+)"/)?.[1] ?? 'no result';
    throw new Error(name + ': expected live Svelte ' + expected + ', found ' + found);
  }
  const size = Number(output.match(/data-smoke-height="(\d+)"/)?.[1] ?? 0);
  const nativeMessage = output.match(/data-smoke-size="size:(\d+):(\d+)"/);
  const resizeCount = Number(output.match(/data-smoke-resizes="(\d+)"/)?.[1] ?? 0);
  if (!nativeMessage || size <= 0 || (heightMode !== 'settings-window' && size > 420) ||
      Number(nativeMessage[2]) <= 0 ||
      (heightMode !== 'settings-window' && Number(nativeMessage[2]) > 420) || resizeCount > 12) {
    throw new Error(name + ': popup size bridge/cap invalid (height=' +
      size + ', native=' + nativeMessage?.[2] + ', changes=' + resizeCount + ')');
  }
  if ((heightMode === 'settings-widget' || heightMode === 'settings-window') &&
      (!output.includes('data-smoke-settings-data="true"') ||
       !output.includes('data-smoke-settings-mode="' + (heightMode === 'settings-window' ? 'window' : 'widget') + '"') ||
       !output.includes('data-smoke-settings-selector="' + (heightMode === 'settings-window' ? 'false' : 'true') + '"') ||
       !output.includes('data-smoke-settings-sidebar="' + (heightMode === 'settings-window' ? 'true' : 'false') + '"'))) {
    throw new Error(name + ': Settings mode, navigation or native data failed: ' +
      (output.match(/data-smoke-settings-[a-z-]+="[^"]*"/g) ?? []).join(', '));
  }
  if (heightMode === 'settings-displays' &&
      (!output.includes('data-smoke-displays="true"') ||
       !output.includes('data-smoke-display-boxes="2"'))) {
    throw new Error(name + ': Niri display topology and controls did not mount (' +
      (output.match(/data-smoke-display[a-z-]*="[^"]*"/g) ?? []).join(', ') + ')');
  }
  if (!output.includes('data-smoke-no-hover-warp="true"')) {
    throw new Error(name + ': removed pixel-distortion must not mount in any widget');
  }
  if (!output.includes('data-smoke-unselectable="true"') ||
      !output.includes('data-smoke-editable-selectable="true"')) {
    throw new Error(name + ': widget text must be unselectable while search inputs remain editable/selectable');
  }
  if (heightMode === 'calendar' &&
      (!output.includes('data-smoke-flow-aligned="true"') ||
        !output.includes('data-smoke-calendar-navigated="true"') ||
       !output.includes('data-smoke-calendar-picked="true"') ||
       !output.includes('data-smoke-calendar-odometer="true"') ||
       !output.includes('data-smoke-tenet-grid-events="2"') ||
       !output.includes('data-smoke-tenet-grid-contact="true"') ||
       !output.includes('data-smoke-tenet-no-fragments="true"'))) {
    const flags = output.match(/data-smoke-(?:calendar|tenet|flow)-[a-z-]+="[^"]*"/g) ?? [];
    throw new Error(name + ': GSAP calendar navigation/selection: ' + flags.join(', '));
  }
  if (heightMode === 'flow' && !output.includes('data-smoke-flow-aligned="true"')) {
    throw new Error(name + ': shared highlight missed selected geometry (' +
      (output.match(/data-smoke-flow-geometry="([^"]*)"/)?.[1] ?? 'missing') + ')');
  }
  if (heightMode === 'clock' &&
      (!output.includes('data-smoke-clock-chaos-flew="true"') ||
       !output.includes('data-smoke-clock-chaos-reset="true"') ||
       !output.includes('data-smoke-calendar-odometer="true"') ||
       !output.includes('data-smoke-tenet-clock-events="2"') ||
       !output.includes('data-smoke-tenet-clock-settled="true"') ||
       !output.includes('data-smoke-tenet-no-fragments="true"'))) {
    const flags = output.match(/data-smoke-(?:clock-chaos|tenet)-[a-z-]+="[^"]*"/g) ?? [];
    throw new Error(name + ': GSAP clock digits must fly and return: ' + flags.join(', '));
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
  if (heightMode !== 'settings-window' && !output.includes('data-smoke-widget-shadow="true"')) {
    throw new Error(name + ': popup must show black/dark-orange depth with soft all-side shadow');
  }
  if (heightMode === 'settings-window' && !output.includes('data-smoke-detached-shadow="true"')) {
    throw new Error(name + ': detached Niri window must not retain the popup shadow gutter');
  }
  if (heightMode === 'scroll') {
    if (size !== 404 || Number(nativeMessage[2]) !== 420 || !output.includes('data-smoke-list-scroll="true"') ||
        !output.includes('data-smoke-scrollbar-hidden="true"') ||
        !output.includes('data-smoke-scrolled="true"') ||
        !output.includes('data-smoke-row-square="true"')) {
      throw new Error(name + ': long launcher list should fill the cap and scroll internally');
    }
  } else if (heightMode === 'shrink' && size >= 420) {
    const n = output.match(/data-smoke-option-count="([^"]*)"/)?.[1];
    const q = output.match(/data-smoke-query-value="([^"]*)"/)?.[1];
    throw new Error(name + ': filtered launcher should shrink below the cap (height=' + size +
      ', rows=' + n + ', query=' + q + ', native=' + nativeMessage?.[2] + ', resizes=' + resizeCount + ')');
  }
  if (heightMode === 'tenet' &&
      (!output.includes('data-smoke-surface-staged="true"') ||
       // The 1.44x surface transition can finish between virtual Chromium
       // frames. Verify its real staged and fully revealed geometries here;
       // GSAP's exact intermediate clip tween is asserted by motion tests.
       !output.includes('data-smoke-surface-settled="true"') ||
       !output.includes('data-smoke-tenet-previsible="true"') ||
       !output.includes('data-smoke-tenet-started="true"') ||
       (!output.includes('data-smoke-surface-moving="true"') &&
        !output.includes('data-smoke-tenet-content-moving="true"')) ||
       !output.includes('data-smoke-tenet-settled="true"'))) {
    const stage = output.match(/data-smoke-(?:tenet|surface)-[a-z-]+="[^"]*"/g) ?? [];
    throw new Error(name + ': native handshake, real content motion or cleanup failed: ' + stage.join(', '));
  }
  if (heightMode === 'reverse' &&
      (!(output.includes('data-smoke-reverse-in-flight="true"') ||
          output.includes('data-smoke-reverse-completed="true"')) ||
       !output.includes('data-smoke-closed="closed:1"') ||
       !output.includes('data-smoke-reverse-completed="true"'))) {
    const stages = output.match(/data-smoke-(?:reverse|closed|tenet-reveal|tenet-settled)[a-z-]*="[^"]*"/g) ?? [];
    throw new Error(name + ': reverse must finish on the same retained timeline before host ACK: ' + stages.join(', '));
  }
  // A throttled page may have no active GSAP timeline and immediately
  // post 'closed' before a same-task reopen event. Native ignores that
  // stale ACK after cancel_close() clears the closing lease flag. What
  // matters for the browser is that the same Svelte root settles visibly.
  if (heightMode === 'reverse-reopen' &&
      (!output.includes('data-smoke-tenet-settled="true"') ||
       !output.includes('data-smoke-surface-settled="true"'))) {
    const stages = output.match(/data-smoke-(?:reverse|closed|tenet-reveal|tenet-settled)[a-z-]*="[^"]*"/g) ?? [];
    throw new Error(name + ': rapid reopen must reverse the in-flight exit without releasing the lease: ' + stages.join(', '));
  }
  if (heightMode === 'reduced' &&
      (!output.includes('data-smoke-surface-staged="true"') ||
       !output.includes('data-smoke-surface-settled="true"') ||
       !output.includes('data-smoke-tenet-started="false"') ||
       !output.includes('data-smoke-tenet-settled="true"'))) {
    const stages = output.match(/data-smoke-tenet-[a-z-]+="[^"]*"/g) ?? [];
    throw new Error(name + ': reduced motion must skip animation and fragments: ' + stages.join(', '));
  }
  if (heightMode === 'icons' && (!output.includes('data-smoke-launcher-a11y="true"') || !output.includes('data-smoke-square="true"') ||
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


/** Exercise real browser pixels, not just pure array projections or CSS strings.
 * 11 Niri entries, a limit of 8, two hidden urgent entries, and a long-name
 * strip that overflows the 680px viewport. Focus must be in visible pixels.
 */
async function smokeTopBar() {
  const script = '<script>' +
    'window.webkit={messageHandlers:{kiraDiagnostics:{postMessage(){}},kiraCore:{postMessage(){}}}};' +
    'window.addEventListener("load",()=>{' +
    'setTimeout(()=>window.dispatchEvent(new CustomEvent("kira:config",{detail:{config:{' +
    'compact:false,title:"KIRA DESKTOP",show_seconds:false,show_names:true,max_workspaces:8' +
    '},error:null}})),45);' +
    'setTimeout(()=>window.dispatchEvent(new CustomEvent("kira:workspaces",{detail:{connected:true,' +
    'workspaces:Array.from({length:11},(_,i)=>({id:String(i+1),idx:i+1,' +
    'name:"Workspace long name "+(i+1),output:"DP-1",is_active:i===8,is_focused:i===8,' +
    'is_urgent:i===0||i===5||i===10}))}})),85);' +
    'setTimeout(()=>{' +
    'const root=document.querySelector("main[aria-label=\\\"Kira Desktop top bar\\\"]");' +
    'const list=root?.querySelector("ol[aria-label=\\\"Niri workspaces\\\"]");' +
    'const focused=list?.querySelector("[data-focused=true]");' +
    'const listRect=list?.getBoundingClientRect();const focusRect=focused?.getBoundingClientRect();' +
    'const shared=list?.querySelector("[data-kira-flow-indicator]");' +
    'const selectedRect=shared?.getBoundingClientRect();' +
    'const flowAligned=!!selectedRect && !!focusRect &&' +
      'Math.abs(selectedRect.left-focusRect.left)<2 &&' +
      'Math.abs(selectedRect.width-focusRect.width)<2;' +
    'const urgency=root?.querySelectorAll("[aria-label*=\\\"urgent workspace hidden\\\"]");' +
    'const visibleUrgent=root?.querySelector("[aria-label*=\\\", urgent\\\"]");' +
    'document.documentElement.dataset.smokeTopbar=!!root && root.getBoundingClientRect().height===42 &&' +
    '!!list && list.scrollWidth>list.clientWidth && list.scrollLeft>0 && !!focused &&' +
    'focused.dataset.workspaceId==="9" && focusRect.left>=listRect.left-2 &&' +
    'focusRect.right<=listRect.right+2 && flowAligned && urgency.length===2 && !!visibleUrgent;' +
    '},600);});</script>';
  const page = html.replace('</head>', script + '</head>');
  const filename = join(dir, 'topbar.html');
  await writeFile(filename, page);
  const output = execFileSync(chrome, [
    '--headless=new', '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage',
    '--disable-background-networking', '--window-size=680,440',
    '--virtual-time-budget=1200', '--dump-dom', 'file://' + filename,
  ], { encoding: 'utf8', timeout: 15000, stdio: ['ignore', 'pipe', 'ignore'] });
  if (!output.includes('data-smoke-topbar="true"')) {
    const list = output.match(/<ol[^>]*aria-label="Niri workspaces"[\s\S]{0,800}/)?.[0] ?? 'not mounted';
    throw new Error('Top Bar focused pixel visibility or urgency regression: ' + list.slice(0, 600));
  }
  console.log('OK: Top Bar focus auto-scroll and urgency on both hidden sides');
}

/** Native detachment event must build and paint the genuine Svelte canvas;
 * no mock sky pixels or remote data is used in this browser smoke. */
async function smokeCalendarSky(windowSize, expectStacked) {
  const steps = [
    { delay: 25, kind: 'calendar', generation: 1 },
    { delay: 600, kind: 'settings-detach', generation: 1 },
    { delay: 820, kind: 'calendar-next', generation: 1 },
  ];
  const check = '<script>window.addEventListener("load",()=>setTimeout(()=>{' +
    'const root=document.querySelector("main[aria-label=\\\"Kira Calendar\\\"]");' +
    'const atlas=root?.querySelector("[data-kira-sky-map]");' +
    'const canvas=atlas?.querySelector("canvas");' +
    'const context=canvas?.getContext("2d");' +
    'const pixel=context?.getImageData(Math.floor(canvas.width/2),Math.floor(canvas.height/2),1,1).data;' +
    'const selected=root?.querySelector("[aria-label=\\\"Copy date\\\"]")?.textContent;' +
    'const rail=root?.querySelector("[data-calendar-rail]");' +
    'const railBox=rail?.getBoundingClientRect();' +
    'const skyBox=atlas?.getBoundingClientRect();' +
    'document.documentElement.dataset.smokeSkyResponsive=!!root && !!railBox && !!skyBox && ' +
      'root.scrollWidth<=root.clientWidth+2 && ' +
      (expectStacked
        ? 'skyBox.top>=railBox.bottom-2 && root.scrollHeight>root.clientHeight;'
        : 'skyBox.left>=railBox.right-2 && skyBox.width>300;') +
    'document.documentElement.dataset.smokeSkyPainted=!!pixel && pixel[3]===255;' +
    'document.documentElement.dataset.smokeSkyMode=root?.dataset.calendarMode||"none";' +
    'document.documentElement.dataset.smokeSkySize=(canvas?.width||0)+"x"+(canvas?.height||0);' +
    'document.documentElement.dataset.smokeSkyDate=!!window.__kiraCalendarBefore && ' +
      'window.__kiraCalendarBefore!==root?.querySelector("[aria-label=\\\"Date picker\\\"] h2")?.textContent && ' +
      '!!atlas?.textContent?.includes("Звёздное небо");' +
  '},2150));</script>';
  const document = html
    .replace('<html lang="en">', '<html lang="en" data-kira-surface="calendar">')
    .replace('</head>', '<style>html,body,#app{height:100%;margin:0}</style>' + scriptFor(steps, 1900) + check + '</head>');
  const path = join(dir, 'calendar-sky-' + windowSize.split(',')[0] + '.html');
  await writeFile(path, document);
  const output = execFileSync(chrome, [
    '--headless=new', '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage',
    '--disable-background-networking', '--window-size=' + windowSize,
    '--virtual-time-budget=2800', '--dump-dom', 'file://' + path,
  ], { encoding: 'utf8', timeout: 15000, stdio: ['ignore', 'pipe', 'ignore'] });
  const size = output.match(/data-smoke-sky-size="(\d+)x(\d+)"/);
  if (!output.includes('data-smoke-sky-mode="window"') ||
      !output.includes('data-smoke-sky-painted="true"') ||
      !output.includes('data-smoke-sky-date="true"') ||
      !output.includes('data-smoke-sky-responsive="true"') ||
      !size || Number(size[1]) < 250 || Number(size[2]) < 250) {
    throw new Error('Calendar detached atlas failed to paint date-aware sky: ' +
      (output.match(/data-smoke-sky-[a-z-]+="[^"]*"/g) ?? []).join(', '));
  }
  console.log('OK: detached Calendar sky painted at ' + windowSize + (expectStacked ? ' (stacked)' : ' (two-column)'));
}

/** Checks the real responsive Settings window and editable display map
 * at the compositor's narrow and wide viewport classes. */
async function smokeSettingsResponsive(width, narrow) {
  const steps = [
    { delay: 25, kind: 'settings', generation: 1 },
    { delay: 180, kind: 'settings-about' },
    { delay: 520, kind: 'settings-detach' },
    { delay: 780, kind: 'settings-navigation' },
    { delay: 920, kind: 'settings-displays-data' },
  ];
  const checks = '<script>window.addEventListener("load",()=>setTimeout(()=>{' +
    'const root=document.querySelector("main[aria-label=\\\"Kira Settings\\\"]");' +
    'const aside=root?.querySelector("aside");' +
    'const sideVisible=!!aside&&getComputedStyle(aside).display!=="none";' +
    'const selector=root?.querySelector("select[aria-label=\\\"Раздел настроек\\\"]");' +
    'const selectorVisible=!!selector&&getComputedStyle(selector.parentElement.parentElement).display!=="none";' +
    'const display=root?.querySelector("section[aria-label=\\\"Настройки дисплеев\\\"]");' +
    'const map=display?.querySelector("[aria-label=\\\"Схема расположения мониторов\\\"]");' +
    'const bounds=map?.getBoundingClientRect();' +
    'const monitors=Array.from(map?.querySelectorAll("button")||[]);' +
    'const contained=!!bounds&&monitors.length===2&&monitors.every(el=>el.getBoundingClientRect().right<=bounds.right+2);' +
    'const selected=aside?.querySelector("button[data-kira-tab=displays]");' +
    'const rail=aside?.querySelector("[data-kira-flow-indicator]");' +
    'const a=selected?.getBoundingClientRect(),b=rail?.getBoundingClientRect();' +
    'const flowed=!!a&&!!b&&Math.abs(a.top-b.top)<2&&Math.abs(a.height-b.height)<2;' +
    'const normal=root?.getBoundingClientRect();' +
    'const noOverflow=!!root&&root.scrollWidth<=root.clientWidth+2&&!!normal&&normal.width>400;' +
    'document.documentElement.dataset.smokeSettingsResponsive=!!root&&' +
      'root.dataset.settingsMode==="window"&&!!display&&contained&&noOverflow&&' +
      (narrow
        ? '!sideVisible&&selectorVisible&&selector.value==="displays";'
        : 'sideVisible&&!selectorVisible&&selected?.getAttribute("aria-current")==="page"&&flowed;') +
    'document.documentElement.dataset.smokeSettingsWidths=(normal?.width||0)+"/"+(bounds?.width||0);' +
  '},2050));</script>';
  const page = html
    .replace('<html lang="en">', '<html lang="en" data-kira-surface="calendar">')
    .replace('</head>', '<style>html,body,#app{height:100%;margin:0}</style>' + scriptFor(steps, 1900) + checks + '</head>');
  const filename = join(dir, 'settings-responsive-' + width + '.html');
  await writeFile(filename, page);
  const output = execFileSync(chrome, [
    '--headless=new', '--no-sandbox', '--disable-gpu',
    '--disable-dev-shm-usage', '--disable-background-networking',
    '--window-size=' + width + ',760', '--virtual-time-budget=2900',
    '--dump-dom', 'file://' + filename,
  ], { encoding: 'utf8', timeout: 18000, stdio: ['ignore', 'pipe', 'ignore'] });
  if (!output.includes('data-smoke-settings-responsive="true"')) {
    throw new Error('Settings responsive/navigation/display map regression at ' + width +
      'px: ' + (output.match(/data-smoke-settings-(?:responsive|widths)="[^"]*"/g) ?? []).join(', '));
  }
  console.log('OK: Settings ' + width + 'px responsive navigation and display editor');
}

try {
  await smokeTopBar();
  await smokeSettingsResponsive(1100, false);
  await smokeSettingsResponsive(630, true);
  await smokeCalendarSky('1100,820', false);
  await smokeCalendarSky('680,720', true);
  await smoke('calendar', [{ delay: 25, kind: 'calendar', generation: 1 }], 'Kira Calendar', 'tenet');
  await smoke('calendar-click-clock-chaos', [
    { delay: 25, kind: 'calendar', generation: 1 },
    { delay: 130, kind: 'clock-chaos' },
    // Interrupt the still-running first scatter with another click.
    { delay: 450, kind: 'clock-chaos' },
  ], 'Kira Calendar', 'clock');
  await smoke('calendar-month-and-day-motion', [
    { delay: 25, kind: 'calendar', generation: 1 },
    { delay: 180, kind: 'calendar-next' },
    { delay: 420, kind: 'calendar-pick' },
  ], 'Kira Calendar', 'calendar');
  await smoke('system', [{ delay: 25, kind: 'system', generation: 1 }], 'Kira System Monitor', 'tenet');
  await smoke('system-live-telemetry', [
    { delay: 25, kind: 'system', generation: 1 },
    { delay: 170, kind: 'metrics' },
    { delay: 280, kind: 'pulse' },
  ], 'Kira System Monitor', 'system');
  await smoke('settings-widget', [
    { delay: 25, kind: 'settings', generation: 1 },
    { delay: 180, kind: 'settings-about' },
  ], 'Kira Settings', 'settings-widget');
  await smoke('settings-window', [
    { delay: 25, kind: 'settings', generation: 1 },
    { delay: 180, kind: 'settings-about' },
    { delay: 520, kind: 'settings-detach' },
  ], 'Kira Settings', 'settings-window');
  await smoke('settings-displays', [
    { delay: 25, kind: 'settings', generation: 1 },
    { delay: 180, kind: 'settings-displays' },
    { delay: 350, kind: 'settings-displays-data' },
  ], 'Kira Settings', 'settings-displays');
  await smoke('launcher', [{ delay: 25, kind: 'launcher', generation: 1 }], 'Kira Launcher', 'tenet');
  // Native close signal triggers the original GSAP master in reverse,
  // then emits a generation-bound ACK; the host keeps the DOM alive until
  // that ACK. Exercise all three real bundled Svelte widget roots.
  for (const kind of ['calendar', 'system', 'launcher']) {
    const title = kind === 'calendar' ? 'Kira Calendar' : kind === 'system' ? 'Kira System Monitor' : 'Kira Launcher';
    await smoke(kind + '-tenet-reverse-dismissal', [
      { delay: 25, kind, generation: 1 },
      { delay: 800, kind: 'tenet-close', generation: 1 },
    ], title, 'reverse');
  }
  await smoke('calendar-tenet-reopen-in-same-frame', [
    { delay: 25, kind: 'calendar', generation: 1 },
    // Virtual Chrome can advance >500ms of GSAP in one 30ms timer step.
    // A same-frame cancellation guarantees we probe the actual race rather
    // than a close that has already legitimately completed.
    { delay: 800, kind: 'tenet-close-reopen', generation: 1 },
  ], 'Kira Calendar', 'reverse-reopen');
  await smoke('calendar-reduced-motion', [{ delay: 25, kind: 'calendar', generation: 1 }], 'Kira Calendar', 'reduced');
  await smoke('system-reduced-motion', [{ delay: 25, kind: 'system', generation: 1 }], 'Kira System Monitor', 'reduced');
  await smoke('launcher-selection-flow', [
    { delay: 25, kind: 'launcher', generation: 1 },
    { delay: 90, kind: 'catalog' },
    { delay: 220, kind: 'launcher-down' },
    { delay: 310, kind: 'launcher-down' },
  ], 'Kira Launcher', 'flow');
  await smoke('system-tab-flow', [
    { delay: 25, kind: 'system', generation: 1 },
    { delay: 220, kind: 'system-processes' },
    { delay: 320, kind: 'system-overview' },
  ], 'Kira System Monitor', 'flow');
  await smoke('launcher-reduced-motion', [{ delay: 25, kind: 'launcher', generation: 1 }], 'Kira Launcher', 'reduced');
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
