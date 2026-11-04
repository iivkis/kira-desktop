#!/usr/bin/env node
/**
 * Exercise the real compiled Top Bar in Chromium at different CSS widths and
 * device scales. Geometry/keyboard checks complement, never replace, Niri QA.
 * No external resources, Playwright dependency or mock rendering of Svelte.
 */
import { readFile, writeFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const chrome = process.env.CHROME_BIN || 'google-chrome';
const html = await readFile(new URL('../dist/index.html', import.meta.url), 'utf8');
const dir = await mkdtemp(join(tmpdir(), 'kira-topbar-warning-'));
const longError = 'НЕ УДАЛОСЬ ЗАГРУЗИТЬ КОНФИГУРАЦИЮ. ' +
  'ПРОВЕРЬТЕ СИНТАКСИС KDL И ПРАВА ДОСТУПА. '.repeat(35);
const setup = `<script>
window.webkit={messageHandlers:{kiraDiagnostics:{postMessage(){}},kiraCore:{postMessage(){}}}};
window.addEventListener('load',()=>{
  const msg=${JSON.stringify(longError)};
  const config={compact:false,title:'KIRA DESKTOP',show_seconds:false,show_names:false,max_workspaces:8};
  setTimeout(()=>{
    window.dispatchEvent(new CustomEvent('kira:config',{detail:{config,error:msg}}));
  },50);
  setTimeout(()=>{
    // Missing launcher native handler yields a second, real UI warning.
    document.querySelector('[aria-label="Открыть меню приложений"]')?.click();
  },125);
  setTimeout(()=>{
    const trigger=document.querySelector('button[aria-label^="Открыть предупреждение"]');
    document.documentElement.dataset.smokeWarningTrigger=!!trigger && trigger.getAttribute('aria-haspopup')==='dialog';
    trigger?.click();
  },190);
  setTimeout(()=>{
    const root=document.querySelector('main[aria-label="Верхняя панель Kira Desktop"]');
    const dialog=root?.querySelector('[role="dialog"][aria-label="Подробности предупреждения"]');
    const page=dialog?.querySelector('textarea[readonly][aria-label^="Полный текст"]');
    const under=root?.querySelector('section[aria-label="Рабочие пространства Niri"]');
    const rect=dialog?.getBoundingClientRect();
    document.documentElement.dataset.smokeWarningOpen=!!root && !!dialog && !!page &&
      root.getBoundingClientRect().height===42 && rect.height===42 &&
      Math.abs(rect.width-root.getBoundingClientRect().width)<1 &&
      under.inert && page===document.activeElement &&
      !dialog.textContent.includes(msg);
    const controls=Array.from(dialog?.querySelectorAll('textarea, button') ?? []);
    const last=controls[controls.length-1];
    last?.focus();
    const tab=new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true});
    last?.dispatchEvent(tab);
    const forward=tab.defaultPrevented && document.activeElement===page;
    const shift=new KeyboardEvent('keydown',{key:'Tab',shiftKey:true,bubbles:true,cancelable:true});
    page?.dispatchEvent(shift);
    document.documentElement.dataset.smokeWarningFocusTrap=forward &&
      shift.defaultPrevented && document.activeElement===last;
    dialog?.querySelector('[aria-label="Следующее предупреждение"]')?.click();
  },260);
  setTimeout(()=>{
    const dialog=document.querySelector('[role="dialog"][aria-label="Подробности предупреждения"]');
    const page=dialog?.querySelector('textarea[readonly][aria-label^="Полный текст"]');
    const full=page?.value ?? '';
    const expected='КОНФИГ: ' + msg;
    const scrolled=page && page.scrollHeight>page.clientHeight;
    if(page)page.scrollTop=page.scrollHeight;
    document.documentElement.dataset.smokeWarningLongExact=full===expected;
    document.documentElement.dataset.smokeWarningLongScrollable=!!scrolled;
    document.documentElement.dataset.smokeWarningLongScrolled=!!page && page.scrollTop>0;
    document.documentElement.dataset.smokeWarningLong=full===expected && scrolled && page.scrollTop>0;
    // Test the keyboard dismissal and subsequent focus restoration.
    window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
  },335);
  setTimeout(()=>{
    const root=document.querySelector('main[aria-label="Верхняя панель Kira Desktop"]');
    const trigger=root?.querySelector('button[aria-label^="Открыть предупреждение"]');
    const clock=root?.querySelector('button[title="Открыть календарь"]');
    document.documentElement.dataset.smokeWarningClosed=!!root && !!trigger && !!clock &&
      !root.querySelector('[role="dialog"]') && trigger===document.activeElement &&
      !root.querySelector('section[aria-label="Рабочие пространства Niri"]').inert &&
      trigger.getAttribute('aria-label').length<140 && trigger.title.includes(msg) &&
      clock.getBoundingClientRect().right<=root.getBoundingClientRect().right+1;
    trigger?.click();
  },430);
  setTimeout(()=>{
    document.querySelector('[aria-label="Закрыть подробности"]')?.click();
  },480);
  setTimeout(()=>{
    const root=document.querySelector('main[aria-label="Верхняя панель Kira Desktop"]');
    document.documentElement.dataset.smokeWarningButtonClose=!!root &&
      !root.querySelector('[role="dialog"]') &&
      !!root.querySelector('button[aria-label^="Открыть предупреждение"]');
  },565);
  // A recovered launcher leaves exactly one real config error.
  setTimeout(()=>{
    window.webkit.messageHandlers.kiraLauncher={postMessage(){}};
    document.querySelector('[aria-label="Открыть меню приложений"]')?.click();
  },610);
  setTimeout(()=>{
    const root=document.querySelector('main[aria-label="Верхняя панель Kira Desktop"]');
    root?.querySelector('button[aria-label^="Открыть предупреждение"]')?.click();
  },660);
  setTimeout(()=>{
    const dialog=document.querySelector('[role="dialog"][aria-label="Подробности предупреждения"]');
    const box=dialog?.querySelector('textarea[readonly]');
    document.documentElement.dataset.smokeWarningSingle=!!dialog && !!box &&
      box.value==='КОНФИГ: '+msg &&
      !dialog.querySelector('[aria-label="Следующее предупреждение"]');
    // The live config event removes the final warning while the inspector is open.
    window.dispatchEvent(new CustomEvent('kira:config',{detail:{config,error:null}}));
  },715);
  setTimeout(()=>{
    const root=document.querySelector('main[aria-label="Верхняя панель Kira Desktop"]');
    const settings=root?.querySelector('button[aria-label="Открыть настройки системы"]');
    document.documentElement.dataset.smokeWarningCleared=!!root && !!settings &&
      !root.querySelector('[role="dialog"]') &&
      !root.querySelector('button[aria-label^="Открыть предупреждение"]') &&
      settings===document.activeElement &&
      !root.querySelector('section[aria-label="Система и местное время"]').inert;
    // A later incident must NOT bring back the previously opened modal.
    window.dispatchEvent(new CustomEvent('kira:config',{detail:{config,error:msg}}));
  },780);
  setTimeout(()=>{
    const root=document.querySelector('main[aria-label="Верхняя панель Kira Desktop"]');
    const trigger=root?.querySelector('button[aria-label^="Открыть предупреждение"]');
    document.documentElement.dataset.smokeWarningRecovered=!!root && !!trigger &&
      !root.querySelector('[role="dialog"]') &&
      trigger.getAttribute('aria-expanded')==='false';
  },870);
});
</script>`;

try {
  const page = html.replace('</head>', setup + '</head>');
  const filename = join(dir, 'warnings.html');
  await writeFile(filename, page);
  for (const [width, scale] of [[540,1],[540,1.25],[680,1.5],[1280,1]]) {
    const output = execFileSync(chrome, [
      '--headless=new','--no-sandbox','--disable-gpu','--disable-dev-shm-usage',
      '--disable-background-networking',`--window-size=${width},440`,
      `--force-device-scale-factor=${scale}`, '--virtual-time-budget=1100',
      '--dump-dom','file://' + filename,
    ], { encoding:'utf8',timeout:15000,stdio:['ignore','pipe','ignore'] });
    const flags = ['trigger','open','focus-trap','long','closed','button-close','single','cleared','recovered'];
    const failures = flags.filter(flag => !output.includes('data-smoke-warning-' + flag + '="true"'));
    if(failures.length){
      const diagnostics=output.match(/data-smoke-warning[a-z-]+="[^"]*"/gi) ?? [];
      throw new Error(`Warning inspector at ${width}px/${scale}x failed ${failures.join(', ')}: ${diagnostics.join(' ')}`);
    }
    console.log(`OK: Top Bar warning inspector at ${width}px / ${scale}x`);
  }
} finally {
  await rm(dir,{recursive:true,force:true});
}
