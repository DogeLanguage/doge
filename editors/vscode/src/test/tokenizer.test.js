'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { tokenize, scanLine, PALETTE_SIZE } = require('../tokenizer');

// Collect the colours assigned to each identifier/keyword by (line, text),
// keyed for easy assertions. Returns a map text -> array of colorIndex in
// document order.
function coloursByText(src) {
  const lines = src.split('\n');
  const map = new Map();
  for (const t of tokenize(src)) {
    const text = lines[t.line].slice(t.start, t.start + t.length);
    if (!map.has(text)) map.set(text, []);
    map.get(text).push(t.colorIndex);
  }
  return map;
}

test('a keyword and its bound name share one colour', () => {
  const c = coloursByText('such age = 7\nwow');
  assert.deepEqual(c.get('such'), c.get('age'));
});

test('successive groups cycle to different colours', () => {
  const toks = tokenize('such a = 1\nsuch b = 2\nwow');
  // three groups: `such a`, `such b`, `wow` → colours 0, 1, 2
  const colours = [...new Set(toks.map((t) => t.colorIndex))];
  assert.deepEqual(colours, [0, 1, 2]);
});

test('`such name` and `much params` are adjacent but distinct groups', () => {
  const c = coloursByText('such greet much a, b:\n    bark "hi"\nwow');
  const suchColour = c.get('such')[0];
  const greetColour = c.get('greet')[0];
  const muchColour = c.get('much')[0];
  const aColour = c.get('a')[0];
  const bColour = c.get('b')[0];
  assert.equal(suchColour, greetColour, 'such + greet share a colour');
  assert.equal(muchColour, aColour, 'much + a share a colour');
  assert.equal(aColour, bColour, 'both params share the much colour');
  assert.notEqual(suchColour, muchColour, 'the two groups differ');
});

test('`oh no err!` fuses the compound keyword with the bound name', () => {
  const c = coloursByText('pls\n    x()\noh no err!\n    bark err');
  const ohColour = c.get('oh')[0];
  assert.equal(c.get('no')[0], ohColour, 'oh + no share a colour');
  assert.equal(c.get('err')[0], ohColour, 'the bound name joins the group');
});

test('strings and comments are never coloured', () => {
  const toks = tokenize('bark "such wow much bark"  # such comment\nwow');
  // Only the two `bark`/`wow` keywords outside the string/comment are coloured.
  // The `such`/`much`/`bark` INSIDE the string and comment must not appear.
  assert.equal(toks.length, 2);
});

test('`so` groups both an import and a constant', () => {
  const c = coloursByText('so nerd\nso PI = 3.14\nwow');
  assert.deepEqual(c.get('so'), [0, 1], 'two separate so-groups cycle');
  assert.equal(c.get('nerd')[0], 0);
  assert.equal(c.get('PI')[0], 1);
});

test('universal keywords and literals are not rainbow-coloured', () => {
  const toks = tokenize('for x in xs:\n    if x:\n        return none');
  assert.equal(toks.length, 0);
});

test('colour indices stay within the palette', () => {
  const many = Array.from({ length: 30 }, (_, i) => `such v${i} = ${i}`).join('\n');
  for (const t of tokenize(many)) {
    assert.ok(t.colorIndex >= 0 && t.colorIndex < PALETTE_SIZE);
  }
});

test('scanLine skips comment tails', () => {
  const toks = scanLine('bark x # not scanned');
  assert.deepEqual(toks.map((t) => t.text), ['bark', 'x']);
});
