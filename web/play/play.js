// NetHackED clean terminal: an 80x24 NetHack-style screen driven only by the
// keyboard. Line 0 is the message line, lines 1-21 the map, 22-23 status.
// URL options: ?pack=<bundled pack id>  ?lang=uk  ?seed=<n>

const BASE = document.querySelector('meta[name="nethacked-base"]')?.content ?? '../';
// Build id from the deploy: versioned URLs so a browser never pairs a cached
// old wasm module with new page code.
const BUILD = document.querySelector('meta[name="nethacked-build"]')?.content ?? 'dev';
const wasm = await import(`${BASE}pkg/nethacked_wasm.js?v=${BUILD}`);
await wasm.default({ module_or_path: `${BASE}pkg/nethacked_wasm_bg.wasm?v=${BUILD}` });
const { WasmGameSession, WasmPack } = wasm;

const COLS = 80;
const ROWS = 24;
const term = document.getElementById('term');
const canvas = document.getElementById('screen');

// Pixel renderer (pixel-ssh: bitmap fonts, palettes, CRT effects over WebGL2).
// Without WebGL2 the page keeps the plain text terminal.
let pixel = null;
try {
  pixel = new wasm.PixelScreen(canvas);
  const saved = JSON.parse(localStorage.getItem('nethacked.pixel') || 'null');
  if (Array.isArray(saved) && saved.length === 3) pixel.restore(...saved);
  document.body.classList.add('pixel');
} catch (e) {
  console.warn('Pixel renderer unavailable, using text terminal:', e?.message || e);
  pixel = null;
}
const ROLE = { text: 0, hero: 1, monster: 2, gold: 3, item: 4, stairs: 5, door: 6, wall: 7, water: 8, message: 9, accent: 10 };
const CLASS_ROLE = { 'c-hero': ROLE.hero, 'c-mon': ROLE.monster, 'c-gold': ROLE.gold, 'c-item': ROLE.item,
  'c-stair': ROLE.stairs, 'c-door': ROLE.door, 'c-wall': ROLE.wall, 'c-water': ROLE.water };
const params = new URLSearchParams(location.search);
const LANG = params.get('lang') === 'uk' ? 'uk' : 'en';
document.documentElement.lang = LANG;
// Role, race and status-label names come from the engine's i18n tables.
const UI = JSON.parse(wasm.uiStringsJson(LANG));

