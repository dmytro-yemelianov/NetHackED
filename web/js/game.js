// Game page: session UI, HUD, input, inventory, i18n, plus the AI arena and
// benchmark panels (they share the session and renderer).
import { loadWasm } from './wasm.js';
import { listPacks, openPack, ACTIVE_KEY } from './pack-store.js';

const wasmModule = await loadWasm();
const { WasmGameSession, TournamentRun, run_tactical_trajectory } = wasmModule;

let session = null;
let selectedRole = 'valkyrie';
let selectedRace = 'human';

// Arena Loop State
let arenaTimer = null;
let arenaStep = 0;
let arenaSeed = 42;
const recentMatches = [];

const viewportEl = document.getElementById('viewport');
const messageBarEl = document.getElementById('messageBar');
const statNameEl = document.getElementById('statName');
const statAlignEl = document.getElementById('statAlign');
const statDepthEl = document.getElementById('statDepth');
const statGoldEl = document.getElementById('statGold');
const statHpEl = document.getElementById('statHp');
const statPwEl = document.getElementById('statPw');
const statAcEl = document.getElementById('statAc');
const statNutrEl = document.getElementById('statNutr');
const statTurnEl = document.getElementById('statTurn');
const obsJsonEl = document.getElementById('obsJson');
const charDialog = document.getElementById('charDialog');
const leaderboardDialog = document.getElementById('leaderboardDialog');

const defaultBenchmarkData = {
  summary: {
    per_policy: {
      "SurvivalHeuristic": {
        mean_max_depth: 2.50,
        mean_kills: 2.10,
        mean_turns: 157.2,
        mean_gold: 50.0,
        win_rate_pct: 0.0
      },
      "AmuletSpeedrunner": {
        mean_max_depth: 2.47,
        mean_kills: 1.70,
        mean_turns: 156.0,
        mean_gold: 50.0,
        win_rate_pct: 0.0
      },
      "RandomBaseline": {
        mean_max_depth: 1.00,
        mean_kills: 0.00,
        mean_turns: 638.6,
        mean_gold: 50.0,
        win_rate_pct: 0.0
      }
    }
  }
};

function populateLeaderboardTable(data) {
  const tbody = document.getElementById('standingsTableBody');
  tbody.innerHTML = '';

  const policies = Object.entries(data.summary.per_policy)
    .map(([name, stats]) => ({ name, ...stats }))
    .sort((a, b) => b.mean_max_depth - a.mean_max_depth || b.mean_kills - a.mean_kills);

  policies.forEach((p, idx) => {
    const tr = document.createElement('tr');
    const medal = idx === 0 ? '🥇 ' : (idx === 1 ? '🥈 ' : (idx === 2 ? '🥉 ' : ''));
    tr.innerHTML = `
      <td style="font-weight:bold; color:var(--text-yellow);">${medal}#${idx + 1}</td>
      <td style="color:var(--text-white); font-weight:bold;">${escapeHtml(p.name)}</td>
      <td style="color:var(--text-green);">${p.win_rate_pct.toFixed(1)}%</td>
      <td>${p.mean_max_depth.toFixed(2)}</td>
      <td>${p.mean_kills.toFixed(1)}</td>
      <td>${p.mean_turns.toFixed(1)}</td>
      <td>${p.mean_gold.toFixed(0)}</td>
    `;
    tbody.appendChild(tr);
  });
}

async function loadLeaderboard() {
  try {
    const resp = await fetch('./benchmark_report.json');
    if (resp.ok) {
      const json = await resp.json();
      populateLeaderboardTable(json);
      return;
    }
  } catch (e) {
    console.warn('Using default benchmark data:', e);
  }
  populateLeaderboardTable(defaultBenchmarkData);
}

let currentLocale = 'en';
const DISPLAY_MODES = ['ascii', 'crt', 'canvas'];
let displayMode = 'ascii';
let crtMode = false;
let soundEnabled = true;
let audioCtx = null;
let canvasEl = null;
let canvasCtx = null;

function initAudio() {
  if (!audioCtx) {
    const AudioCtx = window.AudioContext || window.webkitAudioContext;
    if (AudioCtx) {
      audioCtx = new AudioCtx();
    }
  }
  if (audioCtx && audioCtx.state === 'suspended') {
    audioCtx.resume();
  }
}

function playSfx(type) {
  if (!soundEnabled) return;
  initAudio();
  if (!audioCtx) return;

  const now = audioCtx.currentTime;
  const osc = audioCtx.createOscillator();
  const gain = audioCtx.createGain();
  osc.connect(gain);
  gain.connect(audioCtx.destination);

  switch (type) {
    case 'step':
      osc.type = 'triangle';
      osc.frequency.setValueAtTime(120, now);
      osc.frequency.exponentialRampToValueAtTime(40, now + 0.04);
      gain.gain.setValueAtTime(0.06, now);
      gain.gain.linearRampToValueAtTime(0.001, now + 0.04);
      osc.start(now);
      osc.stop(now + 0.04);
      break;
    case 'hit':
      osc.type = 'sawtooth';
      osc.frequency.setValueAtTime(160, now);
      osc.frequency.exponentialRampToValueAtTime(40, now + 0.12);
      gain.gain.setValueAtTime(0.2, now);
      gain.gain.linearRampToValueAtTime(0.001, now + 0.12);
      osc.start(now);
      osc.stop(now + 0.12);
      break;
    case 'zap':
      osc.type = 'sawtooth';
      osc.frequency.setValueAtTime(900, now);
      osc.frequency.exponentialRampToValueAtTime(120, now + 0.22);
      gain.gain.setValueAtTime(0.18, now);
      gain.gain.linearRampToValueAtTime(0.001, now + 0.22);
      osc.start(now);
      osc.stop(now + 0.22);
      break;
    case 'potion':
      osc.type = 'sine';
      osc.frequency.setValueAtTime(340, now);
      osc.frequency.setValueAtTime(440, now + 0.05);
      osc.frequency.setValueAtTime(554.37, now + 0.1);
      osc.frequency.setValueAtTime(659.25, now + 0.15);
      gain.gain.setValueAtTime(0.14, now);
      gain.gain.linearRampToValueAtTime(0.001, now + 0.22);
      osc.start(now);
      osc.stop(now + 0.22);
      break;
    case 'pray':
      osc.type = 'sine';
      osc.frequency.setValueAtTime(261.63, now);
      osc.frequency.exponentialRampToValueAtTime(523.25, now + 0.35);
      gain.gain.setValueAtTime(0.2, now);
      gain.gain.linearRampToValueAtTime(0.001, now + 0.4);
      osc.start(now);
      osc.stop(now + 0.4);
      break;
    case 'ward':
      osc.type = 'sine';
      osc.frequency.setValueAtTime(587.33, now);
      osc.frequency.exponentialRampToValueAtTime(880, now + 0.2);
      gain.gain.setValueAtTime(0.16, now);
      gain.gain.linearRampToValueAtTime(0.001, now + 0.25);
      osc.start(now);
      osc.stop(now + 0.25);
      break;
    case 'dip':
      osc.type = 'triangle';
      osc.frequency.setValueAtTime(420, now);
      osc.frequency.exponentialRampToValueAtTime(620, now + 0.08);
      osc.frequency.exponentialRampToValueAtTime(280, now + 0.16);
      gain.gain.setValueAtTime(0.15, now);
      gain.gain.linearRampToValueAtTime(0.001, now + 0.2);
      osc.start(now);
      osc.stop(now + 0.2);
      break;
  }
}

