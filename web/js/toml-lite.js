// Minimal TOML for the pack editor's form mode: `[[<arrayKey>]]` entries with
// plain scalar fields. Only lines this module can write back byte-for-byte are
// accepted; anything else marks the file `lossy` so the form goes read-only and
// the user edits it in raw mode instead of having content silently rewritten.

const KEY = /^([A-Za-z0-9_]+)\s*=\s*(.*)$/;
const SAFE_VALUE = /^(true|false|-?(0|[1-9][0-9]*)|"[^"\\]*")$/;
const COMPLEX_VALUE = /^[[{]/;

export function parseEntries(text, arrayKey) {
  const entries = [];
  const preamble = [];
  let cur = null;
  let lossy = false;
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (line === `[[${arrayKey}]]`) {
      cur = { fields: {}, complex: false };
      entries.push(cur);
      continue;
    }
    if (!cur) {
      // Before the first entry only comments and blank lines are kept verbatim.
      if (line && !line.startsWith('#')) lossy = true;
      preamble.push(raw);
      continue;
    }
    if (!line) continue;
    const m = line.match(KEY);
    if (!m) {
      lossy = true; // comment, other table header, dotted/quoted key
      continue;
    }
    const [, k, v] = m;
    if (COMPLEX_VALUE.test(v)) {
      cur.complex = true;
      cur.fields[k] = { raw: v };
      continue;
    }
    if (!SAFE_VALUE.test(v)) {
      lossy = true; // trailing comment, escapes, literal string, 1_000, 0x10, +5, 1e3, floats
      cur.fields[k] = { raw: v };
      continue;
    }
    cur.fields[k] = v === 'true' ? true : v === 'false' ? false : v.startsWith('"') ? v.slice(1, -1) : Number(v);
  }
  while (preamble.length && !preamble[preamble.length - 1].trim()) preamble.pop();
  entries.preamble = preamble.join('\n');
  entries.lossy = lossy || entries.some((e) => e.complex);
  return entries;
}

export function tomlValue(v) {
  if (v && typeof v === 'object' && 'raw' in v) return v.raw;
  if (typeof v === 'boolean') return String(v);
  if (typeof v === 'number') return Number.isFinite(v) ? String(Math.trunc(v)) : '0';
  return `"${String(v).replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n')}"`;
}

export function serializeEntries(entries, arrayKey) {
  const body = entries
    .map((e) => [`[[${arrayKey}]]`, ...Object.entries(e.fields).map(([k, v]) => `${k} = ${tomlValue(v)}`)].join('\n'))
    .join('\n\n');
  return [entries.preamble, body].filter(Boolean).join('\n\n') + '\n';
}
