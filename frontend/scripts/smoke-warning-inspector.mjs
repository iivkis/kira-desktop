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
    const trigger=document.querySelector('button[aria-label^="Открыть полный текст предупреждения"]');
    document.documentElement.dataset.smokeWarningTrigger=!!trigger && trigger.getAttribute('aria-haspopup')==='dialog';
    trigger?.click();
  },190);
  setTimeout(()=>{
    const root=document.querySelector('main[aria-label="Верхняя панель Kira Desktop"]');
    const dialog=root?.querySelector('[role="dialog"][aria-label="Подробности предупреждения"]');
    const page=dialog?.querySelector('[role="document"]');
    const under=root?.querySelector('section[aria-label="Рабочие пространства Niri"]');
    const rect=dialog?.getBoundingClientRect();
    document.documentElement.dataset.smokeWarningOpen=!!root && !!dialog && !!page &&
      root.getBoundingClientRect().height===42 && rect.height===42 &&
      Math.abs(rect.width-root.getBoundingClientRect().width)<1 &&
      under.inert && page===document.activeElement &&
      !dialog.textContent.includes(msg);
    dialog?.querySelector('[aria-label="Следующее предупреждение"]')?.click();
  },260);
  setTimeout(()=>{
    const dialog=document.querySelector('[role="dialog"][aria-label="Подробности предупреждения"]');
    const page=dialog?.querySelector('[role="document"]');
    const full=page?.textContent.trim() ?? '';
    const scrolled=page && page.scrollHeight>page.clientHeight;
    if(page)page.scrollTop=page.scrollHeight;
    document.documentElement.dataset.smokeWarningLong=full===msg && scrolled && page.scrollTop>0;
    // Test the keyboard dismissal and subsequent focus restoration.
    window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
  },335);
  setTimeout(()=>{
    const root=document.querySelector('main[aria-label="Верхняя панель Kira Desktop"]');
    const trigger=root?.querySelector('button[aria-label^="Открыть полный текст предупреждения"]');
    const clock=root?.querySelector('button[title="Открыть календарь"]');
    document.documentElement.dataset.smokeWarningClosed=!!root && !!trigger && !!clock &&
      !root.querySelector('[role="dialog"]') && trigger===document.activeElement &&
      !root.querySelector('section[aria-label="Рабочие пространства Niri"]').inert &&
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
      !!root.querySelector('button[aria-label^="Открыть полный текст предупреждения"]');
  },565);
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
    const flags = ['Trigger','Open','Long','Closed','ButtonClose'];
    const failures = flags.filter(flag => !output.includes('data-smoke-warning' + flag + '="true"'));
    if(failures.length){
      const diagnostics=output.match(/data-smoke-warning[a-z-]+="[^"]*"/gi) ?? [];
      throw new Error(`Warning inspector at ${width}px/${scale}x failed ${failures.join(', ')}: ${diagnostics.join(' ')}`);
    }
    console.log(`OK: Top Bar warning inspector at ${width}px / ${scale}x`);
  }
} finally {
  await rm(dir,{recursive:true,force:true});
}