// Text the page draws itself (engine messages are localized by the engine).
const STR = {
  en: {
    tagline: 'NetHackED — classic NetHack in Rust and WebAssembly',
    pick: "Shall I pick a character's role for you?",
    random: 'Random',
    pickHint: '(end with a letter; ? for help after the game starts)',
    displayKeys: 'F2 system  F3 palette  F4 effects',
    pack: 'Rule pack',
    hero: 'Hero',
    welcome: (name, align, role) => `Hello ${name}, welcome to NetHackED!  You are a ${align.toLowerCase()} ${role.toLowerCase()}.`,
    title: (name, role) => `${name} the ${role}`,
    die: 'You die...  (press any key to start a new game)',
    inventory: 'Inventory',
    empty: 'Not carrying anything.',
    anyKey: '(press any key)',
    unpaid: (n) => ` (unpaid, ${n} zorkmids)`,
    buc: { Blessed: 'blessed ', Cursed: 'cursed ' },
    nothingTo: (verb) => `You don't have anything to ${verb}.`,
    whatTo: (verb, last) => `What do you want to ${verb}? [a-${last} or ?*]`,
    noObject: "You don't have that object.",
    never: 'Never mind.',
    direction: 'In what direction?',
    strangeDir: 'What a strange direction!',
    engrave: 'What do you want to write in the dust here? ',
    unknown: (k) => `Unknown command '${k}'.  (? for help)`,
    packFail: (e) => `Could not load pack: ${e}`,
    verbs: { eat: 'eat', quaff: 'drink', read: 'read', wield: 'wield', drop: 'drop', apply: 'use or apply', rub: 'rub', sacrifice: 'sacrifice' },
  },
  uk: {
    tagline: 'NetHackED — класичний NetHack на Rust і WebAssembly',
    pick: 'Обрати роль персонажа замість вас?',
    random: 'Випадково',
    pickHint: '(натисніть літеру; ? — довідка після початку гри)',
    displayKeys: 'F2 система  F3 палітра  F4 ефекти',
    pack: 'Набір правил',
    hero: 'Герой',
    welcome: (name, align, role) => `Вітаємо, ${name}, у NetHackED!  Ви — ${role}, шлях: ${align.toLowerCase()}.`,
    title: (name, role) => `${name} — ${role}`,
    die: 'Ви загинули...  (натисніть будь-яку клавішу, щоб почати нову гру)',
    inventory: 'Інвентар',
    empty: 'У вас нічого немає.',
    anyKey: '(натисніть будь-яку клавішу)',
    unpaid: (n) => ` (не оплачено, ${n} золотих)`,
    buc: { Blessed: 'благословенний ', Cursed: 'проклятий ' },
    nothingTo: (verb) => `У вас немає нічого, що можна ${verb}.`,
    whatTo: (verb, last) => `Що ви хочете ${verb}? [a-${last} або ?*]`,
    noObject: 'У вас немає такого предмета.',
    never: 'Гаразд, не треба.',
    direction: 'У якому напрямку?',
    strangeDir: 'Дивний напрямок!',
    engrave: 'Що ви хочете написати в пилу тут? ',
    unknown: (k) => `Невідома команда '${k}'.  (? — довідка)`,
    packFail: (e) => `Не вдалося завантажити набір правил: ${e}`,
    verbs: { eat: "з'їсти", quaff: 'випити', read: 'прочитати', wield: 'взяти в руки', drop: 'викинути', apply: 'застосувати', rub: 'потерти', sacrifice: 'принести в жертву' },
  },
}[LANG];

const ROLES = [
  ['valkyrie', 'human'],
  ['wizard', 'human'],
  ['barbarian', 'orc'],
  ['rogue', 'human'],
  ['knight', 'dwarf'],
  ['monk', 'human'],
  ['healer', 'gnome'],
  ['tourist', 'human'],
  ['archaeologist', 'human'],
].map(([role, race]) => [role, race, UI.role[role] || role, UI.race[race] || race]);

const DIRS = { h: 'west', j: 'south', k: 'north', l: 'east', y: 'northwest', u: 'northeast', b: 'southwest', n: 'southeast' };
const ARROWS = { ArrowLeft: 'h', ArrowDown: 'j', ArrowUp: 'k', ArrowRight: 'l' };

// Item commands: key -> [action, verb for the prompt].
const ITEM_CMDS = Object.fromEntries(
  Object.entries({ e: 'eat', q: 'quaff', r: 'read', w: 'wield', d: 'drop', a: 'apply', R: 'rub', S: 'sacrifice' })
    .map(([k, action]) => [k, [action, STR.verbs[action]]]),
);
const DIR_CMDS = { x: ['cast', 'cast'], z: ['zap', 'zap'], f: ['fire', 'fire'], F: ['kick', 'kick'] };

const HELP_UK = [
  'Клавіші NetHackED',
  '',
  ' y k u    рух: h j k l y u b n (або стрілки)',
  '  \\|/     .  чекати        s  шукати        ,  підняти',
  ' h-.-l    <  вгору         >  вниз          i  інвентар',
  '  /|\\     e  їсти          q  пити          r  читати',
  ' b j n    w  зброя         d  викинути      a  застосувати',
  '          R  потерти       x  закляття      z  жезл',
  '          f  стріляти      F  копнути       E  написати',
  '          p  платити       P  молитися      S  жертва',
  '          ?  довідка       Esc скасовує запит',
  '          ^P попереднє повідомлення',
  '          F2 система       F3 палітра       F4 ефекти',
  '',
  'Параметри URL: ?pack=hard-mode  ?lang=uk  ?seed=42',
  '',
  '(натисніть будь-яку клавішу)',
];

