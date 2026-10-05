// Rule pack editor: form view over patch entries + raw TOML, live rebuild.
import { loadWasm } from './wasm.js';
import { packsState as state, render, downloadBytes } from './packs.js';
import { savePack } from './pack-store.js';
import { parseEntries, serializeEntries } from './toml-lite.js';

const FILES = { monsters: 'monsters.toml', items: 'items.toml', roles: 'roles.toml' };
const ARRAY_KEY = { monsters: 'monster', items: 'item', roles: 'role' };
const COMPLEX = new Set(['attacks', 'intrinsics', 'abilities', 'starting_items', 'skills', 'pantheon', 'quest', 'armor']);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

let schemas = null;
let schemasPromise = null;
function loadSchemas() {
  // Cache the promise: drawEditor can be called again while the first fetch is in flight.
  if (!schemasPromise) {
    schemasPromise = (async () => {
      const out = {};
      for (const k of Object.keys(FILES)) {
        try {
          const s = await (await fetch(`packs/schema/${k}.schema.json`)).json();
          const defs = s.definitions || s.$defs || {};
          const defName = Object.keys(defs).find((n) => /Patch$/.test(n));
          out[k] = { ...defs[defName], __defs: defs };
        } catch {
          out[k] = { properties: {}, __defs: {} };
        }
      }
      schemas = out;
      return out;
    })();
  }
  return schemasPromise;
}

function propType(schema, prop) {
  let p = schema.properties?.[prop] || {};
  if (p.$ref) p = schema.__defs[p.$ref.split('/').pop()] || {};
  if (p.anyOf) p = p.anyOf.find((x) => x.type !== 'null') || p.anyOf[0];
  if (p.$ref) p = schema.__defs[p.$ref.split('/').pop()] || {};
  if (p.enum) return { kind: 'enum', values: p.enum };
  if (p.oneOf && p.oneOf.every((x) => x.enum || x.const)) return { kind: 'enum', values: p.oneOf.flatMap((x) => x.enum || [x.const]) };
  const t = Array.isArray(p.type) ? p.type.find((x) => x !== 'null') : p.type;
  return { kind: t || 'string' };
}

let mode = 'form';
let activeTable = 'monsters';
let timer = null;

function scheduleRebuild() {
  clearTimeout(timer);
  timer = setTimeout(rebuild, 300);
}

function rebuild() {
  const json = JSON.stringify(state.sources);
  const errBox = document.getElementById('editor-errors');
  try {
    state.pack = state.wasm.WasmPack.fromFiles(json);
    errBox.innerHTML = '';
    render();
  } catch (e) {
    const rep = JSON.parse(state.wasm.reportForFilesJson(json));
    const items = rep.errors.length ? rep.errors.map((d) => `${d.file}: ${d.entry}${d.field ? '.' + d.field : ''} ${d.message}`) : [String(e.message || e)];
    errBox.innerHTML = items.map((t) => `<li>${esc(t)}</li>`).join('');
  }
}

async function drawEditor() {
  const panel = document.getElementById('tab-edit');
  if (!state.sources) { panel.innerHTML = '<p>This pack has no source files to edit. Download it or create a new pack from vanilla.</p>'; return; }
  await loadSchemas();
  panel.innerHTML = `
    <div class="editor-bar">
      <button data-mode="form" class="${mode === 'form' ? 'active' : ''}">Form</button>
      <button data-mode="raw" class="${mode === 'raw' ? 'active' : ''}">Raw TOML</button>
      <span class="spacer"></span>
      <button id="ed-save">Save</button>
      <button id="ed-dl-pack">Download .nrpack</button>
      <button id="ed-dl-src">Download sources</button>
    </div>
    <ul id="editor-errors" class="pack-error"></ul>
    <div id="editor-body"></div>`;
  panel.querySelectorAll('[data-mode]').forEach((b) => (b.onclick = () => { mode = b.dataset.mode; drawEditor(); }));
  panel.querySelector('#ed-save').onclick = async () => {
    if (!state.pack) return;
    const entry = await savePack(state.wasm, { bytes: state.pack.nrpackBytes(), sources: state.sources });
    location.search = `?pack=${encodeURIComponent(entry.key)}`;
  };
  panel.querySelector('#ed-dl-pack').onclick = () => {
    const m = JSON.parse(state.pack.manifestJson());
    downloadBytes(`${m.id}-${m.version}.nrpack`, state.pack.nrpackBytes());
  };
  panel.querySelector('#ed-dl-src').onclick = () => {
    for (const [name, text] of Object.entries(state.sources)) downloadBytes(name.replaceAll('/', '_'), text, 'text/plain');
  };
  const body = panel.querySelector('#editor-body');
  if (mode === 'raw') drawRaw(body); else drawForm(body);
}

function drawRaw(body) {
  const names = Object.keys(state.sources).sort();
  let current = names.includes('pack.toml') ? 'pack.toml' : names[0];
  body.innerHTML = `<div class="tabs">${names.map((n) => `<button data-file="${esc(n)}">${esc(n)}</button>`).join('')}</div><textarea id="raw-text" spellcheck="false" rows="28"></textarea>`;
  const ta = body.querySelector('#raw-text');
  const show = (n) => { current = n; ta.value = state.sources[n]; body.querySelectorAll('[data-file]').forEach((b) => b.classList.toggle('active', b.dataset.file === n)); };
  body.querySelectorAll('[data-file]').forEach((b) => (b.onclick = () => show(b.dataset.file)));
  ta.oninput = () => { state.sources[current] = ta.value; scheduleRebuild(); };
  show(current);
}