function applyDisplayMode() {
  const isUk = currentLocale === 'uk';
  const btn = document.getElementById('btnDisplayMode');
  if (!canvasEl) {
    canvasEl = document.getElementById('tileCanvas');
    if (canvasEl) canvasCtx = canvasEl.getContext('2d');
  }

  if (displayMode === 'ascii') {
    crtMode = false;
    document.body.classList.remove('crt-mode');
    if (viewportEl) viewportEl.style.display = 'block';
    if (canvasEl) canvasEl.style.display = 'none';
    btn.textContent = isUk ? '📺 Режим: ASCII' : '📺 Mode: ASCII';
  } else if (displayMode === 'crt') {
    crtMode = true;
    document.body.classList.add('crt-mode');
    if (viewportEl) viewportEl.style.display = 'block';
    if (canvasEl) canvasEl.style.display = 'none';
    btn.textContent = isUk ? '📺 Режим: CRT' : '📺 Mode: CRT';
  } else { // canvas
    crtMode = false;
    document.body.classList.remove('crt-mode');
    if (viewportEl) viewportEl.style.display = 'none';
    if (canvasEl) canvasEl.style.display = 'block';
    btn.textContent = isUk ? '🎮 Режим: 2D Тайли' : '🎮 Mode: 2D Tiles';
  }
  if (session) {
    updateUi();
  }
}

function toggleDisplayMode() {
  const idx = DISPLAY_MODES.indexOf(displayMode);
  displayMode = DISPLAY_MODES[(idx + 1) % DISPLAY_MODES.length];
  applyDisplayMode();
}

function renderCanvas(data) {
  if (!canvasEl) {
    canvasEl = document.getElementById('tileCanvas');
    if (canvasEl) canvasCtx = canvasEl.getContext('2d');
  }
  if (!canvasCtx || !data || !data.tiles) return;

  const ctx = canvasCtx;
  const T_W = 12;
  const T_H = 24;

  ctx.fillStyle = '#080a0f';
  ctx.fillRect(0, 0, 960, 504);

  const tiles = data.tiles;
  const playerCoord = data.player_coord; // [x, y]

  for (let y = 0; y < 21; y++) {
    for (let x = 0; x < 80; x++) {
      const tile = tiles[y] ? tiles[y][x] : null;
      if (!tile) continue;

      const px = x * T_W;
      const py = y * T_H;

      switch (tile.kind) {
        case 'stone':
          ctx.fillStyle = '#0a0d14';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#05070a';
          ctx.fillRect(px + 2, py + 4, 2, 2);
          ctx.fillRect(px + 8, py + 14, 2, 2);
          break;
        case 'wall_h':
          ctx.fillStyle = '#3f465c';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#6272a4';
          ctx.fillRect(px, py, T_W, 2);
          ctx.fillStyle = '#282a36';
          ctx.fillRect(px, py + T_H - 2, T_W, 2);
          ctx.fillStyle = '#212534';
          ctx.fillRect(px + 5, py + 2, 1, T_H - 4);
          break;
        case 'wall_v':
          ctx.fillStyle = '#383e52';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#6272a4';
          ctx.fillRect(px, py, 2, T_H);
          ctx.fillStyle = '#212534';
          ctx.fillRect(px + T_W - 2, py, 2, T_H);
          ctx.fillStyle = '#282a36';
          ctx.fillRect(px + 2, py + 11, T_W - 4, 1);
          break;
        case 'room':
          ctx.fillStyle = '#1b1e2b';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.strokeStyle = '#232738';
          ctx.lineWidth = 1;
          ctx.strokeRect(px + 0.5, py + 0.5, T_W - 1, T_H - 1);
          break;
        case 'corr':
          ctx.fillStyle = '#141620';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#222634';
          ctx.fillRect(px + 2, py + 3, 2, 2);
          ctx.fillRect(px + 7, py + 8, 2, 2);
          ctx.fillRect(px + 3, py + 15, 2, 2);
          ctx.fillRect(px + 8, py + 19, 2, 2);
          break;
        case 'door_open':
          ctx.fillStyle = '#1b1e2b';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#8d6e63';
          ctx.fillRect(px, py, 2, T_H);
          ctx.fillRect(px + T_W - 2, py, 2, T_H);
          break;
        case 'door_closed':
          ctx.fillStyle = '#5d4037';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#8d6e63';
          ctx.fillRect(px + 2, py + 2, T_W - 4, T_H - 4);
          ctx.fillStyle = '#d7ccc8';
          ctx.fillRect(px + 3, py + 11, 2, 2);
          break;
        case 'door_broken':
          ctx.fillStyle = '#1b1e2b';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#4e342e';
          ctx.fillRect(px + 2, py + 4, 3, 5);
          ctx.fillRect(px + 6, py + 12, 4, 4);
          break;
        case 'stairs_up':
          ctx.fillStyle = '#1b1e2b';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#8be9fd';
          ctx.fillRect(px + 2, py + 16, 8, 4);
          ctx.fillRect(px + 3, py + 10, 6, 4);
          ctx.fillRect(px + 4, py + 4, 4, 4);
          break;
        case 'stairs_down':
          ctx.fillStyle = '#0e1017';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#ffb86c';
          ctx.fillRect(px + 2, py + 4, 8, 4);
          ctx.fillRect(px + 3, py + 10, 6, 4);
          ctx.fillRect(px + 4, py + 16, 4, 4);
          break;
        case 'pit':
          ctx.fillStyle = '#050608';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.strokeStyle = '#ff5555';
          ctx.lineWidth = 1;
          ctx.strokeRect(px + 2, py + 4, T_W - 4, T_H - 8);
          break;
        case 'altar_lawful':
        case 'altar_neutral':
        case 'altar_chaotic':
          ctx.fillStyle = '#1b1e2b';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#e0e0e0';
          ctx.fillRect(px + 2, py + 6, T_W - 4, T_H - 12);
          const auraColor = tile.kind === 'altar_lawful' ? '#8be9fd' : (tile.kind === 'altar_neutral' ? '#f1fa8c' : '#ff5555');
          ctx.fillStyle = auraColor;
          ctx.fillRect(px + 4, py + 9, T_W - 8, T_H - 18);
          break;
        case 'bridge_open':
        case 'bridge_closed':
          ctx.fillStyle = tile.kind === 'bridge_open' ? '#1b1e2b' : '#3e2723';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.strokeStyle = '#8d6e63';
          ctx.lineWidth = 1;
          ctx.strokeRect(px + 1, py + 2, T_W - 2, T_H - 4);
          break;
        case 'water':
          ctx.fillStyle = '#1565c0';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#42a5f5';
          ctx.fillRect(px + 2, py + 5, 4, 2);
          ctx.fillRect(px + 6, py + 14, 4, 2);
          break;
        case 'ice':
          ctx.fillStyle = '#80deea';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#e0f7fa';
          ctx.fillRect(px + 3, py + 6, 3, 2);
          ctx.fillRect(px + 6, py + 15, 3, 2);
          break;
        case 'lava':
          ctx.fillStyle = '#d84315';
          ctx.fillRect(px, py, T_W, T_H);
          ctx.fillStyle = '#ffeb3b';
          ctx.fillRect(px + 2, py + 4, 3, 3);
          ctx.fillRect(px + 7, py + 12, 3, 3);
          ctx.fillStyle = '#ff5722';
          ctx.fillRect(px + 3, py + 17, 5, 2);
          break;
        default:
          ctx.fillStyle = '#1b1e2b';
          ctx.fillRect(px, py, T_W, T_H);
      }
    }
  }

  // Draw floor items
  if (data.items) {
    for (const item of data.items) {
      const ix = item.x * T_W;
      const iy = item.y * T_H;

      ctx.save();
      if (item.class === 'coin') {
        ctx.fillStyle = '#ffd700';
        ctx.beginPath();
        ctx.arc(ix + T_W / 2, iy + T_H / 2, 4, 0, Math.PI * 2);
        ctx.fill();
      } else if (item.class === 'potion') {
        ctx.fillStyle = '#ff4081';
        ctx.fillRect(ix + 3, iy + 10, 6, 8);
        ctx.fillStyle = '#fff';
        ctx.fillRect(ix + 5, iy + 6, 2, 4);
      } else if (item.class === 'scroll') {
        ctx.fillStyle = '#fff59d';
        ctx.fillRect(ix + 2, iy + 8, 8, 8);
        ctx.fillStyle = '#ff5252';
        ctx.fillRect(ix + 5, iy + 8, 2, 8);
      } else if (item.class === 'spellbook') {
        ctx.fillStyle = '#ba68c8';
        ctx.fillRect(ix + 2, iy + 6, 8, 11);
        ctx.fillStyle = '#ffd700';
        ctx.fillRect(ix + 4, iy + 9, 4, 2);
      } else if (item.class === 'wand') {
        ctx.strokeStyle = '#00e676';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(ix + 2, iy + T_H - 4);
        ctx.lineTo(ix + T_W - 2, iy + 4);
        ctx.stroke();
      } else if (item.class === 'amulet') {
        ctx.strokeStyle = '#ffd700';
        ctx.lineWidth = 1.5;
        ctx.strokeRect(ix + 3, iy + 7, 6, 7);
        ctx.fillStyle = '#e040fb';
        ctx.fillRect(ix + 4, iy + 12, 4, 4);
      } else if (item.class === 'weapon') {
        ctx.strokeStyle = '#81d4fa';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(ix + 3, iy + T_H - 4);
        ctx.lineTo(ix + T_W - 3, iy + 4);
        ctx.stroke();
        ctx.fillStyle = '#ffd700';
        ctx.fillRect(ix + 2, iy + T_H - 7, 3, 3);
      } else if (item.class === 'armor') {
        ctx.fillStyle = '#90a4ae';
        ctx.fillRect(ix + 2, iy + 6, 8, 10);
        ctx.fillStyle = '#607d8b';
        ctx.fillRect(ix + 4, iy + 8, 4, 6);
      } else {
        ctx.fillStyle = '#ffb74d';
        ctx.fillRect(ix + 3, iy + 8, 6, 6);
      }
      ctx.restore();
    }
  }

  // Draw actors (monsters & hero)
  if (data.actors) {
    for (const actor of data.actors) {
      const ax = actor.x * T_W;
      const ay = actor.y * T_H;

      ctx.save();
      if (actor.is_player) {
        // Hero: Golden aura & armored avatar
        ctx.fillStyle = 'rgba(255, 215, 0, 0.25)';
        ctx.beginPath();
        ctx.arc(ax + T_W / 2, ay + T_H / 2, 9, 0, Math.PI * 2);
        ctx.fill();

        // Hero sprite body & helmet
        ctx.fillStyle = '#ffd700';
        ctx.fillRect(ax + 2, ay + 4, 8, 6);
        ctx.fillStyle = '#ffffff';
        ctx.fillRect(ax + 3, ay + 6, 2, 2);
        ctx.fillRect(ax + 7, ay + 6, 2, 2);
        ctx.fillStyle = '#ffb300';
        ctx.fillRect(ax + 3, ay + 10, 6, 9);
        ctx.fillStyle = '#64b5f6';
        ctx.fillRect(ax + 9, ay + 7, 2, 10);
      } else {
        // Monster sprite
        const mname = actor.name.toLowerCase();
        let mcolor = '#ff5555';
        if (mname.includes('dragon')) mcolor = '#ff1744';
        else if (mname.includes('vampire')) mcolor = '#b71c1c';
        else if (mname.includes('skeleton')) mcolor = '#f5f5f5';
        else if (mname.includes('orc')) mcolor = '#388e3c';
        else if (mname.includes('goblin')) mcolor = '#8bc34a';
        else if (mname.includes('priest')) mcolor = '#ab47bc';
        else if (mname.includes('shopkeeper')) mcolor = '#ce93d8';
        else if (mname.includes('watchman')) mcolor = '#29b6f6';
        else if (mname.includes('dog') || mname.includes('cat') || mname.includes('kitten')) mcolor = '#8d6e63';

        ctx.fillStyle = mcolor;
        ctx.fillRect(ax + 2, ay + 5, 8, 12);
        ctx.fillStyle = '#000000';
        ctx.fillRect(ax + 3, ay + 7, 2, 2);
        ctx.fillRect(ax + 7, ay + 7, 2, 2);

        if (actor.hp < actor.max_hp) {
          ctx.fillStyle = '#ff5555';
          ctx.fillRect(ax + 1, ay + 1, 10, 2);
          const hpRatio = Math.max(0, Math.min(1, actor.hp / actor.max_hp));
          ctx.fillStyle = '#33ff66';
          ctx.fillRect(ax + 1, ay + 1, Math.floor(10 * hpRatio), 2);
        }
      }
      ctx.restore();
    }
  }

  // Fog of War / Dynamic lighting shading
  for (let y = 0; y < 21; y++) {
    for (let x = 0; x < 80; x++) {
      const tile = tiles[y] ? tiles[y][x] : null;
      if (!tile || !tile.visible) {
        ctx.fillStyle = 'rgba(8, 9, 12, 0.94)';
        ctx.fillRect(x * T_W, y * T_H, T_W, T_H);
      }
    }
  }

  // Soft lantern ambient vignette around hero
  if (playerCoord) {
    const plx = playerCoord[0] * T_W + T_W / 2;
    const ply = playerCoord[1] * T_H + T_H / 2;
    const rad = 72;
    const grad = ctx.createRadialGradient(plx, ply, 12, plx, ply, rad);
    grad.addColorStop(0, 'rgba(255, 238, 187, 0.08)');
    grad.addColorStop(0.7, 'rgba(255, 220, 150, 0.02)');
    grad.addColorStop(1, 'rgba(0, 0, 0, 0)');
    ctx.fillStyle = grad;
    ctx.beginPath();
    ctx.arc(plx, ply, rad, 0, Math.PI * 2);
    ctx.fill();
  }
}

