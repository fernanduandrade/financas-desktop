'use strict';
/* Finanças Local — interface (sem dependências externas) */

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const BRL = new Intl.NumberFormat('pt-BR', { style: 'currency', currency: 'BRL' });
const money = c => BRL.format((c || 0) / 100);
const esc = s => String(s ?? '').replace(/[&<>"']/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]));
const D = s => { const [y, m, d] = s.split('-').map(Number); return new Date(y, m - 1, d); };
const iso = d => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
const br = s => s ? D(s).toLocaleDateString('pt-BR') : '';
const brShort = s => s ? D(s).toLocaleDateString('pt-BR', { day: '2-digit', month: '2-digit' }) : '';
const cap = s => s.charAt(0).toUpperCase() + s.slice(1);
const monthLabel = d => cap(d.toLocaleDateString('pt-BR', { month: 'long', year: 'numeric' }));
const addMonths = (d, n) => new Date(d.getFullYear(), d.getMonth() + n, 1);
const lastDay = d => new Date(d.getFullYear(), d.getMonth() + 1, 0);
const cls = c => c < 0 ? 'neg' : c > 0 ? 'pos' : '';

const I = {
  check: '<svg viewBox="0 0 24 24"><path d="M5 12l5 5L20 7"/></svg>',
  edit: '<svg viewBox="0 0 24 24"><path d="M4 20h4L19 9l-4-4L4 16zM14 6l4 4"/></svg>',
  trash: '<svg viewBox="0 0 24 24"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/></svg>',
  left: '<svg viewBox="0 0 24 24"><path d="M15 6l-6 6 6 6"/></svg>',
  right: '<svg viewBox="0 0 24 24"><path d="M9 6l6 6-6 6"/></svg>',
  plus: '<svg viewBox="0 0 24 24"><path d="M12 5v14M5 12h14"/></svg>',
  card: '<svg viewBox="0 0 24 24"><path d="M3 7a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM3 10h18"/></svg>',
  swap: '<svg viewBox="0 0 24 24"><path d="M7 7h13l-4-4M17 17H4l4 4"/></svg>',
  x: '<svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg>',
  repeat: '<svg viewBox="0 0 24 24"><path d="M4 12a8 8 0 0 1 14-5l2 2M20 12a8 8 0 0 1-14 5l-2-2M20 4v5h-5M4 20v-5h5"/></svg>',
};

// No Tauri, a API é um comando Rust (sem servidor HTTP). Fora dele, continua o fetch de antes.
const TAURI = window.__TAURI__;
if (TAURI) document.documentElement.classList.add('tauri');

async function api(method, path, body) {
  if (TAURI) {
    try { return await TAURI.core.invoke('api', { method, path, body: body === undefined ? null : body }); }
    catch (e) { throw new Error(typeof e === 'string' ? e : (e?.message || 'Algo deu errado')); }
  }
  const r = await fetch('/api/' + path, {
    method, headers: { 'Content-Type': 'application/json' }, body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j.erro || 'Algo deu errado');
  return j;
}

function toast(msg) {
  const t = $('#toast');
  t.textContent = msg;
  t.classList.add('show');
  clearTimeout(toast._t);
  toast._t = setTimeout(() => t.classList.remove('show'), 2600);
}

function parseMoney(s) {
  s = String(s || '').replace(/[R$\s]/g, '');
  if (!s) return NaN;
  if (s.includes(',')) s = s.replace(/\./g, '').replace(',', '.');
  return Math.round(parseFloat(s) * 100);
}
const moneyInput = c => (Math.abs(c || 0) / 100).toLocaleString('pt-BR', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

let META = null;
const S = { month: null, account: '', category: '', q: '', card: null, due: null, repMonth: null };

async function loadMeta() {
  META = await api('GET', 'meta');
  if (!S.month) { const t = D(META.today); S.month = new Date(t.getFullYear(), t.getMonth(), 1); S.repMonth = S.month; }
}
const acc = id => META.accounts.find(a => a.id === id);
const cat = id => META.categories.find(c => c.id === id);
const activeAccounts = kind => META.accounts.filter(a => !a.archived && (!kind || a.kind === kind));

function catOptions(kind, selected) {
  const cats = META.categories.filter(c => c.kind === kind);
  const roots = cats.filter(c => !c.parent_id).sort((a, b) => a.name.localeCompare(b.name, 'pt-BR'));
  let html = '<option value="">Sem categoria</option>';
  for (const r of roots) {
    html += `<option value="${r.id}" ${r.id === selected ? 'selected' : ''}>${esc(r.name)}</option>`;
    for (const s of cats.filter(c => c.parent_id === r.id).sort((a, b) => a.name.localeCompare(b.name, 'pt-BR')))
      html += `<option value="${s.id}" ${s.id === selected ? 'selected' : ''}>&nbsp;&nbsp;&nbsp;↳ ${esc(s.name)}</option>`;
  }
  return html;
}
function accOptions(list, selected) {
  return list.map(a => `<option value="${a.id}" ${a.id === selected ? 'selected' : ''}>${esc(a.name)}${a.kind === 'cartao' ? ' (cartão)' : ''}</option>`).join('');
}
function invoiceDueFor(date, cl, du) {
  let y = date.getFullYear(), m = date.getMonth();
  if (date.getDate() > cl) m++;
  if (!(du > cl)) m++;
  const last = new Date(y, m + 1, 0).getDate();
  return new Date(y, m, Math.min(du, last));
}
const catDot = c => c ? `<span class="dot" style="background:${esc(c.color || '#999')}"></span>` : '';

// ------------------------------------------------------------ ícones de categoria
const P = {
  food: 'M7 3v8a2 2 0 0 0 2 2v8M5 3v5M9 3v5M17 3c-2 0-3 2-3 5s1 4 3 4v9',
  cart: 'M3 4h2l2.4 11h10.2L20 7H6.2M9 20h.01M17 20h.01',
  house: 'M4 11l8-7 8 7v9H4zM10 20v-5h4v5',
  bolt: 'M13 3L5 14h6l-1 7 8-11h-6z',
  drop: 'M12 3s6 6.5 6 11a6 6 0 0 1-12 0c0-4.5 6-11 6-11z',
  wifi: 'M2 9a15 15 0 0 1 20 0M5.5 12.5a10 10 0 0 1 13 0M9 16a5 5 0 0 1 6 0M12 19.5h.01',
  tool: 'M14.5 6.5a4 4 0 0 0 5 5L12 19a2.1 2.1 0 0 1-3-3l7.5-7.5a4 4 0 0 0-2-2zM5 5l3 3',
  heart: 'M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z',
  pill: 'M10.5 20.5a5 5 0 0 1-7-7l6-6a5 5 0 0 1 7 7zM8.5 8.5l7 7',
  car: 'M4 16v-4l2-5h12l2 5v4zM6 16v3M18 16v3M4 12h16M7.5 13.8h.01M16.5 13.8h.01',
  bus: 'M6 4h12a2 2 0 0 1 2 2v11H4V6a2 2 0 0 1 2-2zM4 11h16M7 20v-3M17 20v-3M8 14h.01M16 14h.01',
  bag: 'M5 8h14l-1 12H6zM9 8V6a3 3 0 0 1 6 0v2',
  shirt: 'M8 4L3 7l2 4 3-1v10h8V10l3 1 2-4-5-3a4 4 0 0 1-8 0z',
  sofa: 'M4 11V8a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v3M2 13a2 2 0 0 1 4 0v2h12v-2a2 2 0 0 1 4 0v5H2zM5 18v2M19 18v2',
  book: 'M4 5a2 2 0 0 1 2-2h13v16H6a2 2 0 0 0-2 2zM4 21V5M8 7h7',
  cap: 'M2 9l10-5 10 5-10 5zM6 11v5c3 2 9 2 12 0v-5M22 9v6',
  plane: 'M21 3L3 10l7 3 3 7zM10 13L21 3',
  ticket: 'M3 7h18v3a2 2 0 0 0 0 4v3H3v-3a2 2 0 0 0 0-4zM14 7v10',
  users: 'M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM2 21a7 7 0 0 1 14 0M16 3.5a4 4 0 0 1 0 7.5M18 14a6 6 0 0 1 4 7',
  paw: 'M12 13c-3 0-5 3-5 5 0 1.5 1.2 2 2.5 2 1 0 1.7-.5 2.5-.5s1.5.5 2.5.5c1.3 0 2.5-.5 2.5-2 0-2-2-5-5-5zM4.5 10a1.5 1.8 0 1 0 3 0 1.5 1.8 0 1 0-3 0M8 6.5a1.5 1.8 0 1 0 3 0 1.5 1.8 0 1 0-3 0M13 6.5a1.5 1.8 0 1 0 3 0 1.5 1.8 0 1 0-3 0M16.5 10a1.5 1.8 0 1 0 3 0 1.5 1.8 0 1 0-3 0',
  trend: 'M3 17l6-6 4 4 8-8M15 7h6v6',
  shield: 'M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z',
  repeat: 'M4 12a8 8 0 0 1 14-5l2 2M20 12a8 8 0 0 1-14 5l-2-2M20 4v5h-5M4 20v-5h5',
  receipt: 'M6 3h12v18l-3-2-3 2-3-2-3 2zM9 8h6M9 12h6M9 16h3',
  bank: 'M3 10l9-6 9 6M5 10v8M9.5 10v8M14.5 10v8M19 10v8M3 20h18',
  pix: 'M12 3l3.5 3.5L12 10 8.5 6.5zM12 14l3.5 3.5L12 21l-3.5-3.5zM3 12l3.5-3.5L10 12l-3.5 3.5zM14 12l3.5-3.5L21 12l-3.5 3.5z',
  wallet: 'M4 7a2 2 0 0 1 2-2h12v4M4 7v11a2 2 0 0 0 2 2h14V9H6a2 2 0 0 1-2-2zM16 14.5h.01',
  undo: 'M9 14L4 9l5-5M4 9h10a6 6 0 0 1 0 12h-3',
  gift: 'M4 11h16v9H4zM3 7h18v4H3zM12 7v13M12 7c-1-3-5-4-5-1.5S10 7 12 7c2 0 5-.5 5-1.5S13 4 12 7',
  spark: 'M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM18.5 16l.6 1.9 1.9.6-1.9.6-.6 1.9-.6-1.9-1.9-.6 1.9-.6z',
  brief: 'M4 8h16v11H4zM9 8V5h6v3M4 13h16',
  laptop: 'M5 5h14v10H5zM2 19h20',
  dots: 'M6 12h.01M12 12h.01M18 12h.01',
  card: 'M3 7a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM3 10h18M7 15h3',
  swap: 'M7 7h13l-4-4M17 17H4l4 4',
  bell: 'M6 16V11a6 6 0 0 1 12 0v5l2 2H4zM10 21h4',
  coins: 'M9 8c4 0 7-1.3 7-3s-3-3-7-3-7 1.3-7 3 3 3 7 3zM2 5v6c0 1.7 3 3 7 3M2 11v6c0 1.7 3 3 7 3M22 13c0 1.7-3 3-7 3s-7-1.3-7-3 3-3 7-3 7 1.3 7 3zM8 13v6c0 1.7 3 3 7 3s7-1.3 7-3v-6',
};
const svgP = d => `<svg viewBox="0 0 24 24"><path d="${d}"/></svg>`;
const norm = s => String(s || '').toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '');
const ICON_RULES = [
  [/uber|^99$|onibus|metro/, 'bus'], [/transporte|combust|gasolina|carro|estacion/, 'car'],
  [/mercado|supermerc|feira/, 'cart'], [/delivery|ifood/, 'food'], [/vale alimenta|refei|lanche|aliment|restaur|bares/, 'food'],
  [/energia|luz/, 'bolt'], [/agua/, 'drop'], [/internet|telefon|celular/, 'wifi'], [/manuten|reparo/, 'tool'], [/aluguel|condom|casa|moradia/, 'house'],
  [/medica|farmac|remedio/, 'pill'], [/saude|medic|dentist/, 'heart'], [/vacina|racao|pet/, 'paw'],
  [/roupa/, 'shirt'], [/mobil/, 'sofa'], [/compra/, 'bag'],
  [/livro/, 'book'], [/educa|curso|certifica|escola/, 'cap'],
  [/viagem/, 'plane'], [/lazer|hobb|cinema/, 'ticket'], [/famil|filho|lara|zaza|mae/, 'users'],
  [/reserva/, 'shield'], [/invest|rendiment|dividend/, 'trend'],
  [/assinatura|servico|streaming/, 'repeat'], [/imposto|taxa/, 'receipt'],
  [/consorc|emprest|divida|financ/, 'bank'], [/^pix$/, 'pix'],
  [/salario|renda|venda|ferias/, 'wallet'], [/cashback|estorno/, 'undo'], [/antecipar/, 'card'],
  [/presente|doac/, 'gift'], [/cuidado|beleza|salao/, 'spark'], [/equipamento/, 'laptop'], [/trabalho/, 'brief'],
];
function catIconKey(c) {
  for (const x of [c, c?.parent_id ? cat(c.parent_id) : null]) {
    if (!x) continue;
    const n = norm(x.name);
    const r = ICON_RULES.find(([re]) => re.test(n));
    if (r) return r[1];
  }
  return 'dots';
}
function chip(c, extra = '') {
  const root = c?.parent_id ? cat(c.parent_id) : c;
  const color = c?.color || root?.color || '#8a958f';
  return `<span class="cico ${extra}" style="--c:${esc(color)}">${svgP(P[c ? catIconKey(c) : 'dots'])}</span>`;
}
function txChip(t) {
  if (t.kind === 'transferencia') return `<span class="cico" style="--c:#2457a6">${svgP(P.swap)}</span>`;
  if (t.kind === 'pagamento_fatura') return `<span class="cico" style="--c:#0a6a4f">${svgP(P.card)}</span>`;
  return chip(cat(t.category_id));
}
function heroMoney(c) {
  const neg = c < 0, v = Math.abs(c || 0);
  const int = Math.floor(v / 100).toLocaleString('pt-BR'), cents = String(v % 100).padStart(2, '0');
  return `${neg ? '−' : ''}<span class="cur">R$</span>${int}<span class="cents">,${cents}</span>`;
}
function plastic(c, closedInv) {
  const label = closedInv ? (closedInv.state === 'atrasada' ? 'Fatura atrasada' : 'Fatura fechada') : 'Fatura atual';
  const val = closedInv ? closedInv.total_cents : c.current_cents, due = closedInv ? closedInv.due : c.current_due;
  const used = c.limit_cents ? Math.min(100, Math.max(0, -c.open_cents / c.limit_cents * 100)) : 0;
  return `<a class="plastic" href="#cartoes" onclick="S.card=${c.id};S.due=null" style="--c:${esc(c.color || '#334')}">
    <div class="p-top"><span class="p-name">${esc(c.name)}</span><span class="chip"></span></div>
    <div class="p-mid"><span>${label}${closedInv ? ' <i class="p-flag">pagar</i>' : ''}</span><b>${money(Math.abs(val || 0))}</b></div>
    <div><div class="p-bot"><span>Vence <b>${brShort(due)}</b></span><span>Disponível <b>${money(c.available_cents)}</b></span></div>
    <div class="limit" title="${Math.round(used)}% do limite em uso"><i style="width:${used}%"></i></div></div></a>`;
}
function donut(items, total, label) {
  const R = 70, C = 2 * Math.PI * R;
  let off = 0;
  const segs = items.map(it => {
    const len = Math.max(0, it.value / total * C);
    const s = `<circle r="${R}" cx="90" cy="90" stroke="${esc(it.color)}" stroke-dasharray="${Math.max(0, len - 2)} ${C}" stroke-dashoffset="${-off}"><title>${esc(it.name)}: ${money(-it.value)}</title></circle>`;
    off += len; return s;
  }).join('');
  return `<div class="donut"><svg viewBox="0 0 180 180"><circle r="${R}" cx="90" cy="90" stroke="var(--line-2)"/>${segs}</svg>
    <div class="center"><span>${esc(label)}</span><b>${money(total)}</b></div></div>`;
}

// ============================================================ roteamento
const PAGES = { inicio: pageInicio, lancamentos: pageLanc, cartoes: pageCartoes, fixas: pageFixas, relatorios: pageRel, config: pageConfig };
async function route() {
  let h = (location.hash || '#inicio').slice(1);
  if (h === 'novo') { history.replaceState(null, '', '#inicio'); h = 'inicio'; setTimeout(() => openTx(), 80); }
  let anchor = null;
  if (h === 'lembretes') { anchor = 'lembretes'; h = 'config'; }
  if (!PAGES[h]) h = 'inicio';
  $$('nav a').forEach(a => a.classList.toggle('active', a.dataset.page === h));
  try {
    await loadMeta();
    await PAGES[h]();
    const m = $('#main');
    if (route._last !== h) { m.classList.remove('enter'); void m.offsetWidth; m.classList.add('enter'); m.scrollTop = 0; }
    route._last = h;
    if (anchor) document.getElementById(anchor)?.scrollIntoView({ block: 'start' });
  } catch (e) {
    $('#main').innerHTML = `<div class="card empty">Não consegui carregar: ${esc(e.message)}<br><br><button class="btn" onclick="route()">Tentar de novo</button></div>`;
  }
}
const refresh = () => route();
window.addEventListener('hashchange', route);
// chamado pela bandeja, pelo painel e pelos avisos: abre uma tela ("novo", "inicio", "relatorios"…)
window.__abrir = r => {
  if ($('#dlg').open && r !== 'novo') $('#dlg').close();
  if (!r) return refresh();
  if (location.hash === '#' + r) route(); else location.hash = r;
};
$$('[data-win]').forEach(b => b.addEventListener('click', () => TAURI.core.invoke('window_control', { action: b.dataset.win })));
$('#btnNovo').addEventListener('click', () => openTx());
document.addEventListener('keydown', e => {
  if (e.key === 'n' && !e.ctrlKey && !e.metaKey && !['INPUT', 'TEXTAREA', 'SELECT'].includes(document.activeElement.tagName) && !$('#dlg').open) {
    e.preventDefault(); openTx();
  }
});

// ============================================================ visão geral
async function pageInicio() {
  const s = await api('GET', 'summary');
  const exp = s.month.by_category;
  const totalExp = -exp.reduce((a, c) => a + c.total, 0) || 1;
  const top = exp.slice(0, 6), rest = exp.slice(6).reduce((a, c) => a + c.total, 0);
  const pieItems = [...top.map(c => ({ name: c.name, color: c.color || '#8a958f', value: -c.total })), ...(rest ? [{ name: 'Outras', color: '#c7d0cb', value: -rest }] : [])];
  const cards = s.cards.filter(c => !c.archived);
  const closed = s.invoices.filter(i => i.state === 'fechada' || i.state === 'atrasada');
  const today = D(s.today);
  const hour = new Date().getHours();
  const hello = hour < 12 ? 'Bom dia' : hour < 18 ? 'Boa tarde' : 'Boa noite';
  $('#main').innerHTML = `
    <section class="hero">
      <div class="hero-top"><span class="hero-date">${hello}. ${cap(today.toLocaleDateString('pt-BR', { weekday: 'long', day: 'numeric', month: 'long' }))}</span>
        <button class="btn light sm" onclick="openTx()">${I.plus}Novo lançamento</button></div>
      <div class="hero-body">
        <div><div class="hero-label">Saldo em contas</div>
          <div class="hero-value">${heroMoney(s.balance_cents)}</div>
          <div class="hero-foot">Previsto para o fim do mês: <b>${money(s.forecast_cents)}</b></div></div>
        <div class="hero-tiles">
          <div class="tile warn"><span>Faturas a pagar</span><b>${money(Math.abs(s.invoices_cents))}</b><small>${closed.length ? `${closed.length} já fechada${closed.length > 1 ? 's' : ''}` : `${s.invoices.length} em aberto`}</small></div>
          <div class="tile"><span>Entrou em ${today.toLocaleDateString('pt-BR', { month: 'long' })}</span><b>${money(s.month.income_cents)}</b><small>receitas do mês</small></div>
          <div class="tile"><span>Saiu em ${today.toLocaleDateString('pt-BR', { month: 'long' })}</span><b>${money(Math.abs(s.month.expense_cents))}</b><small>sobra ${money(s.month.result_cents)}</small></div>
        </div>
      </div>
    </section>
    ${s.launched_today === 0 ? `<div class="nudge"><span class="ico">${svgP(P.bell)}</span>
      <p>Nada lançado hoje ainda<span>Registrar os gastos do dia leva um minuto e mantém o saldo confiável.</span></p>
      <button class="btn primary sm" onclick="openTx()">${I.plus}Lançar agora</button></div>` : ''}
    ${cards.length ? `<div class="plastics">${cards.map(c => plastic(c, s.invoices.find(i => i.card_id === c.id && ['fechada', 'atrasada'].includes(i.state)))).join('')}</div>` : ''}
    <div class="grid split">
      <div class="card"><div class="panel-head"><h2>Próximos 15 dias</h2><span class="hint">${s.upcoming.length} ${s.upcoming.length === 1 ? 'item' : 'itens'}</span></div><div class="list">
        ${s.upcoming.length ? s.upcoming.map(u => {
          const d = D(u.date);
          const ic = u.type === 'fatura' ? `<span class="cico" style="--c:${esc(u.color || '#0a6a4f')}">${svgP(P.card)}</span>` : chip(cat(u.category_id));
          return `<div class="row up">
            ${ic}
            <div style="min-width:0"><div class="t">${esc(u.description)}</div>
              <div class="s"><span class="${u.overdue ? 'neg' : ''}" style="font-weight:650">${u.overdue ? 'Venceu ' : ''}${d.toLocaleDateString('pt-BR', { day: '2-digit', month: 'short' }).replace('.', '')}</span><span>${esc(u.account)}</span>${u.overdue ? '<span class="pill late">atrasada</span>' : ''}${u.type === 'fatura' && u.state === 'fechada' ? '<span class="pill warn">fechada</span>' : ''}${u.type === 'fatura' && u.state === 'aberta' ? '<span class="pill info">aberta</span>' : ''}</div></div>
            <div class="money ${cls(u.amount_cents)}">${money(u.amount_cents)}</div>
            <div>${u.type === 'fatura'
              ? `<button class="btn sm" onclick="openPayInvoice(${u.card_id},'${u.date}',${u.amount_cents})">Pagar</button>`
              : `<button class="btn sm" onclick="togglePaid(${u.id},1)">${u.amount_cents < 0 ? 'Pagar' : 'Receber'}</button>`}</div>
          </div>`; }).join('') : '<div class="empty">Nada vencendo nos próximos 15 dias.</div>'}
      </div></div>
      <div class="grid" style="align-content:start">
        <div class="card"><div class="panel-head"><h2>Gastos de ${today.toLocaleDateString('pt-BR', { month: 'long' })}</h2><a href="#relatorios">Relatórios</a></div>
          ${exp.length ? `<div class="donut-wrap">${donut(pieItems, totalExp, 'gasto no mês')}
            <div class="list legend-list">${top.map(c => `<div class="row">${chip(cat(c.id), 'sm')}<div class="t" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(c.name)}</div><div class="money">${money(c.total).replace('-', '')}<span class="pct">${Math.round(-c.total / totalExp * 100)}%</span></div></div>`).join('')}</div></div>`
            : '<div class="empty">Sem despesas neste mês.</div>'}
        </div>
        <div class="card"><div class="panel-head"><h2>Contas</h2><a href="#config">Gerenciar</a></div><div class="list">
          ${s.accounts.filter(a => !a.archived).map(a => `<div class="row acc-row"><span class="cico sm round" style="--c:${esc(a.color || '#888')}">${svgP(P.bank)}</span><div class="t">${esc(a.name)}</div><div class="money ${cls(a.balance_cents)}">${money(a.balance_cents)}</div></div>`).join('')}
        </div></div>
      </div>
    </div>`;
}

async function togglePaid(id, paid) {
  await api('PUT', `transactions/${id}`, { paid });
  toast(paid ? 'Marcado como pago' : 'Marcado como pendente');
  refresh();
}

// ============================================================ lançamentos
async function pageLanc() {
  const start = iso(S.month), end = iso(lastDay(S.month));
  const qs = new URLSearchParams({ start, end, account: S.account, category: S.category, q: S.q });
  const r = await api('GET', 'transactions?' + qs);
  const allAcc = META.accounts.filter(a => !a.archived);
  const groups = {};
  for (const t of r.items) (groups[t.date] ||= []).push(t);
  const cats = [...META.categories.filter(c => c.kind === 'despesa'), ...META.categories.filter(c => c.kind === 'receita')];
  $('#main').innerHTML = `
    <h1>Lançamentos <span class="sub">${r.items.length} neste mês</span></h1>
    <div class="toolbar">
      <div class="monthnav"><button class="btn ghost icon" id="mPrev" aria-label="Mês anterior">${I.left}</button><span>${monthLabel(S.month)}</span><button class="btn ghost icon" id="mNext" aria-label="Próximo mês">${I.right}</button></div>
      <select id="fAcc"><option value="">Todas as contas e cartões</option>${accOptions(allAcc, +S.account)}</select>
      <select id="fCat"><option value="">Todas as categorias</option>${cats.map(c => `<option value="${c.id}" ${+S.category === c.id ? 'selected' : ''}>${c.parent_id ? '  ↳ ' : ''}${esc(c.name)}${c.kind === 'receita' ? ' (receita)' : ''}</option>`).join('')}</select>
      <input id="fQ" placeholder="Buscar descrição" value="${esc(S.q)}" style="flex:1;min-width:160px">
    </div>
    <div class="grid g4" style="margin-bottom:14px">
      <div class="card metric"><span class="cico round mi" style="--c:#14a37a">${svgP(P.trend)}</span><div class="label">Receitas</div><div class="value pos">${money(r.income_cents)}</div><div class="sub">${r.items.filter(x => x.kind === 'normal' && x.amount_cents > 0).length} lançamentos</div></div>
      <div class="card metric"><span class="cico round mi" style="--c:#d24b3c">${svgP(P.bag)}</span><div class="label">Despesas</div><div class="value neg">${money(r.expense_cents)}</div><div class="sub">${r.items.filter(x => x.kind === 'normal' && x.amount_cents < 0).length} lançamentos</div></div>
      <div class="card metric"><span class="cico round mi" style="--c:#2457a6">${svgP(P.coins)}</span><div class="label">Resultado</div><div class="value ${cls(r.result_cents)}">${money(r.result_cents)}</div><div class="sub">Sem transferências e pagamentos de fatura</div></div>
    </div>
    <div class="card" style="padding:14px 14px"><div class="list">
      ${r.items.length ? Object.keys(groups).map(d => `
        <div class="day"><span>${cap(D(d).toLocaleDateString('pt-BR', { weekday: 'long', day: '2-digit', month: 'long' }))}</span><span>${(() => { const v = groups[d].filter(x => x.kind === 'normal').reduce((a, x) => a + x.amount_cents, 0); return v ? money(v) : ''; })()}</span></div>
        ${groups[d].map(txRow).join('')}`).join('') : '<div class="empty">Nenhum lançamento neste mês com esses filtros.</div>'}
    </div></div>`;
  $('#mPrev').onclick = () => { S.month = addMonths(S.month, -1); refresh(); };
  $('#mNext').onclick = () => { S.month = addMonths(S.month, 1); refresh(); };
  $('#fAcc').onchange = e => { S.account = e.target.value; refresh(); };
  $('#fCat').onchange = e => { S.category = e.target.value; refresh(); };
  let t; $('#fQ').oninput = e => { clearTimeout(t); t = setTimeout(() => { S.q = e.target.value; refresh().then(() => { const q = $('#fQ'); q.focus(); q.setSelectionRange(q.value.length, q.value.length); }); }, 350); };
  $$('[data-tx]').forEach(el => el.addEventListener('click', ev => {
    if (ev.target.closest('button')) return;
    openTx(r.items.find(x => x.id === +el.dataset.tx));
  }));
  window._items = r.items;
}

function txRow(t) {
  const c = cat(t.category_id);
  const isCard = t.account_kind === 'cartao';
  const pills = [];
  if (t.inst_total) pills.push(`<span class="pill">${t.inst_index}/${t.inst_total}</span>`);
  if (t.recurrence_id) pills.push(`<span class="pill">fixa</span>`);
  if (isCard && t.invoice_due) pills.push(`<span class="pill blue">fatura ${brShort(t.invoice_due)}</span>`);
  if (t.kind === 'transferencia') pills.push(`<span class="pill info">${t.amount_cents < 0 ? 'para' : 'de'} ${esc(t.other_account || '')}</span>`);
  if (t.kind === 'pagamento_fatura') pills.push(`<span class="pill info">pagamento de fatura</span>`);
  const late = !isCard && !t.paid && t.date < META.today;
  if (late) pills.push(`<span class="pill late">atrasado</span>`);
  else if (!isCard && !t.paid) pills.push(`<span class="pill warn">pendente</span>`);
  const check = isCard
    ? `<span class="btn ghost icon" style="cursor:default;color:var(--faint)" title="Compra no cartão: entra na fatura">${I.card}</span>`
    : `<button class="check ${t.paid ? 'on' : ''}" title="${t.paid ? 'Pago, clique para marcar como pendente' : 'Marcar como pago'}" onclick="togglePaid(${t.id},${t.paid ? 0 : 1})" aria-label="Alternar pago">${I.check}</button>`;
  return `<div class="row tx" data-tx="${t.id}" tabindex="0">
    ${txChip(t)}
    <div style="min-width:0"><div class="t">${esc(t.description)}</div>
      <div class="s">${c ? `<span>${esc(c.name)}</span>` : ''}<span>${esc(t.account)}</span>${pills.join('')}</div></div>
    <div class="money ${cls(t.amount_cents)}" style="${!t.paid && !isCard ? 'opacity:.62' : ''}">${money(t.amount_cents)}</div>
    ${check}
    <div class="tools"><button class="btn ghost icon" onclick="delTx(${t.id})" aria-label="Excluir" title="Excluir">${I.trash}</button></div>
  </div>`;
}

async function delTx(id) {
  const t = (window._items || []).find(x => x.id === id);
  let scope = 'one';
  if (t && (t.series_id || t.recurrence_id)) {
    const r = await choose('Excluir lançamento', `"${t.description}" faz parte de ${t.series_id ? 'um parcelamento' : 'uma conta fixa'}.`,
      [['one', 'Só este'], ['future', 'Este e os próximos']]);
    if (!r) return;
    scope = r;
  } else if (!confirm('Excluir este lançamento?')) return;
  await api('DELETE', `transactions/${id}?scope=${scope}`);
  toast('Lançamento excluído');
  refresh();
}

// ============================================================ diálogos
function dialog(html, onSubmit) {
  const dlg = $('#dlg'), form = $('#dlgForm');
  form.innerHTML = html;
  form.onsubmit = async e => {
    e.preventDefault();
    const action = e.submitter?.value;
    if (action === 'cancel') return dlg.close();
    try {
      const keep = await onSubmit(action, form);
      if (keep !== false) dlg.close();
    } catch (err) { toast(err.message); }
  };
  $$('[data-close]', form).forEach(b => b.onclick = () => dlg.close());
  // Enter num campo salva (antes acionava o primeiro botão do formulário, que é o "Cancelar")
  form.onkeydown = e => {
    if (e.key !== 'Enter' || e.isComposing || !['INPUT'].includes(e.target.tagName) || e.target.type === 'checkbox') return;
    const save = $('button[value="save"]', form);
    if (save) { e.preventDefault(); form.requestSubmit(save); }
  };
  dlg.showModal();
  const f = $('[autofocus]', form); if (f) { f.focus(); f.select?.(); }
  return form;
}
function choose(title, text, options) {
  return new Promise(res => {
    dialog(`<div class="dlg-head"><h3>${esc(title)}</h3></div><p style="margin:0">${esc(text)}</p>
      <div class="dlg-foot"><button class="btn" value="cancel">Cancelar</button><span style="display:flex;gap:8px">${options.map(([v, l], i) => `<button class="btn ${i === options.length - 1 ? 'primary' : ''}" value="${v}">${esc(l)}</button>`).join('')}</span></div>`,
      async a => { res(a); });
    $('#dlg').addEventListener('close', () => res(null), { once: true });
  });
}

// ------------------------------------------------------------ lançamento (novo / editar)
function openTx(t) {
  const editing = !!t;
  let type = !t ? 'despesa' : t.kind === 'transferencia' ? 'transferencia' : t.amount_cents > 0 ? 'receita' : 'despesa';
  const contas = activeAccounts('conta'), all = META.accounts.filter(a => !a.archived);
  const lastAcc = +localStorage.getItem('ultimaConta') || contas[0]?.id;
  const form = dialog(`
    <div class="dlg-head"><h3>${editing ? 'Editar lançamento' : 'Novo lançamento'}</h3><button type="button" class="btn ghost icon" data-close aria-label="Fechar">${I.x}</button></div>
    ${editing ? '' : `<div class="seg" id="seg"><button type="button" data-t="despesa">Despesa</button><button type="button" data-t="receita">Receita</button><button type="button" data-t="transferencia">Transferência</button></div>`}
    <div class="form">
      <label class="field full">Valor<input class="amount-input" name="valor" inputmode="decimal" placeholder="0,00" required autofocus value="${t ? moneyInput(t.amount_cents) : ''}"></label>
      <label class="field full" id="fDesc">Descrição<input name="description" placeholder="Mercado, Uber, Salário…" value="${esc(t?.description || '')}" list="descList" autocomplete="off"></label>
      <label class="field">Data<input type="date" name="date" required value="${t?.date || META.today}"></label>
      <label class="field" id="fAccount"><span id="accLabel">Conta ou cartão</span><select name="account_id"></select></label>
      <label class="field" id="fTo" hidden>Para a conta<select name="to_id">${accOptions(contas)}</select></label>
      <label class="field" id="fCatF">Categoria<select name="category_id"></select></label>
      <label class="field" id="fInv" hidden>Fatura<select name="invoice_due"></select></label>
      <label class="field" id="fPaid" style="flex-direction:row;align-items:center;gap:8px;color:var(--text)"><input type="checkbox" name="paid" ${!t || t.paid ? 'checked' : ''}> <span id="paidLabel">Já foi pago</span></label>
      ${editing ? '' : `<label class="field" id="fRep">Repetição<select name="rep"><option value="">Não repete</option><option value="parc">Parcelado</option><option value="fixa">Conta fixa mensal</option></select></label>
      <label class="field" id="fN" hidden>Número de parcelas<input type="number" name="parcelas" min="2" max="72" value="2"></label>`}
      <label class="field full">Observação<input name="notes" value="${esc(t?.notes || '')}" placeholder="Opcional"></label>
      ${editing && (t.series_id || t.recurrence_id) ? `<label class="field full">Aplicar alteração em<select name="scope"><option value="one">Só este lançamento</option><option value="future">Este e os próximos ${t.series_id ? '(parcelas)' : '(conta fixa)'}</option></select></label>` : ''}
    </div>
    <datalist id="descList"></datalist>
    <div class="dlg-foot">
      ${editing ? `<button type="button" class="btn danger" id="btnDel">${I.trash}Excluir</button>` : '<span class="muted small" id="parcHint"></span>'}
      <span style="display:flex;gap:8px"><button class="btn" value="cancel">Cancelar</button><button class="btn primary" value="save">Salvar</button></span>
    </div>`, async (action, f) => {
    if (action !== 'save') return;
    const fd = Object.fromEntries(new FormData(f));
    let cents = parseMoney(fd.valor);
    if (!cents || isNaN(cents)) throw new Error('Informe um valor');
    cents = Math.abs(cents);
    if (type === 'transferencia') {
      if (editing) { await api('PUT', `transactions/${t.id}`, { date: fd.date, description: fd.description || 'Transferência', amount_cents: t.amount_cents < 0 ? -cents : cents }); }
      else await api('POST', 'transfers', { from_id: +fd.account_id, to_id: +fd.to_id, amount_cents: cents, date: fd.date, description: fd.description || 'Transferência' });
      toast('Transferência salva'); refresh(); return;
    }
    const accId = +fd.account_id;
    const body = {
      description: fd.description || (cat(+fd.category_id)?.name ?? (type === 'despesa' ? 'Despesa' : 'Receita')),
      amount_cents: type === 'despesa' ? -cents : cents, date: fd.date, account_id: accId,
      category_id: fd.category_id ? +fd.category_id : null, notes: fd.notes || null,
      paid: acc(accId).kind === 'cartao' ? 1 : (fd.paid ? 1 : 0),
    };
    if (acc(accId).kind === 'cartao' && fd.invoice_due) body.invoice_due = fd.invoice_due;
    localStorage.setItem('ultimaConta', accId);
    if (editing) {
      await api('PUT', `transactions/${t.id}?scope=${fd.scope || 'one'}`, body);
      toast('Lançamento atualizado');
    } else {
      if (fd.rep === 'parc') body.parcelas = +fd.parcelas;
      if (fd.rep === 'fixa') body.fixa = true;
      await api('POST', 'transactions', body);
      toast(fd.rep === 'parc' ? `${fd.parcelas} parcelas criadas` : 'Lançamento salvo');
    }
    refresh();
  });

  const sel = n => form.elements[n];
  let keepInv = true;
  function fillAccounts() {
    const list = type === 'despesa' ? all : contas;
    const cur = +sel('account_id').value || (t ? t.account_id : lastAcc);
    sel('account_id').innerHTML = accOptions(list, list.some(a => a.id === cur) ? cur : list[0]?.id);
    $('#accLabel', form).textContent = type === 'transferencia' ? 'Da conta' : type === 'despesa' ? 'Conta ou cartão' : 'Conta';
  }
  function fillInvoices() {
    const a = acc(+sel('account_id').value);
    const isCard = a?.kind === 'cartao';
    $('#fInv', form).hidden = !isCard;
    $('#fPaid', form).hidden = isCard || type === 'transferencia';
    if (!isCard) return;
    const d = D(sel('date').value || META.today);
    const due = invoiceDueFor(d, a.closing_day, a.due_day);
    const opts = [-1, 0, 1, 2].map(n => { const x = new Date(due.getFullYear(), due.getMonth() + n, 1); return new Date(x.getFullYear(), x.getMonth(), Math.min(a.due_day, lastDay(x).getDate())); });
    const cur = keepInv && t?.invoice_due && t.account_id === a.id ? t.invoice_due : iso(due);
    sel('invoice_due').innerHTML = opts.map(o => `<option value="${iso(o)}" ${iso(o) === cur ? 'selected' : ''}>Vence ${o.toLocaleDateString('pt-BR')}${iso(o) === iso(due) ? ' (pela data)' : ''}</option>`).join('');
  }
  function setType(nt) {
    type = nt;
    $$('#seg button', form).forEach(b => b.className = b.dataset.t === type ? 'on ' + (type === 'despesa' ? 'exp' : type === 'receita' ? 'inc' : '') : '');
    $('#fTo', form).hidden = type !== 'transferencia' || editing;
    $('#fAccount', form).hidden = type === 'transferencia' && editing;
    $('#fCatF', form).hidden = type === 'transferencia';
    if ($('#fRep', form)) $('#fRep', form).hidden = type === 'transferencia';
    $('#paidLabel', form).textContent = type === 'receita' ? 'Já foi recebido' : 'Já foi pago';
    if (type !== 'transferencia') sel('category_id').innerHTML = catOptions(type, t?.category_id);
    fillAccounts(); fillInvoices();
    if (type === 'transferencia' && sel('to_id').value === sel('account_id').value && contas.length > 1)
      sel('to_id').value = contas.find(c => c.id !== +sel('account_id').value).id;
  }
  $$('#seg button', form).forEach(b => b.onclick = () => setType(b.dataset.t));
  sel('account_id').onchange = () => { keepInv = false; fillInvoices(); };
  sel('date').onchange = () => {
    keepInv = false;
    fillInvoices();
    if (!editing && sel('date').value > META.today) sel('paid').checked = false;
  };
  if (!editing) {
    sel('rep').onchange = () => {
      $('#fN', form).hidden = sel('rep').value !== 'parc';
      updHint();
    };
    const updHint = () => {
      const c = parseMoney(sel('valor').value), n = +sel('parcelas').value;
      $('#parcHint', form).textContent = sel('rep').value === 'parc' && c > 0 && n > 1 ? `${n}x de ${money(Math.round(c / n))}` : '';
    };
    sel('valor').oninput = updHint; sel('parcelas').oninput = updHint;
    // sugestões de descrição + categoria automática pelo histórico
    api('GET', `transactions?start=${iso(addMonths(D(META.today), -3))}&end=${META.today}`).then(r => {
      const seen = new Map();
      for (const x of r.items) if (x.kind === 'normal' && !seen.has(x.description)) seen.set(x.description, x);
      $('#descList', form).innerHTML = [...seen.keys()].slice(0, 300).map(d => `<option value="${esc(d)}">`).join('');
      sel('description').onchange = () => {
        const m = seen.get(sel('description').value);
        if (m && !sel('category_id').value) {
          const k = m.amount_cents < 0 ? 'despesa' : 'receita';
          if (k === type) sel('category_id').value = m.category_id || '';
        }
      };
    }).catch(() => {});
  } else {
    $('#btnDel', form).onclick = () => { $('#dlg').close(); window._items = [t]; delTx(t.id); };
  }
  setType(type);
  if (t) sel('category_id').value = t.category_id || '';
}

// ============================================================ cartões
async function pageCartoes() {
  const cards = activeAccounts('cartao');
  if (!cards.length) { $('#main').innerHTML = `<h1>Cartões</h1><div class="card empty">Nenhum cartão cadastrado.<br><br><button class="btn primary" onclick="openAccount({kind:'cartao'})">${I.plus}Adicionar cartão</button></div>`; return; }
  if (!S.card || !cards.some(c => c.id === S.card)) S.card = cards[0].id;
  const card = acc(S.card);
  const { invoices } = await api('GET', `invoices?card=${card.id}`);
  const todayS = META.today;
  if (!S.due || !invoices.some(i => i.due === S.due)) {
    const pend = invoices.find(i => !i.paid && ['atrasada', 'fechada'].includes(i.state));
    S.due = (pend || invoices.find(i => i.state === 'aberta') || invoices[invoices.length - 1]).due;
  }
  const idx = invoices.findIndex(i => i.due === S.due);
  const shown = invoices.slice(Math.max(0, idx - 4), idx + 6);
  const inv = invoices[idx];
  const { items } = await api('GET', `invoices?card=${card.id}&due=${inv.due}`);
  const stPill = { paga: 'ok', atrasada: 'late', fechada: 'warn', aberta: 'info', futura: '' };
  $('#main').innerHTML = `
    <h1>Cartões</h1>
    <div class="cardpick">${cards.map(c => `<button class="plastic mini ${c.id === card.id ? 'on' : 'off'}" style="--c:${esc(c.color || '#334')}" onclick="S.card=${c.id};S.due=null;refresh()"><div class="p-top"><span class="p-name">${esc(c.name)}</span><span class="chip"></span></div><div class="p-mid"><span>Disponível</span><b>${money(c.available_cents)}</b></div></button>`).join('')}
      <button class="plastic mini add" onclick="openAccount({kind:'cartao'})">${I.plus} Novo cartão</button></div>
    <div class="inv-tabs">${shown.map(i => `<button class="${i.due === inv.due ? 'on' : ''}" onclick="S.due='${i.due}';refresh()">Vence ${cap(D(i.due).toLocaleDateString('pt-BR', { month: 'short' }).replace('.', ''))}/${i.due.slice(2, 4)}<b class="${i.paid ? '' : 'neg'}">${money(i.total_cents)}</b><span class="pill ${stPill[i.state]}">${i.state}</span></button>`).join('')}</div>
    <div class="grid" style="grid-template-columns:minmax(0,2fr) minmax(270px,1fr);align-items:start">
      <div class="card"><div class="inv-head">
          <div><div class="muted small" style="font-weight:600">Fatura com vencimento em ${br(inv.due)}, fecha em ${br(inv.closing)}</div><div class="big ${inv.paid ? '' : 'neg'}">${money(inv.total_cents)}</div></div>
          <div>${inv.paid
            ? `<span class="pill ok">Paga${inv.paid_at ? ' em ' + br(inv.paid_at) : ''}</span> ${inv.total_cents < 0 ? `<button class="btn sm" onclick="unpayInvoice(${card.id},'${inv.due}')">Reabrir</button>` : ''}`
            : inv.total_cents < 0 ? `<button class="btn primary" onclick="openPayInvoice(${card.id},'${inv.due}',${inv.total_cents})">Pagar fatura</button>` : ''}</div></div>
        <div class="list">${items.length ? items.map(t => ({ ...t, account: card.name, account_kind: 'cartao' })).map(txRow).join('') : '<div class="empty">Nenhuma compra nesta fatura.</div>'}</div>
      </div>
      <div class="card"><div class="panel-head"><h2>Limite</h2><span class="hint">${card.limit_cents ? Math.round(-card.open_cents / card.limit_cents * 100) : 0}% em uso</span></div>
        <div class="bar" style="height:10px;margin:0 0 16px"><i style="width:${card.limit_cents ? Math.min(100, -card.open_cents / card.limit_cents * 100) : 0}%;background:${esc(card.color || 'var(--em-600)')}"></i></div>
        <dl class="kv"><dt>Limite</dt><dd>${money(card.limit_cents)}</dd><dt>Em uso</dt><dd class="neg">${money(card.open_cents)}</dd><dt>Disponível</dt><dd>${money(card.available_cents)}</dd>
        <dt>Fecha no dia</dt><dd>${card.closing_day}</dd><dt>Vence no dia</dt><dd>${card.due_day}</dd><dt>Paga pela conta</dt><dd>${esc(acc(card.payment_account_id)?.name || '—')}</dd></dl>
        <div style="margin-top:14px;display:flex;gap:8px"><button class="btn sm" onclick="openAccount(acc(${card.id}))">${I.edit}Editar cartão</button></div>
        <p class="muted small" style="margin-bottom:0">Compras feitas até o dia ${card.closing_day} entram na fatura que fecha nesse dia; depois disso, vão para a próxima.</p>
      </div>
    </div>`;
  window._items = items;
  $$('[data-tx]').forEach(el => el.addEventListener('click', ev => {
    if (ev.target.closest('button')) return;
    const t = items.find(x => x.id === +el.dataset.tx); openTx({ ...t, account_kind: 'cartao' });
  }));
}

function openPayInvoice(cardId, due, total) {
  const card = acc(cardId);
  dialog(`<div class="dlg-head"><h3>Pagar fatura ${esc(card.name)}</h3><button type="button" class="btn ghost icon" data-close>${I.x}</button></div>
    <p class="muted" style="margin:0">Vencimento ${br(due)}. O valor sai da conta escolhida como "Pagamento de fatura".</p>
    <div class="form">
      <label class="field full">Valor<input class="amount-input" name="valor" inputmode="decimal" value="${moneyInput(total)}" autofocus></label>
      <label class="field">Pagar com<select name="account_id">${accOptions(activeAccounts('conta'), card.payment_account_id)}</select></label>
      <label class="field">Data do pagamento<input type="date" name="date" value="${META.today}"></label>
    </div>
    <div class="dlg-foot"><span></span><span style="display:flex;gap:8px"><button class="btn" value="cancel">Cancelar</button><button class="btn primary" value="save">Confirmar pagamento</button></span></div>`,
    async (a, f) => {
      if (a !== 'save') return;
      const fd = Object.fromEntries(new FormData(f));
      await api('POST', 'invoices/pay', { card_id: cardId, due, account_id: +fd.account_id, date: fd.date, amount_cents: Math.abs(parseMoney(fd.valor)) });
      toast('Fatura paga'); refresh();
    });
}
async function unpayInvoice(cardId, due) {
  if (!confirm('Reabrir esta fatura? O lançamento de pagamento na conta será removido.')) return;
  await api('POST', 'invoices/unpay', { card_id: cardId, due });
  toast('Fatura reaberta'); refresh();
}

// ============================================================ contas fixas
async function pageFixas() {
  const { items } = await api('GET', 'recurrences');
  const exp = items.filter(r => r.amount_cents < 0).reduce((a, r) => a + r.amount_cents, 0);
  const inc = items.filter(r => r.amount_cents > 0).reduce((a, r) => a + r.amount_cents, 0);
  $('#main').innerHTML = `
    <h1>Contas fixas <button class="btn primary sm" onclick="openRec()">${I.plus}Nova conta fixa</button></h1>
    <div class="grid g4" style="margin-bottom:16px">
      <div class="card metric"><div class="label">Despesas fixas por mês</div><div class="value neg">${money(exp)}</div></div>
      <div class="card metric"><div class="label">Receitas fixas por mês</div><div class="value pos">${money(inc)}</div></div>
      <div class="card metric"><div class="label">Sobra fixa</div><div class="value ${cls(inc + exp)}">${money(inc + exp)}</div><div class="sub">Antes dos gastos do dia a dia</div></div>
    </div>
    <div class="card" style="overflow-x:auto"><table>
      <thead><tr><th>Dia</th><th>Descrição</th><th>Categoria</th><th>Conta</th><th>Próxima</th><th class="money">Valor</th><th></th></tr></thead>
      <tbody>${items.map(r => `<tr>
        <td>${r.day}</td><td>${esc(r.description)}</td><td>${catDot(cat(r.category_id))} ${esc(r.category || '—')}</td><td>${esc(r.account)}</td>
        <td>${r.next ? `${brShort(r.next.date)} ${r.next.paid ? '<span class="pill ok">paga</span>' : r.next.date < META.today ? '<span class="pill late">atrasada</span>' : ''}` : '—'}</td>
        <td class="money ${cls(r.amount_cents)}">${money(r.amount_cents)}</td>
        <td style="white-space:nowrap"><button class="btn ghost icon" onclick='openRec(${JSON.stringify(r).replace(/'/g, "&#39;")})' aria-label="Editar">${I.edit}</button><button class="btn ghost icon" onclick="stopRec(${r.id})" aria-label="Encerrar">${I.trash}</button></td></tr>`).join('')}
      </tbody></table>${items.length ? '' : '<div class="empty">Nenhuma conta fixa.</div>'}</div>
    <p class="muted small">As contas fixas geram os lançamentos automaticamente até 2 meses à frente, como pendentes. Marque como pagas na Visão geral ou em Lançamentos.</p>`;
}
function openRec(r) {
  let type = r && r.amount_cents > 0 ? 'receita' : 'despesa';
  const form = dialog(`<div class="dlg-head"><h3>${r ? 'Editar' : 'Nova'} conta fixa</h3><button type="button" class="btn ghost icon" data-close>${I.x}</button></div>
    <div class="seg" id="seg"><button type="button" data-t="despesa">Despesa</button><button type="button" data-t="receita">Receita</button></div>
    <div class="form">
      <label class="field full">Valor mensal<input class="amount-input" name="valor" inputmode="decimal" required autofocus value="${r ? moneyInput(r.amount_cents) : ''}"></label>
      <label class="field full">Descrição<input name="description" required value="${esc(r?.description || '')}"></label>
      <label class="field">Conta ou cartão<select name="account_id">${accOptions(META.accounts.filter(a => !a.archived), r?.account_id)}</select></label>
      <label class="field">Dia do mês<input type="number" name="day" min="1" max="31" required value="${r?.day || D(META.today).getDate()}"></label>
      <label class="field full">Categoria<select name="category_id"></select></label>
    </div>
    ${r ? '<p class="muted small" style="margin:0">A alteração vale para os lançamentos ainda não pagos a partir de hoje.</p>' : ''}
    <div class="dlg-foot"><span></span><span style="display:flex;gap:8px"><button class="btn" value="cancel">Cancelar</button><button class="btn primary" value="save">Salvar</button></span></div>`,
    async (a, f) => {
      if (a !== 'save') return;
      const fd = Object.fromEntries(new FormData(f));
      const c = Math.abs(parseMoney(fd.valor));
      if (!c) throw new Error('Informe um valor');
      const body = { description: fd.description, amount_cents: type === 'despesa' ? -c : c, account_id: +fd.account_id, category_id: fd.category_id ? +fd.category_id : null, day: +fd.day };
      await api(r ? 'PUT' : 'POST', r ? `recurrences/${r.id}` : 'recurrences', body);
      toast('Conta fixa salva'); refresh();
    });
  const setType = t => { type = t; $$('#seg button', form).forEach(b => b.className = b.dataset.t === t ? 'on ' + (t === 'despesa' ? 'exp' : 'inc') : ''); form.elements.category_id.innerHTML = catOptions(t, r?.category_id); };
  $$('#seg button', form).forEach(b => b.onclick = () => setType(b.dataset.t));
  setType(type);
}
async function stopRec(id) {
  const r = await choose('Encerrar conta fixa', 'Ela deixa de gerar lançamentos. O que fazer com os lançamentos futuros ainda não pagos?', [['0', 'Manter'], ['1', 'Excluir']]);
  if (r === null) return;
  await api('DELETE', `recurrences/${id}?future=${r}`);
  toast('Conta fixa encerrada'); refresh();
}

// ============================================================ relatórios
async function pageRel() {
  const end = lastDay(S.repMonth), start = addMonths(S.repMonth, -5);
  const r = await api('GET', `transactions?start=${iso(start)}&end=${iso(end)}`);
  const months = [...Array(6)].map((_, i) => addMonths(start, i));
  const key = d => iso(d).slice(0, 7);
  const agg = Object.fromEntries(months.map(m => [key(m), { inc: 0, exp: 0 }]));
  const byCat = {}, prevCat = {};
  const curK = key(S.repMonth), prevK = key(addMonths(S.repMonth, -1));
  for (const t of r.items) {
    if (t.kind !== 'normal') continue;
    const k = t.date.slice(0, 7);
    if (!agg[k]) continue;
    if (t.amount_cents > 0) agg[k].inc += t.amount_cents; else agg[k].exp += t.amount_cents;
    if (t.amount_cents < 0) {
      const c = cat(t.category_id); const root = c?.parent_id ? cat(c.parent_id) : c;
      const id = root?.id || 0;
      if (k === curK) byCat[id] = (byCat[id] || 0) + t.amount_cents;
      if (k === prevK) prevCat[id] = (prevCat[id] || 0) + t.amount_cents;
    }
  }
  const max = Math.max(1, ...Object.values(agg).flatMap(v => [v.inc, -v.exp]));
  const rowsC = Object.entries(byCat).sort((a, b) => a[1] - b[1]);
  const tot = rowsC.reduce((a, [, v]) => a + v, 0) || -1;
  const step = Math.pow(10, Math.floor(Math.log10(max))) / 2;
  const top = Math.ceil(max / step) * step;
  const ticks = [4, 3, 2, 1, 0].map(i => top * i / 4);
  const short = c => c >= 100000 ? `${(c / 100000).toLocaleString('pt-BR', { maximumFractionDigits: 1 })} mil` : Math.round(c / 100).toLocaleString('pt-BR');
  const pie = rowsC.slice(0, 7).map(([id, v]) => { const c = cat(+id); return { name: c?.name || 'Sem categoria', color: c?.color || '#8a958f', value: -v }; });
  const restV = rowsC.slice(7).reduce((a, [, v]) => a - v, 0);
  if (restV) pie.push({ name: 'Outras', color: '#c7d0cb', value: restV });
  const cur = agg[curK];
  $('#main').innerHTML = `
    <h1>Relatórios</h1>
    <div class="toolbar"><div class="monthnav"><button class="btn ghost icon" id="rPrev" aria-label="Mês anterior">${I.left}</button><span>${monthLabel(S.repMonth)}</span><button class="btn ghost icon" id="rNext" aria-label="Próximo mês">${I.right}</button></div></div>
    <div class="grid g4" style="margin-bottom:18px">
      <div class="card metric"><span class="cico round mi" style="--c:#14a37a">${svgP(P.trend)}</span><div class="label">Receitas em ${S.repMonth.toLocaleDateString('pt-BR', { month: 'long' })}</div><div class="value pos">${money(cur.inc)}</div></div>
      <div class="card metric"><span class="cico round mi" style="--c:#d24b3c">${svgP(P.bag)}</span><div class="label">Despesas</div><div class="value neg">${money(cur.exp)}</div></div>
      <div class="card metric"><span class="cico round mi" style="--c:#2457a6">${svgP(P.coins)}</span><div class="label">Resultado</div><div class="value ${cls(cur.inc + cur.exp)}">${money(cur.inc + cur.exp)}</div><div class="sub">${cur.inc ? `${Math.round((cur.inc + cur.exp) / cur.inc * 100)}% da receita` : ''}</div></div>
    </div>
    <div class="grid split">
      <div class="card"><div class="panel-head"><h2>Receitas e despesas, 6 meses</h2><div class="legend"><span><i class="dot" style="background:var(--em-500)"></i>Receitas</span><span><i class="dot" style="background:#d24b3c"></i>Despesas</span></div></div>
        <div class="chart"><div class="axis">${ticks.map(t => `<span>${short(t)}</span>`).join('')}</div>
          <div class="plot">${months.map(m => { const v = agg[key(m)]; return `<div class="col ${key(m) === curK ? 'cur' : ''}" title="${monthLabel(m)}: receitas ${money(v.inc)}, despesas ${money(v.exp)}"><div class="bars"><i class="inc" style="height:${v.inc / top * 100}%"></i><i class="exp" style="height:${-v.exp / top * 100}%"></i></div><span class="lbl">${m.toLocaleDateString('pt-BR', { month: 'short' }).replace('.', '')}</span></div>`; }).join('')}</div></div>
        <table style="margin-top:22px"><thead><tr><th>Mês</th><th class="money">Receitas</th><th class="money">Despesas</th><th class="money">Resultado</th></tr></thead><tbody>
          ${months.map(m => { const v = agg[key(m)]; return `<tr><td style="font-weight:600">${monthLabel(m)}</td><td class="money pos">${money(v.inc)}</td><td class="money neg">${money(v.exp)}</td><td class="money ${cls(v.inc + v.exp)}">${money(v.inc + v.exp)}</td></tr>`; }).join('')}
        </tbody></table>
      </div>
      <div class="card"><div class="panel-head"><h2>Para onde foi o dinheiro</h2></div>
        ${pie.length ? `<div style="display:grid;place-items:center;margin:4px 0 8px">${donut(pie, pie.reduce((a, x) => a + x.value, 0), 'despesas')}</div>` : ''}
        <table><thead><tr><th>Categoria</th><th class="money">Valor</th><th class="money">vs. mês anterior</th></tr></thead><tbody>
        ${rowsC.map(([id, v]) => { const c = cat(+id); const p = prevCat[id] || 0; const diff = p ? Math.round((v / p - 1) * 100) : null;
          return `<tr><td><span class="tdcat">${c ? chip(c, 'sm') : chip(null, 'sm')}<span>${esc(c?.name || 'Sem categoria')}<br><span class="muted small">${Math.round(v / tot * 100)}% do mês</span></span></span></td><td class="money">${money(v).replace('-', '')}</td><td class="money">${diff === null ? '<span class="muted small">novo</span>' : `<span class="pill ${diff > 10 ? 'late' : diff < -10 ? 'ok' : ''}">${diff > 0 ? '+' : ''}${diff}%</span>`}</td></tr>`; }).join('') || '<tr><td colspan="3" class="empty">Sem despesas.</td></tr>'}
        </tbody></table></div>
    </div>`;
  $('#rPrev').onclick = () => { S.repMonth = addMonths(S.repMonth, -1); refresh(); };
  $('#rNext').onclick = () => { S.repMonth = addMonths(S.repMonth, 1); refresh(); };
}

// ============================================================ configurações
async function pageConfig() {
  const st = META.settings;
  const contas = META.accounts.filter(a => a.kind === 'conta'), cards = META.accounts.filter(a => a.kind === 'cartao');
  const catBlock = kind => {
    const cs = META.categories.filter(c => c.kind === kind), roots = cs.filter(c => !c.parent_id).sort((a, b) => a.name.localeCompare(b.name, 'pt-BR'));
    return roots.map(r => `<div class="row" style="grid-template-columns:1fr auto"><div class="t" style="display:flex;align-items:center;gap:10px">${chip(r, 'sm')}${esc(r.name)}</div><span><button class="btn ghost icon" onclick="openCat(cat(${r.id}))">${I.edit}</button></span></div>
      ${cs.filter(c => c.parent_id === r.id).map(s => `<div class="row" style="grid-template-columns:1fr auto;padding-left:46px"><div class="muted">${esc(s.name)}</div><span><button class="btn ghost icon" onclick="openCat(cat(${s.id}))">${I.edit}</button></span></div>`).join('')}`).join('');
  };
  $('#main').innerHTML = `
    <h1>Configurações</h1>
    <div class="grid settings-grid">
      <div class="card"><div class="panel-head"><h2>Contas</h2></div><div class="list">${contas.map(a => `<div class="row" style="grid-template-columns:1fr auto auto"><div class="t" style="display:flex;align-items:center;gap:10px;${a.archived ? 'opacity:.5' : ''}"><span class="cico sm round" style="--c:${esc(a.color || '#888')}">${svgP(P.bank)}</span>${esc(a.name)}${a.archived ? ' (arquivada)' : ''}</div><div class="money">${money(a.balance_cents)}</div><button class="btn ghost icon" onclick="openAccount(acc(${a.id}))">${I.edit}</button></div>`).join('')}</div>
        <button class="btn sm" style="margin-top:10px" onclick="openAccount({kind:'conta'})">${I.plus}Nova conta</button>
        <h2 style="margin:22px 0 8px">Cartões</h2><div class="list">${cards.map(a => `<div class="row" style="grid-template-columns:1fr auto auto"><div class="t" style="display:flex;align-items:center;gap:10px;${a.archived ? 'opacity:.5' : ''}"><span class="cico sm" style="--c:${esc(a.color || '#888')}">${svgP(P.card)}</span><span>${esc(a.name)}<br><span class="muted small" style="font-weight:500">Fecha dia ${a.closing_day}, vence dia ${a.due_day}</span></span></div><div class="money">${money(a.limit_cents)}</div><button class="btn ghost icon" onclick="openAccount(acc(${a.id}))">${I.edit}</button></div>`).join('')}</div>
        <button class="btn sm" style="margin-top:10px" onclick="openAccount({kind:'cartao'})">${I.plus}Novo cartão</button>
      </div>
      <div class="card" id="lembretes"><div class="panel-head"><h2>Lembretes e avisos</h2></div>
        <form id="remForm">
          <div class="opt"><label class="switch"><input type="checkbox" name="lembrete_diario_ativo" ${st.lembrete_diario_ativo === '1' ? 'checked' : ''}><i></i></label><div class="txt"><b>Lembrete diário</b><span>Pergunta se você já lançou os gastos do dia.</span></div><input type="time" name="lembrete_diario" value="${esc(st.lembrete_diario)}"></div>
          <div class="opt"><span class="switch" style="visibility:hidden"></span><div class="txt"><b>Contas a vencer</b><span>Avisa uma vez por dia o que vence nos próximos
            <select name="aviso_contas_dias" style="width:auto;padding:2px 26px 2px 8px;margin:0 2px">${[0, 1, 2, 3, 5, 7].map(n => `<option value="${n}" ${+st.aviso_contas_dias === n ? 'selected' : ''}>${n}</option>`).join('')}</select> dias.</span></div><input type="time" name="aviso_contas_hora" value="${esc(st.aviso_contas_hora)}"></div>
          <div class="opt"><label class="switch"><input type="checkbox" name="revisao_semanal_ativo" ${st.revisao_semanal_ativo === '1' ? 'checked' : ''}><i></i></label><div class="txt"><b>Revisão de domingo</b><span>Resumo da semana: saldo, faturas e gastos do mês.</span></div><input type="time" name="revisao_semanal_hora" value="${esc(st.revisao_semanal_hora)}"></div>
          <h2 style="margin:18px 0 4px">Como o aviso aparece</h2>
          <div class="opt"><span class="switch" style="visibility:hidden"></span><div class="txt"><b>Estilo</b><span>O aviso do Finanças tem botões como "Lançar agora" e "Mais tarde".</span></div>
            <select name="aviso_estilo"><option value="proprio" ${st.aviso_estilo !== 'sistema' ? 'selected' : ''}>Do Finanças</option><option value="sistema" ${st.aviso_estilo === 'sistema' ? 'selected' : ''}>Do sistema</option></select></div>
          <div class="opt"><label class="switch"><input type="checkbox" name="tremer_tela" ${st.tremer_tela !== '0' ? 'checked' : ''}><i></i></label><div class="txt"><b>Tremer a tela</b><span>A tela inteira treme por meio segundo quando chega um aviso.</span></div>
            <select name="tremor_intensidade"><option value="suave" ${st.tremor_intensidade === 'suave' ? 'selected' : ''}>Suave</option><option value="medio" ${!['suave', 'forte'].includes(st.tremor_intensidade) ? 'selected' : ''}>Médio</option><option value="forte" ${st.tremor_intensidade === 'forte' ? 'selected' : ''}>Forte</option></select></div>
          <div class="opt"><label class="switch"><input type="checkbox" name="som_aviso" ${st.som_aviso !== '0' ? 'checked' : ''}><i></i></label><div class="txt"><b>Som</b><span>Toca o som de aviso do sistema.</span></div></div>
          ${TAURI ? `<div class="opt"><label class="switch"><input type="checkbox" name="iniciar_com_sistema" ${st.iniciar_com_sistema !== '0' ? 'checked' : ''}><i></i></label><div class="txt"><b>Iniciar com o computador</b><span>O Finanças liga junto com o sistema, em segundo plano, para os lembretes chegarem na hora.</span></div></div>` : ''}
          <div class="opt" style="display:block"><div class="txt"><b>Celular (ntfy)</b><span>Instale o app ntfy (grátis), toque em "+" e assine o mesmo tópico. Use um nome difícil de adivinhar: quem souber o nome lê as mensagens.</span></div>
            <input name="ntfy_topico" placeholder="ex.: financas-fernando-8k2q" value="${esc(st.ntfy_topico)}" style="width:100%;margin-top:10px"></div>
          <div style="display:flex;gap:8px;margin-top:14px"><button class="btn primary" type="submit">Salvar</button><button class="btn" type="button" id="btnTest">${svgP(P.bell)}Testar aviso</button></div>
        </form>
      </div>
      <div class="card"><div class="panel-head"><h2>Categorias de despesa</h2></div><div class="list">${catBlock('despesa')}</div><button class="btn sm" style="margin-top:10px" onclick="openCat({kind:'despesa'})">${I.plus}Nova categoria</button></div>
      <div class="card"><div class="panel-head"><h2>Categorias de receita</h2></div><div class="list">${catBlock('receita')}</div><button class="btn sm" style="margin-top:10px" onclick="openCat({kind:'receita'})">${I.plus}Nova categoria</button></div>
      <div class="card"><div class="panel-head"><h2>Backup</h2></div>
        <p style="margin-top:0">Seus dados ficam só neste computador, em:<br><code class="small">${esc(META.data_dir)}</code></p>
        <p class="muted small">Uma cópia automática é salva todo dia na pasta <code>backups</code> (mantém as últimas 15). Para levar ao Ubuntu, exporte aqui e importe lá.</p>
        <div style="display:flex;gap:8px;flex-wrap:wrap"><button class="btn primary" id="btnExport">Exportar backup (.json)</button><label class="btn">Importar backup<input type="file" accept=".json,application/json" id="impFile" hidden></label></div>
      </div>
    </div>`;
  $('#remForm').onsubmit = async e => {
    e.preventDefault();
    const f = e.target, d = Object.fromEntries(new FormData(f));
    d.lembrete_diario_ativo = f.elements.lembrete_diario_ativo.checked ? '1' : '0';
    d.revisao_semanal_ativo = f.elements.revisao_semanal_ativo.checked ? '1' : '0';
    d.tremer_tela = f.elements.tremer_tela.checked ? '1' : '0';
    d.som_aviso = f.elements.som_aviso.checked ? '1' : '0';
    if (f.elements.iniciar_com_sistema) d.iniciar_com_sistema = f.elements.iniciar_com_sistema.checked ? '1' : '0';
    await api('POST', 'settings', d); toast('Lembretes salvos');
  };
  $('#btnTest').onclick = async () => { $('#remForm').requestSubmit(); setTimeout(async () => { await api('POST', 'notify-test'); toast('Aviso de teste enviado'); }, 300); };
  $('#btnExport').onclick = async () => {
    if (TAURI) {
      try { const p = await TAURI.core.invoke('export_backup'); if (p) toast('Backup salvo em ' + p); }
      catch (e) { toast(String(e)); }
      return;
    }
    const data = await api('GET', 'backup');
    const a = document.createElement('a');
    a.href = URL.createObjectURL(new Blob([JSON.stringify(data, null, 1)], { type: 'application/json' }));
    a.download = `financas-backup-${META.today}.json`; a.click();
  };
  $('#impFile').onchange = async e => {
    const file = e.target.files[0]; if (!file) return;
    const data = JSON.parse(await file.text());
    if (!confirm(`Importar este backup? Isso SUBSTITUI todos os dados atuais (${(data.transactions || []).length} lançamentos no arquivo).`)) return;
    await api('POST', 'backup', data); toast('Backup importado'); refresh();
  };
}

function openAccount(a) {
  const isCard = a.kind === 'cartao', editing = !!a.id;
  dialog(`<div class="dlg-head"><h3>${editing ? 'Editar' : 'Novo'} ${isCard ? 'cartão' : 'conta'}</h3><button type="button" class="btn ghost icon" data-close>${I.x}</button></div>
    <div class="form">
      <label class="field full">Nome<input name="name" required autofocus value="${esc(a.name || '')}"></label>
      ${isCard ? `
        <label class="field">Limite<input name="limite" inputmode="decimal" value="${a.limit_cents ? moneyInput(a.limit_cents) : ''}"></label>
        <label class="field">Paga pela conta<select name="payment_account_id">${accOptions(activeAccounts('conta'), a.payment_account_id)}</select></label>
        <label class="field">Dia de fechamento<input type="number" min="1" max="31" name="closing_day" required value="${a.closing_day || ''}"></label>
        <label class="field">Dia de vencimento<input type="number" min="1" max="31" name="due_day" required value="${a.due_day || ''}"></label>`
      : `<label class="field full">Saldo atual<input name="saldo" inputmode="decimal" value="${editing ? (a.balance_cents < 0 ? '-' : '') + moneyInput(a.balance_cents) : '0,00'}"><span class="small">Alterar aqui ajusta o saldo inicial para a conta mostrar esse valor hoje.</span></label>`}
      <label class="field">Cor<input type="color" name="color" value="${esc(a.color || '#0f6e56')}"></label>
    </div>
    <div class="dlg-foot">${editing ? `<button type="button" class="btn danger" id="btnArch">${I.trash}Excluir</button>` : '<span></span>'}
      <span style="display:flex;gap:8px"><button class="btn" value="cancel">Cancelar</button><button class="btn primary" value="save">Salvar</button></span></div>`,
    async (act, f) => {
      if (act !== 'save') return;
      const fd = Object.fromEntries(new FormData(f));
      const body = { name: fd.name, kind: a.kind, color: fd.color, archived: 0 };
      if (isCard) Object.assign(body, { limit_cents: parseMoney(fd.limite) || 0, closing_day: +fd.closing_day, due_day: +fd.due_day, payment_account_id: +fd.payment_account_id || null });
      else { const v = parseMoney(fd.saldo); if (!isNaN(v)) body.balance_cents = v; }
      await api(editing ? 'PUT' : 'POST', editing ? `accounts/${a.id}` : 'accounts', body);
      toast('Salvo'); refresh();
    });
  if (editing) $('#btnArch').onclick = async () => {
    if (!confirm(`Excluir "${a.name}"? Se houver lançamentos, ela será apenas arquivada.`)) return;
    const r = await api('DELETE', `accounts/${a.id}`); $('#dlg').close(); toast(r.resultado === 'arquivada' ? 'Arquivada (tem lançamentos)' : 'Excluída'); refresh();
  };
}

function openCat(c) {
  const editing = !!c.id;
  const roots = META.categories.filter(x => x.kind === c.kind && !x.parent_id && x.id !== c.id);
  dialog(`<div class="dlg-head"><h3>${editing ? 'Editar' : 'Nova'} categoria de ${c.kind}</h3><button type="button" class="btn ghost icon" data-close>${I.x}</button></div>
    <div class="form">
      <label class="field full">Nome<input name="name" required autofocus value="${esc(c.name || '')}"></label>
      <label class="field">Subcategoria de<select name="parent_id"><option value="">— nenhuma (principal) —</option>${roots.map(r => `<option value="${r.id}" ${r.id === c.parent_id ? 'selected' : ''}>${esc(r.name)}</option>`).join('')}</select></label>
      <label class="field">Cor<input type="color" name="color" value="${esc(c.color || '#888888')}"></label>
    </div>
    <div class="dlg-foot">${editing ? `<button type="button" class="btn danger" id="btnDelCat">${I.trash}Excluir</button>` : '<span></span>'}
      <span style="display:flex;gap:8px"><button class="btn" value="cancel">Cancelar</button><button class="btn primary" value="save">Salvar</button></span></div>`,
    async (act, f) => {
      if (act !== 'save') return;
      const fd = Object.fromEntries(new FormData(f));
      await api(editing ? 'PUT' : 'POST', editing ? `categories/${c.id}` : 'categories', { name: fd.name, kind: c.kind, parent_id: fd.parent_id ? +fd.parent_id : null, color: fd.color });
      toast('Categoria salva'); refresh();
    });
  if (editing) $('#btnDelCat').onclick = async () => {
    if (!confirm(`Excluir "${c.name}"?`)) return;
    const r = await api('DELETE', `categories/${c.id}`); $('#dlg').close(); toast(r.resultado === 'arquivada' ? 'Arquivada (tem lançamentos)' : 'Excluída'); refresh();
  };
}

// ============================================================ início
route();
