'use strict';
/* Painel da bandeja: resumo + atalhos. O Rust avisa "panel:show"; o painel busca o resumo,
   mede a própria altura e pede para ser posicionado perto do relógio. */
const T = window.__TAURI__;
const BRL = new Intl.NumberFormat('pt-BR', { style: 'currency', currency: 'BRL' });
const money = c => BRL.format((c || 0) / 100);
const esc = s => String(s ?? '').replace(/[&<>"']/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]));
const D = s => { const [y, m, d] = s.split('-').map(Number); return new Date(y, m - 1, d); };
const svg = d => `<svg viewBox="0 0 24 24"><path d="${d}"/></svg>`;
const ICON = {
  open: 'M14 4h6v6M20 4l-9 9M18 14v6H4V6h6',
  plus: 'M12 5v14M5 12h14',
  bell: 'M6 16V11a6 6 0 0 1 12 0v5l2 2H4zM10 21h4',
  power: 'M12 3v8M6.6 6.6a8 8 0 1 0 10.8 0',
};
function heroMoney(c) {
  const neg = c < 0, v = Math.abs(c || 0);
  const int = Math.floor(v / 100).toLocaleString('pt-BR'), cents = String(v % 100).padStart(2, '0');
  return `${neg ? '−' : ''}<span class="cur">R$</span>${int}<span class="cents">,${cents}</span>`;
}

async function show() {
  let s;
  try { s = await T.core.invoke('api', { method: 'GET', path: 'summary', body: null }); }
  catch (e) { s = { balance_cents: 0, forecast_cents: 0, invoices_cents: 0, upcoming: [], launched_today: 0, today: new Date().toISOString().slice(0, 10) }; }
  const today = D(s.today);
  const date = today.toLocaleDateString('pt-BR', { weekday: 'long', day: 'numeric', month: 'short' }).replace('.', '');
  const next = (s.upcoming || []).find(i => i.amount_cents < 0);
  const n = s.launched_today || 0;
  document.getElementById('card').innerHTML = `
    <div class="band">
      <div class="row"><span class="brandline"><img src="/icon-64.png" alt="">Finanças</span><span class="date">${esc(date)}</span></div>
      <div class="label">Saldo em contas</div>
      <div class="value">${heroMoney(s.balance_cents)}</div>
      <div class="foot">Previsto no fim do mês: <b>${money(s.forecast_cents)}</b></div>
    </div>
    <div class="info">
      <div class="r"><span>Faturas a pagar</span><b class="money ${s.invoices_cents < 0 ? 'neg' : ''}">${money(Math.abs(s.invoices_cents))}</b></div>
      <div class="r"><span>Próxima conta</span>${next
        ? `<b class="${next.overdue ? 'neg' : ''}" title="${esc(next.description)} · ${money(next.amount_cents)}"><small>${D(next.date).toLocaleDateString('pt-BR', { day: '2-digit', month: '2-digit' })}</small>${esc(next.description)}</b>`
        : '<b>nada em 15 dias</b>'}</div>
      <div class="r"><span>Lançado hoje</span><b class="${n ? 'ok' : 'amber'}">${n ? `${n} lançamento${n > 1 ? 's' : ''}` : 'nada ainda'}</b></div>
    </div>
    <hr>
    <div class="menu">
      <button data-a="abrir">${svg(ICON.open)}Abrir app</button>
      <button data-a="novo">${svg(ICON.plus)}Lançamento rápido<kbd>N</kbd></button>
      <button data-a="lembretes">${svg(ICON.bell)}Lembretes</button>
      <hr>
      <button data-a="sair" class="quiet">${svg(ICON.power)}Sair</button>
    </div>`;
  document.querySelectorAll('[data-a]').forEach(b => b.onclick = () => act(b.dataset.a));
  document.body.classList.remove('show');
  document.body.classList.toggle('top', !navigator.userAgent.includes('Windows'));
  await document.fonts.ready;
  const h = document.getElementById('card').offsetHeight + 24;
  await T.core.invoke('panel_fit', { height: h });
  requestAnimationFrame(() => document.body.classList.add('show'));
}
const act = what => { document.body.classList.remove('show'); T.core.invoke('panel_action', { what }); };

T.event.listen('panel:show', show);
document.addEventListener('keydown', e => {
  if (e.key === 'Escape') act('fechar');
  if (e.key === 'n' || e.key === 'N') act('novo');
});