function toggleSound() {
  soundEnabled = !soundEnabled;
  document.getElementById('btnToggleSound').textContent = soundEnabled
    ? (currentLocale === 'uk' ? '🔊 Звук: УВІМК' : '🔊 SFX: ON')
    : (currentLocale === 'uk' ? '🔇 Звук: ВИМК' : '🔇 SFX: OFF');
}

const inventoryDialog = document.getElementById('inventoryDialog');
const engraveDialog = document.getElementById('engraveDialog');
let selectedInvIndex = 0;
let cachedInventory = [];

function openInventoryDialog() {
  if (!session) return;
  renderInventoryTable();
  inventoryDialog.showModal();
}

function renderInventoryTable() {
  if (!session || !session.get_inventory_json) return;
  try {
    cachedInventory = JSON.parse(session.get_inventory_json());
  } catch (e) {
    cachedInventory = [];
  }
  const tbody = document.getElementById('invTableBody');
  tbody.innerHTML = '';

  if (cachedInventory.length === 0) {
    tbody.innerHTML = '<tr><td colspan="7" style="text-align:center; color:var(--text-dim);">Inventory is empty.</td></tr>';
    document.getElementById('invSelectedItemName').textContent = 'None';
    document.getElementById('invSelectedMeta').textContent = '';
    return;
  }

  if (selectedInvIndex >= cachedInventory.length) {
    selectedInvIndex = 0;
  }

  cachedInventory.forEach((item, idx) => {
    const tr = document.createElement('tr');
    tr.className = `inv-row ${idx === selectedInvIndex ? 'selected' : ''}`;
    const bucClass = item.buc === 'Blessed' ? 'buc-blessed' : (item.buc === 'Cursed' ? 'buc-cursed' : 'buc-uncursed');
    const costStr = item.unpaid_cost !== null && item.unpaid_cost !== undefined ? `${item.unpaid_cost} zm (unpaid)` : '-';
    tr.innerHTML = `
      <td>${idx}</td>
      <td style="font-weight:bold;">${escapeHtml(item.name)}</td>
      <td>${escapeHtml(item.class)}</td>
      <td class="${bucClass}">${escapeHtml(item.buc)}</td>
      <td>${item.enchantment >= 0 ? '+' : ''}${item.enchantment}</td>
      <td>${item.weight}</td>
      <td style="color:${item.unpaid_cost ? 'var(--text-yellow)' : 'var(--text-dim)'};">${costStr}</td>
    `;
    tr.addEventListener('click', () => {
      selectInvItem(idx);
    });
    tbody.appendChild(tr);
  });

  selectInvItem(selectedInvIndex);
}

