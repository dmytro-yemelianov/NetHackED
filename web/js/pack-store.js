// Catalog of rule packs: vanilla, bundled (web/packs/index.json), and packs
// the visitor uploaded or edited (IndexedDB, keyed by hash).

const DB_NAME = 'nethacked-packs';
const STORE = 'packs';
export const ACTIVE_KEY = 'nethacked.activePack';

function openDb() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE, { keyPath: 'key' });
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function tx(mode, fn) {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const t = db.transaction(STORE, mode);
    const store = t.objectStore(STORE);
    const out = fn(store);
    t.oncomplete = () => resolve(out && 'result' in out ? out.result : undefined);
    t.onerror = () => reject(t.error);
  });
}

let bundledCache = null;
async function bundled() {
  if (!bundledCache) {
    try {
      const r = await fetch('packs/index.json', { cache: 'no-cache' });
      bundledCache = r.ok ? await r.json() : [];
    } catch {
      bundledCache = [];
    }
  }
  return bundledCache;
}

async function storedRecords() {
  try {
    return (await tx('readonly', (s) => s.getAll())) || [];
  } catch {
    return []; // IndexedDB unavailable (private mode): no stored packs
  }
}

export async function listPacks() {
  const list = [{ key: 'vanilla', id: 'vanilla', version: '', title: 'Vanilla', hash: '', origin: 'vanilla', hasSources: false }];
  for (const b of await bundled()) {
    list.push({ key: `bundled:${b.id}`, id: b.id, version: b.version, title: b.title || b.id, hash: b.hash, origin: 'bundled', hasSources: true });
  }
  for (const r of await storedRecords()) {
    list.push({ key: r.key, id: r.id, version: r.version, title: r.title, hash: r.hash, origin: 'stored', hasSources: !!r.sources });
  }
  return list;
}

export async function openPack(wasm, key) {
  if (key === 'vanilla') return wasm.WasmPack.vanilla();
  if (key.startsWith('bundled:')) {
    const b = (await bundled()).find((e) => `bundled:${e.id}` === key);
    if (!b) throw new Error(`Unknown bundled pack ${key}`);
    const r = await fetch(`packs/${b.file}`);
    if (!r.ok) throw new Error(`Could not download ${b.file} (${r.status})`);
    return wasm.WasmPack.fromNhpack(new Uint8Array(await r.arrayBuffer()));
  }
  const rec = await tx('readonly', (s) => s.get(key));
  if (!rec) throw new Error(`Pack ${key} is no longer stored`);
  return wasm.WasmPack.fromNhpack(rec.bytes);
}

export async function getSources(key) {
  if (key.startsWith('bundled:')) {
    const b = (await bundled()).find((e) => `bundled:${e.id}` === key);
    if (!b) return null;
    const list = await (await fetch(`packs/${b.sources}files.txt`)).text();
    const out = {};
    for (const f of list.split('\n').map((s) => s.trim()).filter(Boolean)) {
      out[f] = await (await fetch(`packs/${b.sources}${f}`)).text();
    }
    return out;
  }
  if (key.startsWith('stored:')) {
    const rec = await tx('readonly', (s) => s.get(key));
    return rec?.sources ?? null;
  }
  return null;
}

export async function savePack(wasm, { bytes, sources = null }) {
  const pack = wasm.WasmPack.fromNhpack(bytes); // verifies format + hash
  const m = JSON.parse(pack.manifestJson());
  const rec = { key: `stored:${pack.hash()}`, id: m.id, version: m.version, title: m.name || m.id, hash: pack.hash(), bytes, sources };
  await tx('readwrite', (s) => s.put(rec));
  return { key: rec.key, id: rec.id, version: rec.version, title: rec.title, hash: rec.hash, origin: 'stored', hasSources: !!sources };
}

export async function deletePack(key) {
  if (!key.startsWith('stored:')) throw new Error('Only stored packs can be deleted');
  await tx('readwrite', (s) => s.delete(key));
}