function drawForm(body) {
  const table = activeTable;
  const file = FILES[table];
  const schema = schemas[table];
  const entries = parseEntries(state.sources[file] || '', ARRAY_KEY[table]);
  const vanilla = JSON.parse(state.wasm.WasmPack.vanilla().rulesetJson())[table].map((r) => r.name);
  const props = Object.keys(schema.properties || {}).filter((p) => !['name', 'new', 'remove'].includes(p));

  const lossy = entries.lossy;
  body.innerHTML = `
    ${lossy ? `<p class="notice">${esc(file)} has entries the form cannot round-trip (nested tables, arrays or comments inside entries). Edit it in Raw TOML mode.</p>` : ''}
    <div class="tabs">${Object.keys(FILES).map((t) => `<button data-table="${t}" class="${t === table ? 'active' : ''}">${t}</button>`).join('')}</div>
    <datalist id="vanilla-names">${vanilla.map((n) => `<option value="${esc(n)}">`).join('')}</datalist>
    <div class="entries">${entries.map((e, i) => `
      <fieldset data-i="${i}">
        <legend>${esc(e.fields.name)} ${e.fields.new ? '<span class="badge">new</span>' : ''} <button data-del-entry="${i}" title="Remove entry">×</button></legend>
        ${Object.entries(e.fields).filter(([k]) => k !== 'name').map(([k, v]) => fieldInput(schema, i, k, v)).join('')}
        <select data-add-field="${i}"><option value="">+ field</option>${props.filter((p) => !(p in e.fields)).map((p) => `<option>${p}</option>`).join('')}</select>
      </fieldset>`).join('')}
    </div>
    <div class="add-entry">
      <input list="vanilla-names" id="new-entry-name" placeholder="name (vanilla to patch, or new)">
      <button id="add-entry">+ Patch entry</button>
    </div>`;

  body.querySelectorAll('[data-table]').forEach((b) => (b.onclick = () => { activeTable = b.dataset.table; drawForm(body); }));
  if (lossy) {
    body.querySelectorAll('.entries input, .entries select, .entries button, .add-entry input, .add-entry button').forEach((el) => (el.disabled = true));
    return;
  }
  const commit = () => { state.sources[file] = serializeEntries(entries, ARRAY_KEY[table]); scheduleRebuild(); };
  body.querySelectorAll('[data-field]').forEach((inp) => {
    inp.onchange = () => {
      const [i, k] = inp.dataset.field.split('|');
      const t = propType(schema, k).kind;
      entries[+i].fields[k] = inp.type === 'checkbox' ? inp.checked : (t === 'integer' || t === 'number') ? Number(inp.value) : inp.value;
      commit();
    };
  });
  body.querySelectorAll('[data-del-field]').forEach((b) => (b.onclick = () => { const [i, k] = b.dataset.delField.split('|'); delete entries[+i].fields[k]; commit(); drawForm(body); }));
  body.querySelectorAll('[data-del-entry]').forEach((b) => (b.onclick = () => { entries.splice(+b.dataset.delEntry, 1); commit(); drawForm(body); }));
  body.querySelectorAll('[data-add-field]').forEach((sel) => (sel.onchange = () => {
    if (!sel.value) return;
    const t = propType(schema, sel.value);
    if (COMPLEX.has(sel.value) || t.kind === 'object' || t.kind === 'array') { mode = 'raw'; drawEditor(); return; }
    entries[+sel.dataset.addField].fields[sel.value] = t.kind === 'boolean' ? false : (t.kind === 'integer' || t.kind === 'number') ? 0 : t.kind === 'enum' ? t.values[0] : '';
    commit(); drawForm(body);
  }));
  body.querySelector('#add-entry').onclick = () => {
    const name = body.querySelector('#new-entry-name').value.trim();
    if (!name) return;
    const fields = { name };
    if (!vanilla.includes(name)) {
      if (table === 'roles') return alertInline('Roles can only be patched, not added.');
      fields.new = true;
    }
    entries.push({ fields, complex: false });
    commit(); drawForm(body);
  };
}

function alertInline(msg) {
  document.getElementById('editor-errors').innerHTML = `<li>${esc(msg)}</li>`;
}

function fieldInput(schema, i, k, v) {
  const id = `${i}|${k}`;
  const del = `<button data-del-field="${id}" title="Remove field">×</button>`;
  if (v && typeof v === 'object' && 'raw' in v) return `<label>${k} <code>${esc(v.raw)}</code> <em>(edit in raw mode)</em> ${del}</label>`;
  const t = propType(schema, k);
  if (t.kind === 'boolean') return `<label>${k} <input type="checkbox" data-field="${id}" ${v ? 'checked' : ''}> ${del}</label>`;
  if (t.kind === 'integer' || t.kind === 'number') return `<label>${k} <input type="number" data-field="${id}" value="${esc(v)}"> ${del}</label>`;
  if (t.kind === 'enum') return `<label>${k} <select data-field="${id}">${t.values.map((x) => `<option ${x === v ? 'selected' : ''}>${esc(x)}</option>`).join('')}</select> ${del}</label>`;
  return `<label>${k} <input type="text" data-field="${id}" value="${esc(v)}"> ${del}</label>`;
}

document.addEventListener('pack-selected', async () => {
  state.wasm = state.wasm || (await loadWasm());
  if (!document.getElementById('tab-edit').hidden || state.key === 'new') drawEditor();
});
document.addEventListener('click', (e) => {
  if (e.target.matches('.tabs button[data-tab="edit"]')) drawEditor();
});