function selectInvItem(idx) {
  selectedInvIndex = idx;
  const rows = document.querySelectorAll('.inv-row');
  rows.forEach((r, i) => {
    r.classList.toggle('selected', i === idx);
  });
  const item = cachedInventory[idx];
  if (item) {
    document.getElementById('invSelectedItemName').textContent = item.name;
    document.getElementById('invSelectedMeta').textContent = `Class: ${item.class} | BUC: ${item.buc} | Wt: ${item.weight}`;
  } else {
    document.getElementById('invSelectedItemName').textContent = 'None';
    document.getElementById('invSelectedMeta').textContent = '';
  }
}

function toggleLanguage() {
  currentLocale = currentLocale === 'en' ? 'uk' : 'en';
  if (session && session.set_locale) {
    session.set_locale(currentLocale);
  }
  applyLocalization(currentLocale);
  updateUi(null);
}

function applyLocalization(locale) {
  const isUk = locale === 'uk';
  document.getElementById('btnToggleLang').textContent = isUk ? '🌐 УКР (Змінити на EN)' : '🌐 EN / УКР';
  document.getElementById('btnToggleSound').textContent = soundEnabled ? (isUk ? '🔊 Звук: УВІМК' : '🔊 SFX: ON') : (isUk ? '🔇 Звук: ВИМК' : '🔇 SFX: OFF');
  applyDisplayMode();

  document.getElementById('lblHero').textContent = isUk ? 'Герой:' : 'Hero:';
  document.getElementById('lblAlign').textContent = isUk ? 'Шлях:' : 'Align:';
  document.getElementById('lblDlvl').textContent = isUk ? 'Рівень:' : 'Dlvl:';
  document.getElementById('lblGold').textContent = isUk ? 'Золото:' : 'Gold:';
  document.getElementById('lblHp').textContent = isUk ? 'Здоров\'я:' : 'HP:';
  document.getElementById('lblPw').textContent = isUk ? 'Мана:' : 'Pw:';
  document.getElementById('lblAc').textContent = isUk ? 'Броня:' : 'AC:';
  document.getElementById('lblNutr').textContent = isUk ? 'Ситість:' : 'Nutr:';
  document.getElementById('lblTurn').textContent = isUk ? 'Хід:' : 'Turn:';
  document.getElementById('lblDirectControls').textContent = isUk ? 'Керування' : 'Direct Controls';

  document.getElementById('btnOpenInv').textContent = isUk ? '🎒 [i] Інвентар' : '🎒 [i] Inventory';
  document.getElementById('btnActEat').textContent = isUk ? '[e] Їсти' : '[e] Eat';
  document.getElementById('btnActQuaff').textContent = isUk ? '[q] Пити' : '[q] Quaff';
  document.getElementById('btnActCast').textContent = isUk ? '[x] Закляття' : '[x] Cast';
  document.getElementById('btnActRead').textContent = isUk ? '[r] Читати' : '[r] Read';
  document.getElementById('btnActPickup').textContent = isUk ? '[,] Підняти' : '[,] PickUp';
  document.getElementById('btnActPay').textContent = isUk ? '[p] Платити' : '[p] Pay';
  document.getElementById('btnActPray').textContent = isUk ? '[P] Молитися' : '[P] Pray';
  document.getElementById('btnActSacrifice').textContent = isUk ? '[S] Жертва' : '[S] Sacrifice';
  document.getElementById('btnActWield').textContent = isUk ? '[w] Зброя' : '[w] Wield';
  document.getElementById('btnActDrop').textContent = isUk ? '[d] Кинути' : '[d] Drop';
  document.getElementById('btnActRub').textContent = isUk ? '[R] Терти' : '[R] Rub';
  document.getElementById('btnActPriceCheck').textContent = isUk ? '[$] Оцінити' : '[$] Appraise';
  document.getElementById('btnActEngraveModal').textContent = isUk ? '[E] Викарбувати' : '[E] Engrave';
  document.getElementById('btnActDescend').textContent = isUk ? '[>] Вниз' : '[>] Descend';
  document.getElementById('btnActAscend').textContent = isUk ? '[<] Вгору' : '[<] Ascend';

  document.getElementById('btnLeaderboard').textContent = isUk ? '🏆 Таблиця лідерів & ШІ Арена' : '🏆 Leaderboard & AI Arena';
  document.getElementById('btnNewChar').textContent = isUk ? 'Персонаж' : 'Character';
  document.getElementById('btnReset').textContent = isUk ? 'Скинути (Сід 42)' : 'Reset (Seed 42)';

  // Character creation modal
  const lblCharCreation = document.getElementById('lblCharCreation');
  if (lblCharCreation) lblCharCreation.textContent = isUk ? 'Створення персонажа' : 'Character Creation';
  const lblCharName = document.getElementById('lblCharName');
  if (lblCharName) lblCharName.textContent = isUk ? 'Ім\'я героя:' : 'Character Name:';
  document.getElementById('lblRuleset').textContent = isUk ? 'Набір правил:' : 'Ruleset:';
  document.getElementById('lnkManagePacks').textContent = isUk ? 'Керувати наборами…' : 'Manage packs…';
  document.getElementById('lblHudRuleset').textContent = isUk ? 'Правила:' : 'Rules:';
  const lblChooseRole = document.getElementById('lblChooseRole');
  if (lblChooseRole) lblChooseRole.textContent = isUk ? 'Оберіть клас / роль:' : 'Choose Class / Role:';
  const btnStartGame = document.getElementById('btnStartGame');
  if (btnStartGame) btnStartGame.textContent = isUk ? 'Вирушити в підземелля' : 'Embark Into Dungeon';

  const roleValk = document.getElementById('roleBtnValkyrie');
  if (roleValk) roleValk.textContent = isUk ? 'Валькірія (Людина)' : 'Valkyrie (Human)';
  const roleWiz = document.getElementById('roleBtnWizard');
  if (roleWiz) roleWiz.textContent = isUk ? 'Маг (Людина)' : 'Wizard (Human)';
  const roleBarb = document.getElementById('roleBtnBarbarian');
  if (roleBarb) roleBarb.textContent = isUk ? 'Варвар (Орк)' : 'Barbarian (Orc)';
  const roleRog = document.getElementById('roleBtnRogue');
  if (roleRog) roleRog.textContent = isUk ? 'Розбійник (Людина)' : 'Rogue (Human)';
  const roleKni = document.getElementById('roleBtnKnight');
  if (roleKni) roleKni.textContent = isUk ? 'Лицар (Дворф)' : 'Knight (Dwarf)';
  const roleMonk = document.getElementById('roleBtnMonk');
  if (roleMonk) roleMonk.textContent = isUk ? 'Монах (Людина)' : 'Monk (Human)';
  const roleHeal = document.getElementById('roleBtnHealer');
  if (roleHeal) roleHeal.textContent = isUk ? 'Цілитель (Гном)' : 'Healer (Gnome)';
  const roleTour = document.getElementById('roleBtnTourist');
  if (roleTour) roleTour.textContent = isUk ? 'Турист (Людина)' : 'Tourist (Human)';
  const roleArch = document.getElementById('roleBtnArchaeologist');
  if (roleArch) roleArch.textContent = isUk ? 'Археолог (Людина)' : 'Archaeologist (Human)';

  // Inventory & Engrave modals
  const lblInvHeader = document.getElementById('lblInvHeader');
  if (lblInvHeader) lblInvHeader.textContent = isUk ? '🎒 Інвентар героя' : '🎒 Hero Inventory';
  const lblInvSubtext = document.getElementById('lblInvSubtext');
  if (lblInvSubtext) lblInvSubtext.textContent = isUk ? 'Оберіть предмет для дій або огляду:' : 'Select an item to inspect or perform an action:';
  const lblEngraveHeader = document.getElementById('lblEngraveHeader');
  if (lblEngraveHeader) lblEngraveHeader.textContent = isUk ? '✒️ Викарбувати на поросі' : '✒️ Engrave in the Dust';
  const lblEngravePrompt = document.getElementById('lblEngravePrompt');
  if (lblEngravePrompt) lblEngravePrompt.textContent = isUk ? 'Що ви бажаєте написати пальцем?' : 'What do you want to engrave with your fingertips?';
  const lblEngraveHelp = document.getElementById('lblEngraveHelp');
  if (lblEngraveHelp) lblEngraveHelp.innerHTML = isUk ? 'Напис <strong style="color:var(--text-yellow);">Elbereth</strong> захищає клітинку та відлякує не-гуманоїдних монстрів!' : 'Writing <strong style="color:var(--text-yellow);">Elbereth</strong> wards the tile and prevents non-humanoid monsters from attacking!';

  if (isUk) {
    messageBarEl.textContent = 'Ласкаво просимо до NetRust! Формалізовано та математично доведено в Lean 4.';
  } else {
    messageBarEl.textContent = 'Welcome to NetRust! Core mechanics modeled in Lean 4.';
  }
}

