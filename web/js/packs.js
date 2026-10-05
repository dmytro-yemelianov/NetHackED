// Pack manager: list, upload, inspect, diff, delete, play.
import { loadWasm } from './wasm.js';
import { listPacks, openPack, getSources, savePack, deletePack, ACTIVE_KEY } from './pack-store.js';

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

export function downloadBytes(name, bytes, mime = 'application/json') {
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = Object.assign(document.createElement('a'), { href: url, download: name });
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export async function filesFromInput(fileList) {
  const out = {};
  for (const f of fileList) {
    const path = f.webkitRelativePath || f.name;
    if (path.endsWith('.toml')) out[path] = await f.text();
  }
  return out;
}

const state = { wasm: null, entries: [], key: null, pack: null, vanilla: null, sources: null };
export const packsState = state;

function showError(msg) {
  $('pack-error').textContent = msg || '';
}

async function refreshList(selectKey) {
  state.entries = await listPacks();
  $('pack-list').innerHTML = state.entries
    .map((e) => `<li><button data-key="${esc(e.key)}" class="${e.key === selectKey ? 'active' : ''}">
        ${esc(e.title)} <small>${esc(e.version)}</small>
        <span class="badge">${esc(e.origin)}</span></button></li>`)
    .join('');
  if (selectKey) await select(selectKey);
}

async function select(key) {
  showError('');
  try {
    state.key = key;
    state.pack = await openPack(state.wasm, key);
    const entry = state.entries.find((e) => e.key === key);
    state.sources = entry?.hasSources ? await getSources(key) : null;
    for (const b of document.querySelectorAll('#pack-list button')) b.classList.toggle('active', b.dataset.key === key);
    render();
  } catch (e) {
    showError(String(e.message || e));
  }
}

export function render() {
  const p = state.pack;
  if (!p) return;
  const m = JSON.parse(p.manifestJson());
  const rs = JSON.parse(p.rulesetJson());
  const diff = JSON.parse(p.diffVsVanillaJson());
  const report = JSON.parse(p.reportJson());
  const entry = state.entries.find((e) => e.key === state.key);
  const changed = new Set(diff.map((d) => `${d.table}/${d.entry}`));

  $('tab-overview').innerHTML = `
    <h2>${esc(m.name || m.id)} <small>${esc(m.version)}</small></h2>
    <p>${esc(m.description || '')}</p>
    <dl class="kv">
      <dt>id</dt><dd>${esc(m.id)}</dd>
      <dt>hash</dt><dd><code>${esc(p.hash())}</code></dd>
      <dt>entries</dt><dd>${rs.monsters.length} monsters · ${rs.items.length} items · ${rs.roles.length} roles</dd>
      <dt>vs vanilla</dt><dd>${diff.length} changed field(s)</dd>
      <dt>validation</dt><dd>${report.errors.length} error(s), ${report.warnings.length} warning(s)</dd>
    </dl>
    <div class="actions">
      ${state.key === 'new' ? '<em>Unsaved: use Edit → Save to store it, then play.</em>' : '<button id="act-play">Play with this pack</button>'}
      <button id="act-download">Download .nrpack</button>
      ${entry?.origin === 'stored' ? '<button id="act-delete" class="danger">Delete</button>' : ''}
    </div>`;
  if ($('act-play')) $('act-play').onclick = () => { localStorage.setItem(ACTIVE_KEY, state.key); location.href = 'index.html'; };
  $('act-download').onclick = () => downloadBytes(`${m.id}-${m.version}.nrpack`, p.nrpackBytes());
  const del = $('act-delete');
  if (del) del.onclick = async () => { await deletePack(state.key); await refreshList('vanilla'); };

  renderTable('tab-monsters', 'monsters', rs.monsters, ['name', 'glyph', 'level', 'speed', 'ac', 'base_hp', 'alignment', 'size'], changed);
  renderTable('tab-items', 'items', rs.items, ['name', 'class', 'cost', 'weight', 'damage_small', 'damage_large', 'ac_bonus', 'nutrition'], changed);
  renderTable('tab-roles', 'roles', rs.roles, ['name', 'base_hp', 'ac', 'speed', 'default_alignment', 'initial_alignment_record'], changed);

  $('tab-diff').innerHTML = diff.length
    ? `<table class="grid"><thead><tr><th>table</th><th>entry</th><th>field</th><th>before</th><th>after</th></tr></thead><tbody>${
        diff.map((d) => `<tr><td>${esc(d.table)}</td><td>${esc(d.entry)}</td><td>${esc(d.field)}</td><td><code>${esc(d.before)}</code></td><td><code>${esc(d.after)}</code></td></tr>`).join('')
      }</tbody></table>`
    : '<p>Identical to vanilla.</p>';

  const diagRow = (sev) => (d) => `<tr class="${sev}"><td>${sev}</td><td>${esc(d.file)}</td><td>${esc(d.entry)}${d.field ? '.' + esc(d.field) : ''}</td><td>${esc(d.message)}</td></tr>`;
  $('tab-diagnostics').innerHTML = report.errors.length + report.warnings.length
    ? `<table class="grid"><tbody>${report.errors.map(diagRow('error')).join('')}${report.warnings.map(diagRow('warning')).join('')}</tbody></table>`
    : '<p>No errors or warnings.</p>';

  $('edit-tab-btn').hidden = !(state.sources || state.key === 'new');
  document.dispatchEvent(new CustomEvent('pack-selected'));
}

function renderTable(panelId, table, rows, cols, changed) {
  const panel = $(panelId);
  panel.innerHTML = `<input type="search" placeholder="Filter…" class="filter">
    <table class="grid"><thead><tr>${cols.map((c) => `<th data-col="${c}">${c}</th>`).join('')}</tr></thead><tbody></tbody></table>`;
  const tbody = panel.querySelector('tbody');
  let sortCol = 'name', dir = 1, filter = '';
  const fmt = (v) => (v && typeof v === 'object' ? JSON.stringify(v) : v);
  const draw = () => {
    const f = filter.toLowerCase();
    tbody.innerHTML = rows
      .filter((r) => !f || String(r.name).toLowerCase().includes(f))
      .sort((a, b) => (a[sortCol] > b[sortCol] ? dir : a[sortCol] < b[sortCol] ? -dir : 0))
      .map((r) => `<tr class="${changed.has(`${table}/${r.name}`) ? 'changed' : ''}">${cols.map((c) => `<td>${esc(fmt(r[c]))}</td>`).join('')}</tr>`)
      .join('');
  };
  panel.querySelector('.filter').oninput = (e) => { filter = e.target.value; draw(); };
  for (const th of panel.querySelectorAll('th')) th.onclick = () => { dir = sortCol === th.dataset.col ? -dir : 1; sortCol = th.dataset.col; draw(); };
  draw();
}

async function importFiles(files) {
  showError('');
  if (!Object.keys(files).length) return showError('No .toml files found.');
  try {
    const pack = state.wasm.WasmPack.fromFiles(JSON.stringify(files));
    const entry = await savePack(state.wasm, { bytes: pack.nrpackBytes(), sources: files });
    await refreshList(entry.key);
  } catch (e) {
    showError(String(e.message || e));
  }
}

async function importNrpack(file) {
  showError('');
  try {
    const entry = await savePack(state.wasm, { bytes: new Uint8Array(await file.arrayBuffer()) });
    await refreshList(entry.key);
  } catch (e) {
    showError(`${file.name}: ${e.message || e}`);
  }
}

async function main() {
  state.wasm = await loadWasm();
  state.vanilla = state.wasm.WasmPack.vanilla();

  for (const btn of document.querySelectorAll('.tabs button')) {
    btn.onclick = () => {
      for (const b of document.querySelectorAll('.tabs button')) b.classList.toggle('active', b === btn);
      for (const panel of document.querySelectorAll('.tab-panel')) panel.hidden = panel.id !== `tab-${btn.dataset.tab}`;
    };
  }
  $('pack-list').onclick = (e) => { const b = e.target.closest('button[data-key]'); if (b) select(b.dataset.key); };
  $('upload-nrpack').onchange = (e) => e.target.files[0] && importNrpack(e.target.files[0]);
  $('upload-files').onchange = async (e) => importFiles(await filesFromInput(e.target.files));
  $('upload-folder').onchange = async (e) => importFiles(await filesFromInput(e.target.files));
  const dz = $('drop-zone');
  dz.ondragover = (e) => { e.preventDefault(); dz.classList.add('over'); };
  dz.ondragleave = () => dz.classList.remove('over');
  dz.ondrop = async (e) => {
    e.preventDefault();
    dz.classList.remove('over');
    const list = [...e.dataTransfer.files];
    const nr = list.find((f) => f.name.endsWith('.nrpack'));
    if (nr) return importNrpack(nr);
    importFiles(await filesFromInput(list));
  };

  $('new-from-vanilla').onclick = async () => {
    const files = JSON.parse(state.wasm.vanillaPackFilesJson());
    // Start from an empty patch set on top of vanilla, not a full dump.
    files['pack.toml'] = 'id = "my-pack"\nname = "My Pack"\nversion = "0.1.0"\nbase = "vanilla"\ndescription = "Created in the NetRust web pack editor"\n';
    files['monsters.toml'] = '# [[monster]]\n# name = "jackal"\n# level = 3\n';
    files['items.toml'] = '# [[item]]\n# name = "leather armor"\n# cost = 10\n';
    files['roles.toml'] = '# [[role]]\n# name = "Valkyrie"\n# base_hp = 20\n';
    state.key = 'new';
    state.sources = files;
    state.pack = state.wasm.WasmPack.fromFiles(JSON.stringify(files));
    render();
    document.querySelector('.tabs button[data-tab="edit"]').click();
  };

  const params = new URLSearchParams(location.search);
  await refreshList(params.get('pack') || localStorage.getItem(ACTIVE_KEY) || 'vanilla');
}

main().catch((e) => showError(String(e.message || e)));
