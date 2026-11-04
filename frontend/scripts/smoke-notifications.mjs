#!/usr/bin/env node
/** Exercise the bundled NotificationHost in a real headless Chromium DOM. */
import { readFile, writeFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const raw = await readFile(new URL('../dist/index.html', import.meta.url), 'utf8');
const dir = await mkdtemp(join(tmpdir(), 'kira-notification-smoke-'));
const chrome = process.env.CHROME_BIN || 'google-chrome';
const script = `
<script>
window.__kiraSmokeMessages = [];
// The native host is mapped with opacity=0 until ACK. Some WebKit versions
// classify such a mapped page as hidden; its DOM must still be able to ACK.
Object.defineProperty(document, 'visibilityState', {value:'hidden', configurable:true});
window.webkit = {messageHandlers:{
  kiraNotificationUi:{postMessage(message){window.__kiraSmokeMessages.push(String(message));}},
  kiraDiagnostics:{postMessage(){}},
}};
const notices = [
  {id:1,app:"Telegram",summary:"First",body:"<img src=x onerror=alert(1)>",
   critical:false,actions:[{key:"default",label:"Open"}]},
  {id:2,app:"Discord",summary:"Second",body:"Hello",
   critical:false,actions:[{key:"reply",label:"Ответить"}]},
  {id:3,app:"Browser",summary:"Third",body:"",
   critical:true,actions:[]},
];
window.addEventListener("load",()=>{
  // Corrupt snapshots from a transport or stale renderer must not poison
  // keyed Svelte state or consume a future valid revision.
  setTimeout(()=>window.dispatchEvent(new CustomEvent("kira:notifications:update",{
    detail:{revision:2,height:320,cardHeight:96,visible:[notices[0],notices[0]]}
  })),40);
  setTimeout(()=>window.dispatchEvent(new CustomEvent("kira:notifications:update",{
    detail:{revision:1,height:320,cardHeight:96,visible:notices}
  })),75);
  setTimeout(()=>{
    const cards=Array.from(document.querySelectorAll("[data-kira-notification-card]"));
    const root=document.querySelector("main[data-kira-ui-root]");
    const rects=cards.map(card=>card.getBoundingClientRect());
    const ordered=cards.map(card=>card.getAttribute("data-kira-notification-card")).join(",")==="1,2,3" &&
      rects.length===3 && rects[0].top>rects[1].top && rects[1].top>rects[2].top &&
      getComputedStyle(root).flexDirection==="column-reverse";
    const capped=rects.length===3 && rects.every(rect=>rect.height>0 && rect.height<=320/3);
    const escaped=cards[0]?.textContent.includes("<img src=x onerror=alert(1)>") &&
      cards[0]?.querySelector("img")===null;
    cards[0]?.querySelector("button[aria-label^='Открыть уведомление']")?.click();
    Array.from(cards[1]?.querySelectorAll("button")||[]).find(
      button=>button.textContent.includes("Ответить"))?.click();
    cards[2]?.querySelector("button[aria-label^='Закрыть уведомление']")?.click();
    const actions=["action:1:default","action:2:reply","close:3"].every(
      message=>window.__kiraSmokeMessages.includes(message));
    const ready=window.__kiraSmokeMessages.includes("ready");
    const painted=window.__kiraSmokeMessages.includes("painted:1:1,2,3");
    document.documentElement.dataset.smokeNotifications=String(
      !!root && ordered && capped && escaped && actions && ready && painted);
    document.documentElement.dataset.smokeNotes=JSON.stringify({
      ordered,capped,escaped,actions,ready,painted,messages:window.__kiraSmokeMessages,
      rects:rects.map(rect=>[rect.y,rect.height]),
    });
  },850);
});
</script>`;
const document=raw.replace('<html lang="ru">','<html lang="ru" data-kira-surface="notifications">')
  .replace('</head>',script+'\n</head>');
if (!document.includes('data-kira-surface="notifications"')) throw Error('Missing notification marker');
try {
  const path=join(dir,'notifications.html');
  await writeFile(path,document);
  const output=execFileSync(chrome,[
    '--headless=new','--no-sandbox','--disable-gpu','--disable-dev-shm-usage',
    '--disable-background-networking','--window-size=480,640',
    '--virtual-time-budget=2400','--dump-dom','file://'+path,
  ],{encoding:'utf8',timeout:15000,stdio:['ignore','pipe','ignore']});
  if (!output.includes('data-smoke-notifications="true"')) {
    const flags=output.match(/data-smoke-(?:notifications|notes)="[^"]*"/g) || [];
    throw Error('NotificationHost DOM/paint/action smoke failed: '+flags.join(', '));
  }
  console.log('NotificationHost: bottom-up cards, size caps, escaped body, actions and exact ordered-ID paint ACK OK');
} finally {
  await rm(dir,{recursive:true,force:true});
}