function updateUi(lastObs) {
  if (!session) return;
  viewportEl.textContent = session.render_ascii();
  if (displayMode === 'canvas' && session.get_canvas_render_data_json) {
    try {
      const cData = JSON.parse(session.get_canvas_render_data_json());
      renderCanvas(cData);
    } catch (err) {
      console.error("Canvas render error:", err);
    }
  }
  statNameEl.textContent = session.get_player_name();
  statAlignEl.textContent = session.get_localized_alignment ? session.get_localized_alignment() : session.get_player_alignment();
  statDepthEl.textContent = session.get_depth();
  statGoldEl.textContent = session.get_player_gold();
  statHpEl.textContent = `${session.get_player_hp()}(${session.get_player_max_hp()})`;
  statPwEl.textContent = `${session.get_player_pw()}(${session.get_player_max_pw()})`;
  statAcEl.textContent = session.get_player_ac();
  statNutrEl.textContent = `${session.get_player_nutrition()} (${session.get_localized_hunger_state ? session.get_localized_hunger_state() : session.get_hunger_state()})`;
  statTurnEl.textContent = session.get_turn();

  const obsJson = session.get_observation_json();
  obsJsonEl.value = obsJson;

  if (lastObs && lastObs.last_events && lastObs.last_events.length > 0) {
    const lastMsg = lastObs.last_events
      .filter(e => e.LogMessage)
      .map(e => e.LogMessage.text)
      .pop();
    if (lastMsg) {
      messageBarEl.textContent = lastMsg;
    }
  }
}

function updateProgress(done, total) {
  const tb = document.getElementById('tournamentTableBody');
  tb.innerHTML = `<tr><td colspan="7" style="text-align:center; color:#d6a3ff;">Running seed ${Number(done)} of ${Number(total)}...</td></tr>`;
}

function escapeHtml(s) { return String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }

let lastDir = 'east';

function doAction(action, arg) {
  if (!session) return null;
  // Directional spells/wands use the last movement direction.
  if ((action === 'cast' || action === 'zap') && (arg === null || arg === undefined || /^\d+$/.test(arg))) {
    arg = lastDir;
  }
  // Sound FX trigger
  if (action === 'move') playSfx('step');
  else if (action === 'zap') playSfx('zap');
  else if (action === 'quaff' || action === 'eat') playSfx('potion');
  else if (action === 'pray' || action === 'sacrifice') playSfx('pray');
  else if (action === 'engrave') playSfx('ward');
  else if (action === 'dip') playSfx('dip');

  const resStr = session.step(action, arg ? arg : null);
  try {
    const res = JSON.parse(resStr);
    if (res.error) {
      messageBarEl.textContent = res.error;
      return null;
    }
    if (action === 'move' && arg) lastDir = arg;
    if (res.last_events && res.last_events.some(e => e.AttackLanded)) {
      playSfx('hit');
    }
    updateUi(res);
    if (inventoryDialog.open) {
      renderInventoryTable();
    }
    return res;
  } catch (e) {
    updateUi(null);
    return null;
  }
}

// AI Policy Decider
function computeAiDecision(policy, obs) {
  const directions = ['north', 'east', 'south', 'west', 'northeast', 'southeast', 'southwest', 'northwest'];

  if (policy === 'RandomBaseline') {
    const rnd = Math.floor(Math.random() * 12);
    if (rnd < 8) return { act: 'move', arg: directions[rnd], reason: 'Random walk' };
    if (rnd === 8) return { act: 'wait', arg: null, reason: 'Wait' };
    if (rnd === 9) return { act: 'descend', arg: null, reason: 'Descend stairs' };
    if (rnd === 10) return { act: 'pickup', arg: null, reason: 'Pick up' };
    return { act: 'pray', arg: null, reason: 'Pray' };
  }

  if (policy === 'PetTesterTactical') {
    if (obs.player_hp <= Math.floor(obs.player_max_hp * 0.35)) {
      return { act: 'engrave', arg: 'Elbereth', reason: 'Critical HP! Engrave Elbereth dust ward' };
    }
    if (obs.visible_actors && obs.visible_actors.length > 0) {
      const hostile = obs.visible_actors.find(a => !a.is_player && !a.name.toLowerCase().includes('dog') && !a.name.toLowerCase().includes('cat') && !a.name.toLowerCase().includes('kitten'));
      if (hostile) {
        const dx = hostile.coord.x - obs.player_coord.x;
        const dy = hostile.coord.y - obs.player_coord.y;
        if (Math.abs(dx) <= 1 && Math.abs(dy) <= 1) {
          const dir = getDirFromDelta(dx, dy);
          if (dir) return { act: 'move', arg: dir, reason: `Engage ${hostile.name}` };
        }
      }
    }
    if (arenaStep % 7 === 0) return { act: 'pickup', arg: null, reason: 'Pet-tested BUC item pickup' };
    if (arenaStep % 10 === 0) return { act: 'descend', arg: null, reason: 'Dijkstra branch descent' };
    return { act: 'move', arg: directions[arenaStep % 8], reason: 'Tactical exploration' };
  }

  if (policy === 'SurvivalHeuristic') {
    if (obs.player_hp <= 6) {
      return { act: 'pray', arg: null, reason: 'Emergency prayer (low HP)' };
    }
    if (obs.visible_actors && obs.visible_actors.length > 0) {
      const hostile = obs.visible_actors.find(a => !a.is_player);
      if (hostile) {
        const dx = hostile.coord.x - obs.player_coord.x;
        const dy = hostile.coord.y - obs.player_coord.y;
        if (Math.abs(dx) <= 1 && Math.abs(dy) <= 1) {
          const dir = getDirFromDelta(dx, dy);
          if (dir) return { act: 'move', arg: dir, reason: `Engage ${hostile.name}` };
        }
      }
    }
    if (arenaStep % 12 === 0) return { act: 'descend', arg: null, reason: 'Try descending stairs' };
    if (arenaStep % 5 === 0) return { act: 'pickup', arg: null, reason: 'Scan floor for loot' };
    return { act: 'move', arg: directions[arenaStep % 8], reason: 'Cautious patrol' };
  }

  if (policy === 'AmuletSpeedrunner') {
    if (arenaStep % 8 === 0) {
      return { act: 'descend', arg: null, reason: 'Speedrun descent (>)' };
    }
    return { act: 'move', arg: directions[(arenaStep * 2) % 8], reason: 'Fast stair hunt' };
  }

  // RLPolicy (Trained Neural Network)
  if (policy === 'RLPolicy') {
    return evaluateNeuralPolicy(obs);
  }
  return { act: 'wait', arg: null, reason: 'Default wait' };
}