const HELP_EN = [
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
  '          ^P previous message',
  '          F2 system        F3 palette       F4 effects',
  '',
  'URL options: ?pack=hard-mode  ?lang=uk  ?seed=42',
  '',
  '(press any key)',
];
const HELP = LANG === 'uk' ? HELP_UK : HELP_EN;

let session = null;
let mode = 'pick'; // pick | play | item | dir | text | overlay | over
let pending = null; // prompt state
let overlay = null; // lines drawn over the map
let packLabel = '';
let roleLabel = '';

// The message line lives in the engine (a port of the tty top line: packing,
// --More--, ^P history). say() is a pline that is remembered for ^P; prompt()
// shows a prompt or echo that is not.
const say = (text) => session.msgPost(text);
const prompt = (text) => session.msgShow(text);

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
  const align = LANG === 'uk' ? session.get_localized_alignment() : session.get_player_alignment();
  // Show hunger only when it is not the normal state (canonical English name).
  const hungerState = session.get_hunger_state();
  const hunger = hungerState === 'Normal' ? '' : ' ' + (LANG === 'uk' ? session.get_localized_hunger_state() : hungerState);
  const l1 = `${STR.title(name, roleLabel)}  ${align}${packLabel ? `  [${packLabel}]` : ''}`;
  const hp = `${session.get_player_hp()}(${session.get_player_max_hp()})`;
  const pw = `${session.get_player_pw()}(${session.get_player_max_pw()})`;
  const l2 =
    LANG === 'uk'
      ? `${UI.status.dlvl}:${session.get_depth()} ${UI.status.gold}:${session.get_player_gold()} ${UI.status.hp}:${hp} ${UI.status.pw}:${pw} ${UI.status.ac}:${session.get_player_ac()} ${UI.status.turn}:${session.get_turn()}${hunger}`
      : `Dlvl:${session.get_depth()} $:${session.get_player_gold()} HP:${hp} Pw:${pw} AC:${session.get_player_ac()} T:${session.get_turn()}${hunger}`;
  return [l1, l2];
}

