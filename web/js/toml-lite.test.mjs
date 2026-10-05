// node --test web/js/toml-lite.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { parseEntries, serializeEntries } from './toml-lite.js';

const roundTrip = (text) => serializeEntries(parseEntries(text, 'monster'), 'monster');

test('plain entries round-trip exactly and are not lossy', () => {
  const text = '# header comment\n\n[[monster]]\nname = "jackal"\nlevel = 5\nspeed = -1\nhostile = true\n';
  const e = parseEntries(text, 'monster');
  assert.equal(e.lossy, false);
  assert.deepEqual(e[0].fields, { name: 'jackal', level: 5, speed: -1, hostile: true });
  assert.equal(roundTrip(text), text);
});

for (const [label, line] of [
  ['trailing comment on a number', 'level = 3 # buff'],
  ['trailing comment on a string', 'name = "jackal" # x'],
  ['escaped string', 'name = "a \\"b\\""'],
  ['literal string', "name = 'x'"],
  ['underscore number', 'cost = 1_000'],
  ['hex number', 'cost = 0x10'],
  ['plus sign', 'level = +5'],
  ['exponent', 'cost = 1e3'],
  ['float', 'weight = 1.5'],
  ['dotted key', 'armor.ac = 3'],
  ['quoted key', '"level" = 3'],
  ['comment inside entry', '# note'],
  ['inline table', 'intrinsics = {}'],
  ['array', 'attacks = [{ at = "Bite" }]'],
  ['nested table header', '[monster.extra]'],
]) {
  test(`marks file lossy: ${label}`, () => {
    const e = parseEntries(`[[monster]]\nname = "jackal"\n${line}\n`, 'monster');
    assert.equal(e.lossy, true, line);
  });
}

test('top-level key in preamble is lossy', () => {
  assert.equal(parseEntries('title = "x"\n[[monster]]\nname = "a"\n', 'monster').lossy, true);
});

test('serialized strings are valid TOML basic strings', () => {
  const e = parseEntries('[[monster]]\nname = "a"\n', 'monster');
  e[0].fields.name = 'say "hi" \\ ok';
  const out = serializeEntries(e, 'monster');
  assert.match(out, /name = "say \\"hi\\" \\\\ ok"/);
});