let trainedPolicyWeights = null;

async function loadPolicyWeights() {
  try {
    const resp = await fetch('policy_weights.json');
    if (resp.ok) {
      trainedPolicyWeights = await resp.json();
      console.log('Loaded trained neural policy weights.');
    }
  } catch (e) {
    console.warn('Could not fetch policy_weights.json, using built-in neural fallback:', e);
  }
}

function evaluateNeuralPolicy(obs) {
  const pCoord = obs.player_coord || { x: 40, y: 12 };
  const hp = obs.player_hp || 18;
  const maxHp = obs.player_max_hp || 18;
  const ac = obs.player_ac || 7;
  const depth = obs.depth || 1;
  const gold = obs.player_gold || 50;
  const nutr = obs.player_nutrition || 900;
  const pw = obs.player_pw || 5;
  const turn = obs.turn || 1;
  const hostiles = (obs.visible_actors || []).filter(a => !a.is_player).length;

  const x = [
    hp / Math.max(1, maxHp),
    hp <= 6 ? 1.0 : 0.0,
    ac / 10.0,
    depth / 10.0,
    Math.min(5.0, gold / 100.0),
    nutr / 1000.0,
    pw / 20.0,
    Math.min(2.0, hostiles),
    pCoord.x / 80.0,
    pCoord.y / 24.0,
    Math.sin(turn * 0.1),
    Math.cos(turn * 0.1)
  ];

  if (!trainedPolicyWeights || !trainedPolicyWeights.w1) {
    if (hp <= 6) return { act: 'pray', arg: null, reason: 'Neural fallback: Emergency prayer' };
    if (turn % 10 === 0) return { act: 'descend', arg: null, reason: 'Neural fallback: Try descend' };
    const dirs = ['north', 'east', 'south', 'west', 'northeast', 'southeast', 'southwest', 'northwest'];
    return { act: 'move', arg: dirs[turn % 8], reason: 'Neural fallback: Explore' };
  }

  const w1 = trainedPolicyWeights.w1;
  const b1 = trainedPolicyWeights.b1;
  const w2 = trainedPolicyWeights.w2;
  const b2 = trainedPolicyWeights.b2;

  // Layer 1: ReLU(W1 * x + b1)
  const h = [];
  for (let i = 0; i < w1.length; i++) {
    let sum = b1[i];
    for (let j = 0; j < x.length; j++) {
      sum += w1[i][j] * x[j];
    }
    h.push(Math.max(0.0, sum));
  }

  // Layer 2: W2 * h + b2
  const logits = [];
  for (let i = 0; i < w2.length; i++) {
    let sum = b2[i];
    for (let j = 0; j < h.length; j++) {
      sum += w2[i][j] * h[j];
    }
    logits.push(sum);
  }

  // Argmax action
  let bestAct = 0;
  let maxLogit = logits[0];
  for (let i = 1; i < logits.length; i++) {
    if (logits[i] > maxLogit) {
      maxLogit = logits[i];
      bestAct = i;
    }
  }

  const actionNames = trainedPolicyWeights.action_names || [
    "North", "East", "South", "West",
    "NorthEast", "SouthEast", "SouthWest", "NorthWest",
    "Wait", "Descend", "Ascend", "PickUp", "Pay", "Pray", "EngraveElbereth"
  ];
  const chosenName = actionNames[bestAct];

  const compassMap = {
    "North": "north", "East": "east", "South": "south", "West": "west",
    "NorthEast": "northeast", "SouthEast": "southeast", "SouthWest": "southwest", "NorthWest": "northwest"
  };

  if (chosenName in compassMap) {
    return { act: 'move', arg: compassMap[chosenName], reason: `Neural Policy (Logit ${maxLogit.toFixed(2)}): ${chosenName}` };
  } else if (chosenName === "Wait") {
    return { act: 'wait', arg: null, reason: `Neural Policy (Logit ${maxLogit.toFixed(2)}): Wait` };
  } else if (chosenName === "Descend") {
    return { act: 'descend', arg: null, reason: `Neural Policy (Logit ${maxLogit.toFixed(2)}): Descend Stairs` };
  } else if (chosenName === "Ascend") {
    return { act: 'ascend', arg: null, reason: `Neural Policy (Logit ${maxLogit.toFixed(2)}): Ascend Stairs` };
  } else if (chosenName === "PickUp") {
    return { act: 'pickup', arg: null, reason: `Neural Policy (Logit ${maxLogit.toFixed(2)}): Pick Up Item` };
  } else if (chosenName === "Pray") {
    return { act: 'pray', arg: null, reason: `Neural Policy (Logit ${maxLogit.toFixed(2)}): Pray to Deity` };
  } else if (chosenName === "Pay") {
    return { act: 'pay', arg: null, reason: `Neural Policy (Logit ${maxLogit.toFixed(2)}): Pay Merchant` };
  } else if (chosenName === "EngraveElbereth") {
    return { act: 'wait', arg: null, reason: `Neural Policy (Logit ${maxLogit.toFixed(2)}): Ward Engraving` };
  }
  return { act: 'wait', arg: null, reason: `Neural Policy: Default wait` };
}

function getDirFromDelta(dx, dy) {
  if (dx === 0 && dy === -1) return 'north';
  if (dx === 1 && dy === 0) return 'east';
  if (dx === 0 && dy === 1) return 'south';
  if (dx === -1 && dy === 0) return 'west';
  if (dx === 1 && dy === -1) return 'northeast';
  if (dx === 1 && dy === 1) return 'southeast';
  if (dx === -1 && dy === 1) return 'southwest';
  if (dx === -1 && dy === -1) return 'northwest';
  return null;
}

function stepAiMatch() {
  if (!session) return;
  arenaStep++;
  document.getElementById('arenaStepCount').textContent = arenaStep;

  let obs;
  try {
    obs = JSON.parse(session.get_observation_json());
  } catch (e) {
    obs = {};
  }

  const policy = document.getElementById('arenaPolicySelect').value;
  const decision = computeAiDecision(policy, obs);

  document.getElementById('arenaDecision').textContent = `${decision.act.toUpperCase()} (${decision.arg || ''}) — ${decision.reason}`;

  const res = doAction(decision.act, decision.arg);

  const isDead = session.get_player_hp() === 0;
  const reachedMax = arenaStep >= 200;

  if (isDead || reachedMax) {
    stopAiMatch();
    const outcome = isDead ? 'DIED' : 'TRUNCATED (200t)';
    const pill = document.getElementById('arenaStatusPill');
    pill.className = isDead ? 'arena-status-pill pill-dead' : 'arena-status-pill pill-win';
    pill.textContent = outcome;

    recentMatches.unshift({
      time: new Date().toLocaleTimeString(),
      policy,
      seed: arenaSeed,
      steps: arenaStep,
      dlvl: session.get_depth(),
      outcome
    });
    updateRecentMatchesTable();
  }
}