function draw() {
  const lines = [];
  if (mode === 'pick') {
    lines.push(...pickScreen());
  } else {
    lines.push(pad(session.msgLine()));
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
  if (pixel) drawPixel(lines.slice(0, ROWS));
  term.innerHTML = lines
    .slice(0, ROWS)
    .map((l, i) => (mode !== 'pick' && i >= 1 && i <= 21 && !overlay ? colorLine(l) : esc(l)))
    .join('\n');
  fit();
}

function pickScreen() {
  const out = [
    '',
    `  ${STR.tagline}`,
    '  from Yemelianov (Emelyanov Dmytro)',
    '',
    `  ${STR.pick}`,
    '',
  ];
  ROLES.forEach(([, , label, raceLabel], i) => out.push(`    ${String.fromCharCode(97 + i)} - ${label} (${raceLabel})`));
  out.push('', `    * - ${STR.random}`, '', `  ${STR.pickHint}`);
  if (pixel) out.push(`  ${STR.displayKeys}`);
  if (packLabel) out.push('', `  ${STR.pack}: ${packLabel}`);
  return out.map(pad);
}

// Color role per cell for the pixel renderer.
function drawPixel(lines) {
  const roles = new Uint8Array(COLS * ROWS);
  lines.forEach((line, row) => {
    for (let col = 0; col < Math.min(COLS, line.length); col++) {
      let r = ROLE.text;
      if (mode === 'pick') {
        r = row === 1 ? ROLE.message : row === 2 ? ROLE.accent : ROLE.text;
      } else if (row === 0) {
        r = ROLE.message;
      } else if (row <= 21 && !overlay) {
        r = CLASS_ROLE[glyphClass(line[col])] ?? ROLE.text;
      } else if (row === 22 && packLabel && line.indexOf(`[${packLabel}]`) >= 0 && col >= line.indexOf(`[${packLabel}]`)) {
        r = ROLE.accent;
      }
      roles[row * COLS + col] = r;
    }
  });
  pixel.draw(lines.join('\n'), roles);
}

function fit() {
  if (pixel) return; // the canvas is sized by CSS
  // Scale the 80x24 grid to the window while keeping crisp monospace text.
  term.style.transform = 'none';
  const r = term.getBoundingClientRect();
  const s = Math.min(window.innerWidth / r.width, window.innerHeight / r.height) * 0.96;
  term.style.transform = `scale(${s})`;
}
window.addEventListener('resize', fit);

// One engine step. The engine posts the turn's messages to the message line
// (the same turn_messages the terminal UI uses); a rejected action only says why.
function act(action, arg = null) {
  const res = JSON.parse(session.step(action, arg));
  if (res.error) {
    say(res.error);
    return;
  }
  if (res.is_game_over && mode !== 'over') {
    mode = 'over';
    session.msgAppendPage(STR.die);
  }
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
  if (!items.length) return [title, '', STR.empty, '', STR.anyKey];
  return [
    title,
    '',
    ...items.map((it) => {
      const letter = String.fromCharCode(97 + it.index);
      const buc = STR.buc[it.buc] || '';
      const ench = it.enchantment ? `${it.enchantment > 0 ? '+' : ''}${it.enchantment} ` : '';
      const cost = it.unpaid_cost ? STR.unpaid(it.unpaid_cost) : '';
      return `${letter} - ${buc}${ench}${it.display_name || it.name}${cost}`;
    }),
    '',
    STR.anyKey,
  ];
}

function itemPrompt(action, verb) {
  const items = inventory();
  if (!items.length) {
    say(STR.nothingTo(verb));
    return;
  }
  const last = String.fromCharCode(96 + items.length);
  prompt(STR.whatTo(verb, last));
  mode = 'item';
  pending = { action, verb, count: items.length };
}

function dirPrompt(action) {
  prompt(STR.direction);
  mode = 'dir';
  pending = { action };
}

async function newGame(roleIdx) {
  const [role, race, label] = ROLES[roleIdx];
  roleLabel = label;
  const seed = BigInt(params.get('seed') || Math.floor(Math.random() * 2 ** 31));
  const packId = params.get('pack');
  let failure = '';
  try {
    if (packId) {
      const index = await (await fetch(`${BASE}packs/index.json`)).json();
      const entry = index.find((p) => p.id === packId);
      if (!entry) throw new Error(`unknown pack "${packId}"`);
      const bytes = new Uint8Array(await (await fetch(`${BASE}packs/${entry.file}`)).arrayBuffer());
      session = WasmGameSession.newWithPack(seed, role, race, STR.hero, WasmPack.fromNhpack(bytes));
    } else {
      session = WasmGameSession.new_with_character(seed, role, race, STR.hero);
    }
  } catch (e) {
    session = WasmGameSession.new_with_character(seed, role, race, STR.hero);
    failure = STR.packFail(e.message || e);
    packLabel = '';
  }
  if (params.get('lang') === 'uk') session.set_locale('uk');
  if (failure) {
    say(failure);
  } else {
    const align = LANG === 'uk' ? session.get_localized_alignment() : session.get_player_alignment();
    say(STR.welcome(STR.hero, align, label));
  }
  mode = 'play';
}

// F2 / F3 / F4: display system, palette, visual effects (pixel renderer only).
function onDisplayKey(key) {
  if (!pixel) return false;
  const label = key === 'F2' ? pixel.cycleSystem() : key === 'F3' ? pixel.cyclePalette() : key === 'F4' ? pixel.cycleEffects() : null;
  if (label === null) return false;
  try { localStorage.setItem('nethacked.pixel', JSON.stringify(Array.from(pixel.state()))); } catch { /* storage unavailable */ }
  if (session && mode !== 'pick') prompt(label);
  draw();
  return true;
}

function onKey(e) {
  // ^P: previous message (cmd.c:164 doprev_message). Handled before the
  // modifier early return, and preventDefault keeps the browser's print dialog
  // away. Ignored at --More-- and outside the idle play screen.
  if (e.ctrlKey && !e.metaKey && !e.altKey && (e.code === 'KeyP' || e.key === 'p' || e.key === 'P')) {
    e.preventDefault();
    if (session && mode === 'play' && !session.msgMore()) {
      session.msgPrev();
      draw();
    }
    return;
  }
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  if (onDisplayKey(e.key)) {
    e.preventDefault();
    return;
  }
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

  if (session.msgMore()) {
    // --More--: Esc skips the rest, any other key shows the next page. The
    // game-over line stays on screen after a skip (it is the last thing to see).
    if (key === 'Escape') {
      session.msgSkipRest();
      if (mode === 'over') session.msgShow(STR.die);
    } else {
      session.msgDismiss();
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
    if (key === '?' || key === '*') {
      overlay = invLines(STR.inventory);
      draw();
      overlay = null;
      return;
    }
    const { action, count } = pending;
    // Leave the prompt first: act() may turn the mode into 'over'.
    mode = 'play';
    pending = null;
    const idx = key.charCodeAt(0) - 97;
    if (key === 'Escape') say(STR.never);
    else if (key.length === 1 && idx >= 0 && idx < count) act(action, String(idx));
    else say(STR.noObject);
    draw();
    return;
  }

  if (mode === 'dir') {
    const { action } = pending;
    mode = 'play';
    pending = null;
    if (key === 'Escape') say(STR.never);
    else if (DIRS[key]) act(action, DIRS[key]);
    else say(STR.strangeDir);
    draw();
    return;
  }

  if (mode === 'text') {
    if (key === 'Escape') {
      say(STR.never);
      mode = 'play';
    } else if (key === 'Enter') {
      const text = pending.text.trim();
      const { action } = pending;
      mode = 'play';
      pending = null;
      if (text) act(action, text);
      else say(STR.never);
    } else if (key === 'Backspace') {
      pending.text = pending.text.slice(0, -1);
      prompt(pending.prompt + pending.text);
    } else if (key.length === 1 && pending.text.length < 60) {
      pending.text += key;
      prompt(pending.prompt + pending.text);
    }
    draw();
    return;
  }

  // mode === 'play': the line is cleared when the next command key is read
  // (wintty.c:4100-4102).
  session.msgClear();
  if (DIRS[key]) act('move', DIRS[key]);
  else if (key === '.') act('wait');
  else if (key === 's') act('search');
  else if (key === ',') act('pickup');
  else if (key === '<') act('ascend');
  else if (key === '>') act('descend');
  else if (key === 'p') act('pay');
  else if (key === 'P') act('pray');
  else if (key === 'i') { overlay = invLines(STR.inventory); mode = 'overlay'; }
  else if (key === '?') { overlay = HELP; mode = 'overlay'; }
  else if (ITEM_CMDS[key]) itemPrompt(...ITEM_CMDS[key]);
  else if (DIR_CMDS[key]) dirPrompt(DIR_CMDS[key][0]);
  else if (key === 'E') {
    pending = { action: 'engrave', prompt: STR.engrave, text: '' };
    prompt(pending.prompt);
    mode = 'text';
  } else if (key !== 'Escape') say(STR.unknown(key));
  draw();
}

window.addEventListener('keydown', onKey);

if (pixel) {
  const t0 = performance.now();
  const loop = (now) => {
    pixel.frame((now - t0) / 1000);
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
}

const packParam = params.get('pack');
if (packParam) packLabel = packParam;
draw();
term.focus();
