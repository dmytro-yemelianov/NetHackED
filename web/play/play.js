// NetHackED clean terminal: an 80x24 NetHack-style screen driven only by the
// keyboard. Line 0 is the message line, lines 1-21 the map, 22-23 status.
// URL options: ?pack=<bundled pack id>  ?lang=uk  ?seed=<n>

const BASE = document.querySelector('meta[name="nethacked-base"]')?.content ?? '../';
const wasm = await import(`${BASE}pkg/nethacked_wasm.js`);
await wasm.default();
const { WasmGameSession, WasmPack } = wasm;

const COLS = 80;
const ROWS = 24;
const term = document.getElementById('term');
const params = new URLSearchParams(location.search);

const ROLES = [
  ['valkyrie', 'human', 'Valkyrie'],
  ['wizard', 'human', 'Wizard'],
  ['barbarian', 'orc', 'Barbarian'],
  ['rogue', 'human', 'Rogue'],
  ['knight', 'dwarf', 'Knight'],
  ['monk', 'human', 'Monk'],
  ['healer', 'gnome', 'Healer'],
  ['tourist', 'human', 'Tourist'],
  ['archaeologist', 'human', 'Archaeologist'],
];

const DIRS = { h: 'west', j: 'south', k: 'north', l: 'east', y: 'northwest', u: 'northeast', b: 'southwest', n: 'southeast' };
const ARROWS = { ArrowLeft: 'h', ArrowDown: 'j', ArrowUp: 'k', ArrowRight: 'l' };

// Item commands: key -> [action, verb for the prompt].
const ITEM_CMDS = {
  e: ['eat', 'eat'],
  q: ['quaff', 'drink'],
  r: ['read', 'read'],
  w: ['wield', 'wield'],
  d: ['drop', 'drop'],
  a: ['apply', 'use or apply'],
  R: ['rub', 'rub'],
  S: ['sacrifice', 'sacrifice'],
};
const DIR_CMDS = { x: ['cast', 'cast'], z: ['zap', 'zap'], f: ['fire', 'fire'], F: ['kick', 'kick'] };

const HELP = [
  'NetHackED keys',
  '',
  ' y k u    move: h j k l y u b n (or arrow keys)',
  '  \\|/     .  wait          s  search        ,  pick up',
  ' h-.-l    <  go up         >  go down       i  inventory',
  '  /|\\     e  eat           q  quaff         r  read',
  ' b j n    w  wield         d  drop          a  apply',
  '          R  rub           x  cast          z  zap',
  '          f  fire          F  kick          E  engrave',
  '          p  pay           P  pray          S  sacrifice',
  '          ?  this help     Esc cancels a prompt',
  '',
  'URL options: ?pack=hard-mode  ?lang=uk  ?seed=42',
  '',
  '(press any key)',
];

let session = null;
let mode = 'pick'; // pick | play | item | dir | text | overlay | over
let pending = null; // prompt state
let messages = []; // queued messages (shown with --More--)
let message = '';
let overlay = null; // lines drawn over the map
let packLabel = '';
let roleLabel = '';

const esc = (s) => s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
const pad = (s) => (s.length > COLS ? s.slice(0, COLS) : s.padEnd(COLS));

function glyphClass(ch) {
  if (ch === '@') return 'c-hero';
  if (/[a-zA-Z]/.test(ch)) return 'c-mon';
  if (ch === '$') return 'c-gold';
  if (ch === '<' || ch === '>') return 'c-stair';
  if (ch === '+') return 'c-door';
  if (ch === '|' || ch === '-') return 'c-wall';
  if (ch === '}' || ch === '~') return 'c-water';
  if (')[!?/="*%(`0_'.includes(ch)) return 'c-item';
  return '';
}

function colorLine(line) {
  let out = '';
  let run = '';
  let runCls = '';
  const flush = () => {
    if (!run) return;
    out += runCls ? `<span class="${runCls}">${esc(run)}</span>` : esc(run);
    run = '';
  };
  for (const ch of line) {
    const cls = glyphClass(ch);
    if (cls !== runCls) {
      flush();
      runCls = cls;
    }
    run += ch;
  }
  flush();
  return out;
}

function statusLines() {
  const name = session.get_player_name();
  const align = params.get('lang') === 'uk' ? session.get_localized_alignment() : session.get_player_alignment();
  const hunger = params.get('lang') === 'uk' ? session.get_localized_hunger_state() : session.get_hunger_state();
  const l1 = `${name} the ${roleLabel}  ${align}${packLabel ? `  [${packLabel}]` : ''}`;
  const l2 = `Dlvl:${session.get_depth()} $:${session.get_player_gold()} HP:${session.get_player_hp()}(${session.get_player_max_hp()}) Pw:${session.get_player_pw()}(${session.get_player_max_pw()}) AC:${session.get_player_ac()} T:${session.get_turn()}${hunger && hunger !== 'Normal' && hunger !== 'Не голодний' ? ' ' + hunger : ''}`;
  return [l1, l2];
}