function startAiMatch() {
  if (arenaTimer) return;
  const speed = parseInt(document.getElementById('arenaSpeedSelect').value, 10) || 120;
  document.getElementById('btnRunArenaMatch').style.display = 'none';
  document.getElementById('btnStopArenaMatch').style.display = 'inline-block';
  const pill = document.getElementById('arenaStatusPill');
  pill.className = 'arena-status-pill pill-running';
  pill.textContent = 'RUNNING';

  arenaTimer = setInterval(stepAiMatch, speed);
}

function stopAiMatch() {
  if (arenaTimer) {
    clearInterval(arenaTimer);
    arenaTimer = null;
  }
  document.getElementById('btnRunArenaMatch').style.display = 'inline-block';
  document.getElementById('btnStopArenaMatch').style.display = 'none';
}

function resetArenaMatch() {
  stopAiMatch();
  arenaStep = 0;
  arenaSeed = Math.floor(Math.random() * 10000);
  session = new WasmGameSession(BigInt(arenaSeed));
  document.getElementById('arenaStepCount').textContent = '0';
  document.getElementById('arenaDecision').textContent = 'Ready to begin.';
  const pill = document.getElementById('arenaStatusPill');
  pill.className = 'arena-status-pill pill-running';
  pill.textContent = 'READY';
  updateUi(null);
}

function updateRecentMatchesTable() {
  const tbody = document.getElementById('recentMatchesBody');
  tbody.innerHTML = '';
  if (recentMatches.length === 0) {
    tbody.innerHTML = '<tr><td colspan="6" style="text-align:center; color:var(--text-dim);">No matches run yet in this session.</td></tr>';
    return;
  }
  recentMatches.slice(0, 10).forEach(m => {
    const tr = document.createElement('tr');
    const outCol = m.outcome === 'DIED' ? 'var(--text-red)' : 'var(--text-green)';
    tr.innerHTML = `
      <td>${m.time}</td>
      <td style="color:var(--text-cyan); font-weight:bold;">${escapeHtml(m.policy)}</td>
      <td>${m.seed}</td>
      <td>${m.steps}</td>
      <td>${m.dlvl}</td>
      <td style="color:${outCol}; font-weight:bold;">${m.outcome}</td>
    `;
    tbody.appendChild(tr);
  });
}

// ---- Ruleset (rule pack) selection ----
function activeRulesetKey() {
  try {
    return localStorage.getItem(ACTIVE_KEY) || 'vanilla';
  } catch {
    return 'vanilla';
  }
}

async function populateRulesetSelect() {
  const sel = document.getElementById('rulesetSelect');
  const entries = await listPacks();
  const active = activeRulesetKey();
  sel.innerHTML = entries
    .map((e) => `<option value="${escapeHtml(e.key)}">${escapeHtml(e.title)}${e.version ? ' ' + escapeHtml(e.version) : ''}${e.origin === 'stored' ? ' (uploaded)' : ''}</option>`)
    .join('');
  sel.value = entries.some((e) => e.key === active) ? active : 'vanilla';
  sel.onchange = () => {
    try { localStorage.setItem(ACTIVE_KEY, sel.value); } catch { /* storage unavailable */ }
  };
}

// Create a session on the selected ruleset; vanilla keeps the original path.
async function createSession(seed, role, race, name) {
  const key = document.getElementById('rulesetSelect').value || activeRulesetKey();
  let s;
  if (key === 'vanilla') {
    s = WasmGameSession.new_with_character(seed, role, race, name);
  } else {
    const pack = await openPack(wasmModule, key);
    s = WasmGameSession.newWithPack(seed, role, race, name, pack);
  }
  const hudItem = document.getElementById('hudRulesetItem');
  hudItem.hidden = key === 'vanilla';
  document.getElementById('statRuleset').textContent =
    key === 'vanilla' ? '' : `${s.rulesetId()} · ${s.rulesetHash().slice(7, 19)}`;
  return s;
}

async function start() {
  await populateRulesetSelect();
  session = new WasmGameSession(42n);
  if (activeRulesetKey() !== 'vanilla') {
    try {
      session = await createSession(42n, selectedRole, selectedRace, 'Hero');
      messageBarEl.textContent = `Playing with rule pack: ${session.rulesetId()}`;
    } catch (err) {
      messageBarEl.textContent = `Could not load rule pack: ${err.message || err}`;
    }
  }
  applyDisplayMode();
  updateUi(null);
  loadLeaderboard();
  loadPolicyWeights();
  viewportEl.focus();
}

function execInvAction(act) {
  if (cachedInventory.length === 0) return;
  doAction(act, String(selectedInvIndex));
  renderInventoryTable();
}

function execDipAction(waterType) {
  if (cachedInventory.length === 0) return;
  doAction('dip', `${selectedInvIndex}:${waterType}`);
  renderInventoryTable();
}

function openEngraveDialog() {
  engraveDialog.showModal();
}

function confirmEngrave() {
  const text = document.getElementById('engraveText').value.trim() || 'Elbereth';
  doAction('engrave', text);
  engraveDialog.close();
}

// Keybindings
window.addEventListener('keydown', (e) => {
  if (document.activeElement === document.getElementById('charName') ||
      document.activeElement === document.getElementById('engraveText') ||
      charDialog.open || leaderboardDialog.open || inventoryDialog.open || engraveDialog.open) {
    return;
  }

  switch (e.key) {
    case 'h': case 'ArrowLeft': doAction('move', 'west'); break;
    case 'l': case 'ArrowRight': doAction('move', 'east'); break;
    case 'k': case 'ArrowUp': doAction('move', 'north'); break;
    case 'j': case 'ArrowDown': doAction('move', 'south'); break;
    case 'y': doAction('move', 'northwest'); break;
    case 'u': doAction('move', 'northeast'); break;
    case 'b': doAction('move', 'southwest'); break;
    case 'n': doAction('move', 'southeast'); break;
    case '.': case ' ': doAction('wait', null); break;
    case ',': doAction('pickup', null); break;
    case 'i': openInventoryDialog(); break;
    case 'e': doAction('eat', '0'); break;
    case 'q': doAction('quaff', '0'); break;
    case 'x': doAction('cast', lastDir); break;
    case 'r': doAction('read', '0'); break;
    case 'E': openEngraveDialog(); break;
    case 'p': doAction('pay', null); break;
    case 'P': doAction('pray', null); break;
    case 'S': doAction('sacrifice', '0'); break;
    case 'w': doAction('wield', '0'); break;
    case 'd': doAction('drop', '0'); break;
    case '>': doAction('descend', null); break;
    case '<': doAction('ascend', null); break;
  }
});

// Button clicks
document.querySelectorAll('[data-act]').forEach(btn => {
  btn.addEventListener('click', () => {
    doAction(btn.dataset.act, btn.dataset.arg || null);
  });
});

// Sound and Display toggles
document.getElementById('btnToggleSound').addEventListener('click', toggleSound);
document.getElementById('btnDisplayMode').addEventListener('click', toggleDisplayMode);

// Inventory and Engrave modal triggers
document.getElementById('btnOpenInv').addEventListener('click', openInventoryDialog);
document.getElementById('btnCloseInventory').addEventListener('click', () => inventoryDialog.close());
document.getElementById('btnActEngraveModal').addEventListener('click', openEngraveDialog);
document.getElementById('btnCloseEngrave').addEventListener('click', () => engraveDialog.close());
document.getElementById('btnConfirmEngrave').addEventListener('click', confirmEngrave);

