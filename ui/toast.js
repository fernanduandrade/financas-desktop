'use strict';
/* Aviso do Finanças: entra deslizando da direita, tem botões, barra de tempo que pausa com o mouse em cima
   e se empilha com outros avisos (o Rust posiciona). */
const T = window.__TAURI__;
const id = +new URLSearchParams(location.search).get('id');
const esc = s => String(s ?? '').replace(/[&<>"']/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]));
const svg = d => `<svg viewBox="0 0 24 24"><path d="${d}"/></svg>`;
const KIND = {
  diario: { c: '#0d7f5e', i: 'M4 20h4L19 9l-4-4L4 16zM14 6l4 4' },
  contas: { c: '#c47d0e', i: 'M5 6h14a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1zM8 3v4M16 3v4M4 11h16' },
  semanal: { c: '#2457a6', i: 'M4 20V12M10 20V5M16 20v-7M21 20H3' },
  info: { c: '#0d7f5e', i: 'M6 16V11a6 6 0 0 1 12 0v5l2 2H4zM10 21h4' },
};
let closed = false;

async function main() {
  const p = await T.core.invoke('toast_payload', { id });
  const k = KIND[p.kind] || KIND.info;
  const card = document.getElementById('card');
  card.style.setProperty('--k', k.c);
  card.innerHTML = `
    <div class="band"><span class="brandline"><img src="/icon-64.png" alt="">Finanças <span class="when">· agora</span></span>
      <button class="x" id="x" aria-label="Fechar">${svg('M7 7l10 10M17 7L7 17')}</button></div>
    <div class="body"><div class="ico">${svg(k.i)}</div>
      <div><div class="title">${esc(p.title)}</div><div class="msg">${esc(p.message)}</div></div></div>
    <div class="acts">${p.actions.map((a, i) => `<button data-i="${i}" class="${i === 0 ? 'primary' : ''}">${esc(a)}</button>`).join('')}</div>
    <div class="bar"><i id="bar"></i></div>`;
  if (!p.actions.length) card.querySelector('.acts').remove();
  document.getElementById('x').onclick = () => dismiss();
  card.querySelectorAll('[data-i]').forEach(b => b.onclick = () => {
    if (closed) return;
    closed = true;
    card.classList.add('out');
    setTimeout(() => T.core.invoke('toast_action', { id, index: +b.dataset.i }), 160);
  });
  await document.fonts.ready;
  await T.core.invoke('toast_ready', { id, height: card.offsetHeight + 24 });
  card.classList.add(p.wiggle ? 'wiggle' : 'in');
  countdown(p.seconds * 1000);
}

function countdown(total) {
  const bar = document.getElementById('bar');
  let left = total, last = performance.now(), paused = false;
  document.body.addEventListener('mouseenter', () => paused = true);
  document.body.addEventListener('mouseleave', () => paused = false);
  const tick = now => {
    if (closed) return;
    if (!paused) left -= now - last;
    last = now;
    bar.style.transform = `scaleX(${Math.max(0, left / total)})`;
    if (left <= 0) return dismiss();
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

function dismiss() {
  if (closed) return;
  closed = true;
  document.getElementById('card').classList.add('out');
  setTimeout(() => T.core.invoke('toast_close', { id }), 190);
}

main().catch(() => T.core.invoke('toast_close', { id }));