function draw() {
  const lines = [];
  if (mode === 'pick') {
    lines.push(...pickScreen());
  } else {
    const msg = messages.length ? `${messages[0]}${messages.length > 1 ? '--More--' : ''}` : message;
    lines.push(pad(msg));
    const map = session.render_ascii().split('\n');
    for (let y = 0; y < 21; y++) lines.push(pad(map[y] ?? ''));
    lines.push(...statusLines().map(pad));
    if (overlay) {
      // Right-aligned overlay window, like NetHack's menus.
      const width = Math.min(COLS, Math.max(...overlay.map((l) => l.length)) + 2);
      const left = COLS - width;
      overlay.slice(0, 22).forEach((l, i) => {
        const row = i + 1;
        lines[row] = lines[row].slice(0, left) + pad(' ' + l).slice(0, width);
      });
    }
  }
  while (lines.length < ROWS) lines.push(pad(''));
  term.innerHTML = lines
    .slice(0, ROWS)
    .map((l, i) => (mode !== 'pick' && i >= 1 && i <= 21 && !overlay ? colorLine(l) : esc(l)))
    .join('\n');
  fit();
}

function pickScreen() {
  const out = [
    '',
    '  NetHackED — classic NetHack in Rust and WebAssembly',
    '  from Yemelianov (Emelyanov Dmytro)',
    '',
    '  Shall I pick a character\'s role for you?',
    '',
  ];
  ROLES.forEach(([, race, label], i) => out.push(`    ${String.fromCharCode(97 + i)} - ${label} (${race})`));
  out.push('', '    * - Random', '', '  (end with a letter; ? for help after the game starts)');
  if (packLabel) out.push('', `  Rule pack: ${packLabel}`);
  return out.map(pad);
}

function fit() {
  // Scale the 80x24 grid to the window while keeping crisp monospace text.
  term.style.transform = 'none';
  const r = term.getBoundingClientRect();
  const s = Math.min(window.innerWidth / r.width, window.innerHeight / r.height) * 0.96;
  term.style.transform = `scale(${s})`;
}
window.addEventListener('resize', fit);

function pushEvents(res) {
  if (!res) return;
  const texts = (res.last_events || []).filter((e) => e.LogMessage).map((e) => e.LogMessage.text);
  messages = [];
  // Pack messages into 80-column lines, leaving room for --More--.
  let line = '';
  for (const t of texts) {
    if (!line) line = t;
    else if ((line + '  ' + t).length <= COLS - 8) line += '  ' + t;
    else { messages.push(line); line = t; }
  }
  if (line) messages.push(line);
  message = messages.length === 1 ? messages.shift() : '';
  if (res.is_game_over) {
    mode = 'over';
    messages.push('You die...  (press any key to start a new game)');
  }
}

function act(action, arg = null) {
  const res = JSON.parse(session.step(action, arg));
  if (res.error) {
    message = res.error;
    return;
  }
  pushEvents(res);
}

function inventory() {
  try {
    return JSON.parse(session.get_inventory_json());
  } catch {
    return [];
  }
}

function invLines(title) {
  const items = inventory();
  if (!items.length) return [title, '', 'Not carrying anything.', '', '(press any key)'];
  return [
    title,
    '',
    ...items.map((it) => {
      const letter = String.fromCharCode(97 + it.index);
      const buc = it.buc && it.buc !== 'Uncursed' ? `${it.buc.toLowerCase()} ` : '';
      const ench = it.enchantment ? `${it.enchantment > 0 ? '+' : ''}${it.enchantment} ` : '';
      const cost = it.unpaid_cost ? ` (unpaid, ${it.unpaid_cost} zorkmids)` : '';
      return `${letter} - ${buc}${ench}${it.name}${cost}`;
    }),
    '',
    '(press any key)',
  ];
}

function itemPrompt(action, verb) {
  const items = inventory();
  if (!items.length) {
    message = "You don't have anything to " + verb + '.';
    return;
  }
  const last = String.fromCharCode(96 + items.length);
  message = `What do you want to ${verb}? [a-${last} or ?*]`;
  mode = 'item';
  pending = { action, verb, count: items.length };
}

function dirPrompt(action) {
  message = 'In what direction?';
  mode = 'dir';
  pending = { action };
}