// Inventory modal action buttons
document.getElementById('btnInvWield').addEventListener('click', () => execInvAction('wield'));
document.getElementById('btnInvQuaff').addEventListener('click', () => execInvAction('quaff'));
document.getElementById('btnInvRead').addEventListener('click', () => execInvAction('read'));
document.getElementById('btnInvEat').addEventListener('click', () => execInvAction('eat'));
document.getElementById('btnInvDrop').addEventListener('click', () => execInvAction('drop'));
document.getElementById('btnInvRub').addEventListener('click', () => execInvAction('rub'));
document.getElementById('btnInvPriceCheck').addEventListener('click', () => execInvAction('price_check'));
document.getElementById('btnDipHoly').addEventListener('click', () => execDipAction('holy'));
document.getElementById('btnDipPlain').addEventListener('click', () => execDipAction('plain'));
document.getElementById('btnDipUnholy').addEventListener('click', () => execDipAction('unholy'));

// Language toggle
document.getElementById('btnToggleLang').addEventListener('click', toggleLanguage);

// Dialog controls
document.getElementById('btnNewChar').addEventListener('click', () => {
  charDialog.showModal();
});

document.getElementById('btnCloseDialog').addEventListener('click', () => {
  charDialog.close();
});

document.getElementById('btnLeaderboard').addEventListener('click', () => {
  leaderboardDialog.showModal();
});

document.getElementById('btnCloseLeaderboard').addEventListener('click', () => {
  stopAiMatch();
  leaderboardDialog.close();
});

// Tabs inside Leaderboard Dialog
document.getElementById('tabStandingsBtn').addEventListener('click', () => {
  document.getElementById('tabStandingsBtn').classList.add('active');
  document.getElementById('tabArenaBtn').classList.remove('active');
  document.getElementById('tabStandingsContent').style.display = 'block';
  document.getElementById('tabArenaContent').style.display = 'none';
});

document.getElementById('tabArenaBtn').addEventListener('click', () => {
  document.getElementById('tabArenaBtn').classList.add('active');
  document.getElementById('tabStandingsBtn').classList.remove('active');
  document.getElementById('tabStandingsContent').style.display = 'none';
  document.getElementById('tabArenaContent').style.display = 'block';
});

// Arena buttons
document.getElementById('btnRunArenaMatch').addEventListener('click', startAiMatch);
document.getElementById('btnStopArenaMatch').addEventListener('click', stopAiMatch);
document.getElementById('btnResetArenaMatch').addEventListener('click', resetArenaMatch);

document.getElementById('btnRunTournament').addEventListener('click', async () => {
  const runBtn = document.getElementById('btnRunTournament');
  if (runBtn.disabled) return;
  runBtn.disabled = true;
  let run = null;
  const container = document.getElementById('tournamentResultsContainer');
  const tbody = document.getElementById('tournamentTableBody');
  container.style.display = 'block';
  tbody.innerHTML = '<tr><td colspan="7" style="text-align:center; color:#d6a3ff;">Running 25 seeds x 2 roles per policy via WASM engine... Please wait...</td></tr>';

  await new Promise(r => setTimeout(r, 50));
  {
    try {
      run = new TournamentRun(25, 200); // 25 seeds * 2 roles = 50 runs per policy (200 total)
      while (!run.step(1)) {
        updateProgress(run.progress(), run.total());
        await new Promise(r => setTimeout(r));
      }
      const summary = JSON.parse(run.report_json());
      tbody.innerHTML = '';
      const policies = Object.keys(summary.per_policy || {}).sort((a, b) => {
        return (summary.per_policy[b].win_rate_pct || 0) - (summary.per_policy[a].win_rate_pct || 0);
      });
      for (const pol of policies) {
        const st = summary.per_policy[pol];
        const tr = document.createElement('tr');
        const isTop = pol === 'PetTesterTactical';
        tr.style.background = isTop ? 'rgba(157, 78, 221, 0.2)' : '';
        tr.innerHTML = `
          <td style="font-weight:bold; color:${isTop ? '#d6a3ff' : 'var(--text-white)'};">${escapeHtml(pol)} ${isTop ? '⭐' : ''}</td>
          <td>${st.runs}</td>
          <td style="color:${st.win_rate_pct > 0 ? 'var(--text-green)' : 'var(--text-dim)'}; font-weight:bold;">${st.win_rate_pct.toFixed(1)}%</td>
          <td>${st.mean_max_depth.toFixed(2)}</td>
          <td>${st.mean_kills.toFixed(1)}</td>
          <td>${st.mean_gold.toFixed(0)}</td>
          <td>${st.mean_turns.toFixed(0)}</td>
        `;
        tbody.appendChild(tr);
      }
    } catch (e) {
      console.error(e);
      tbody.innerHTML = `<tr><td colspan="7" style="color:var(--text-red);">Error running tournament: ${escapeHtml(e.message)}</td></tr>`;
    } finally {
      if (run) { try { run.free(); } catch (_) {} }
      runBtn.disabled = false;
    }
  }
});

document.getElementById('btnReplayTrajectory').addEventListener('click', () => {
  stopAiMatch();
  const pill = document.getElementById('arenaStatusPill');
  pill.className = 'arena-status-pill pill-running';
  pill.textContent = 'REPLAYING';

  const seed = BigInt(Math.floor(Math.random() * 10000));
  session = new WasmGameSession(seed);
  arenaStep = 0;
  updateUi(null);

  try {
    const trajJson = run_tactical_trajectory(seed, 150);
    const traj = JSON.parse(trajJson);
    document.getElementById('arenaDecision').textContent = `Replaying Trajectory (${traj.steps.length} steps, Score: ${traj.final_score})...`;

    let stepIdx = 0;
    const replayTimer = setInterval(() => {
      if (stepIdx >= traj.steps.length) {
        clearInterval(replayTimer);
        pill.textContent = traj.victory ? 'ASCENDED' : 'COMPLETE';
        pill.className = traj.victory ? 'arena-status-pill pill-win' : 'arena-status-pill pill-dead';
        document.getElementById('arenaDecision').textContent = `Replay Complete: ${traj.end_reason} (Final Score: ${traj.final_score})`;
        return;
      }
      const s = traj.steps[stepIdx];
      document.getElementById('arenaStepCount').textContent = s.turn;
      document.getElementById('arenaDecision').textContent = `Turn ${s.turn} [Dlvl ${s.depth}]: Action ${s.action} — ${s.log_summary || 'OK'}`;

      playSfx('step');
      stepIdx++;
    }, 100);
  } catch (e) {
    console.error(e);
    document.getElementById('arenaDecision').textContent = `Replay error: ${e.message}`;
  }
});

document.querySelectorAll('.role-btn').forEach(btn => {
  btn.addEventListener('click', () => {
    document.querySelectorAll('.role-btn').forEach(b => b.classList.remove('active'));
    btn.classList.add('active');
    selectedRole = btn.dataset.role;
    selectedRace = btn.dataset.race;
  });
});

document.getElementById('btnStartGame').addEventListener('click', async () => {
  const name = document.getElementById('charName').value.trim() || 'Hero';
  try {
    session = await createSession(42n, selectedRole, selectedRace, name);
  } catch (err) {
    messageBarEl.textContent = `Could not create character: ${err.message || err}`;
    return;
  }
  messageBarEl.textContent = `Welcome, ${name} the ${selectedRole.toUpperCase()}! Your quest begins.`;
  charDialog.close();
  updateUi(null);
});

document.getElementById('btnReset').addEventListener('click', async () => {
  stopAiMatch();
  try {
    session = await createSession(42n, selectedRole, selectedRace, 'Hero');
  } catch {
    session = new WasmGameSession(42n);
  }
  messageBarEl.textContent = "Simulation reset to seed 42.";
  updateUi(null);
});

document.getElementById('btnCopyJson').addEventListener('click', () => {
  navigator.clipboard.writeText(obsJsonEl.value);
  alert("Observation JSON copied to clipboard!");
});

document.getElementById('btnAiStep').addEventListener('click', () => {
  stepAiMatch();
});

start();