async function newGame(roleIdx) {
  const [role, race, label] = ROLES[roleIdx];
  roleLabel = label;
  const seed = BigInt(params.get('seed') || Math.floor(Math.random() * 2 ** 31));
  const packId = params.get('pack');
  try {
    if (packId) {
      const index = await (await fetch(`${BASE}packs/index.json`)).json();
      const entry = index.find((p) => p.id === packId);
      if (!entry) throw new Error(`unknown pack "${packId}"`);
      const bytes = new Uint8Array(await (await fetch(`${BASE}packs/${entry.file}`)).arrayBuffer());
      session = WasmGameSession.newWithPack(seed, role, race, 'Hero', WasmPack.fromNhpack(bytes));
    } else {
      session = WasmGameSession.new_with_character(seed, role, race, 'Hero');
    }
  } catch (e) {
    session = WasmGameSession.new_with_character(seed, role, race, 'Hero');
    message = `Could not load pack: ${e.message || e}`;
    packLabel = '';
  }
  if (params.get('lang') === 'uk') session.set_locale('uk');
  messages = [];
  if (!message) {
    const align = session.get_player_alignment().toLowerCase();
    message = `Hello Hero, welcome to NetHackED!  You are a ${align} ${ROLES[roleIdx][2].toLowerCase()}.  (? for help)`;
  }
  mode = 'play';
}

function onKey(e) {
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  const key = ARROWS[e.key] || e.key;
  if (key.length > 1 && key !== 'Escape' && key !== 'Enter') return;
  e.preventDefault();

  if (mode === 'pick') {
    const i = key === '*' ? Math.floor(Math.random() * ROLES.length) : key.charCodeAt(0) - 97;
    if (i >= 0 && i < ROLES.length && key.length === 1) {
      newGame(i).then(draw);
    }
    return;
  }

  if (messages.length) {
    // --More--: any key advances.
    messages.shift();
    if (!messages.length && mode === 'over') {
      mode = 'pick';
      session = null;
    }
    draw();
    return;
  }

  if (mode === 'over') {
    mode = 'pick';
    session = null;
    draw();
    return;
  }

  if (mode === 'overlay') {
    overlay = null;
    mode = 'play';
    draw();
    return;
  }

  if (mode === 'item') {
    if (key === 'Escape') {
      message = 'Never mind.';
    } else if (key === '?' || key === '*') {
      overlay = invLines('Inventory');
      draw();
      overlay = null;
      return;
    } else {
      const idx = key.charCodeAt(0) - 97;
      if (key.length === 1 && idx >= 0 && idx < pending.count) {
        act(pending.action, String(idx));
      } else {
        message = 'You don\'t have that object.';
      }
    }
    mode = 'play';
    pending = null;
    draw();
    return;
  }

  if (mode === 'dir') {
    if (key === 'Escape') message = 'Never mind.';
    else if (DIRS[key]) act(pending.action, DIRS[key]);
    else message = 'What a strange direction!';
    mode = 'play';
    pending = null;
    draw();
    return;
  }

  if (mode === 'text') {
    if (key === 'Escape') {
      message = 'Never mind.';
      mode = 'play';
    } else if (key === 'Enter') {
      const text = pending.text.trim();
      mode = 'play';
      if (text) act(pending.action, text);
      else message = 'Never mind.';
    } else if (key === 'Backspace') {
      pending.text = pending.text.slice(0, -1);
      message = pending.prompt + pending.text;
    } else if (key.length === 1 && pending.text.length < 60) {
      pending.text += key;
      message = pending.prompt + pending.text;
    }
    draw();
    return;
  }

  // mode === 'play'
  message = '';
  if (DIRS[key]) act('move', DIRS[key]);
  else if (key === '.') act('wait');
  else if (key === 's') act('search');
  else if (key === ',') act('pickup');
  else if (key === '<') act('ascend');
  else if (key === '>') act('descend');
  else if (key === 'p') act('pay');
  else if (key === 'P') act('pray');
  else if (key === 'i') { overlay = invLines('Inventory'); mode = 'overlay'; }
  else if (key === '?') { overlay = HELP; mode = 'overlay'; }
  else if (ITEM_CMDS[key]) itemPrompt(...ITEM_CMDS[key]);
  else if (DIR_CMDS[key]) dirPrompt(DIR_CMDS[key][0]);
  else if (key === 'E') {
    pending = { action: 'engrave', prompt: 'What do you want to write in the dust here? ', text: '' };
    message = pending.prompt;
    mode = 'text';
  } else if (key !== 'Escape') message = `Unknown command '${key}'.  (? for help)`;
  draw();
}

window.addEventListener('keydown', onKey);

const packParam = params.get('pack');
if (packParam) packLabel = packParam;
draw();
term.focus();
