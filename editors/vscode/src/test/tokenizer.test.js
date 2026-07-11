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

test('a used variable reuses its definition colour', () => {
  const c = coloursByText('such age = 7\nbark age\nwow');
  const defColour = c.get('such')[0];
  assert.deepEqual(c.get('age'), [defColour, defColour], 'binding and use share the colour');
});

test('a called function reuses its definition colour', () => {
  const c = coloursByText('such greet much x:\n    bark x\nwow\ngreet(1)');
  const defColour = c.get('greet')[0];
  assert.equal(c.get('greet')[1], defColour, 'the call site matches the definition');
});

test('an imported name reuses its import colour', () => {
  const c = coloursByText('so nerd\nbark nerd(7)');
  assert.deepEqual(c.get('nerd'), [0, 0], 'import and use both colour 0');
});

test('an undeclared identifier used in an expression stays uncoloured', () => {
  const c = coloursByText('such age = other + 1\nwow');
  assert.equal(c.get('other'), undefined, 'a name never bound gets no token');
});

test('tokens stay sorted by (line, start) after the use pass', () => {
  const toks = tokenize('such age = 7\nbark age\nwow');
  for (let i = 1; i < toks.length; i++) {
    const prev = toks[i - 1];
    const cur = toks[i];
    assert.ok(
      cur.line > prev.line || (cur.line === prev.line && cur.start >= prev.start),
      'tokens are in ascending document order'
    );
  }
});

test('universal keywords and literals are not themselves rainbow-coloured', () => {
  // `for`/`in`/`if`/`return`/`none` never get a token; only the loop variable
  // `x` (a fresh binding) and its use in the body do.
  const c = coloursByText('for x in xs:\n    if x:\n        return none');
  assert.equal(c.get('for'), undefined);
  assert.equal(c.get('in'), undefined);
  assert.equal(c.get('if'), undefined);
  assert.equal(c.get('return'), undefined);
  assert.equal(c.get('none'), undefined);
  // `xs` is never bound, so it stays the theme default (no token).
  assert.equal(c.get('xs'), undefined);
});

test('a for-loop variable and its body uses share one fresh colour', () => {
  const c = coloursByText('for item in items:\n    bark item\nwow');
  const varColour = c.get('item')[0];
  assert.equal(c.get('item')[1], varColour, 'the body use matches the binding');
});

test('a for-loop variable reuses an already-declared name colour', () => {
  const c = coloursByText('such item = 1\nfor item in items:\n    bark item\nwow');
  const declColour = c.get('such')[0];
  // Every `item` — the declaration, the loop header, and the body use — is the
  // colour the `such` binding first gave it.
  for (const colour of c.get('item')) {
    assert.equal(colour, declColour);
  }
});

test('a destructuring declaration paints every target name', () => {
  const c = coloursByText('such a, b, many rest = xs\nwow');
  const suchColour = c.get('such')[0];
  // Each leading name, the `many` collector keyword, and the collector name all
  // join the one `such` group.
  assert.equal(c.get('a')[0], suchColour, 'a joins the group');
  assert.equal(c.get('b')[0], suchColour, 'b joins the group');
  assert.equal(c.get('many')[0], suchColour, 'the collector keyword joins');
  assert.equal(c.get('rest')[0], suchColour, 'the collector name joins');
});

test('a for-loop paints every destructuring variable', () => {
  const c = coloursByText('for k, v in d:\n    bark k\n    bark v\nwow');
  const kColour = c.get('k')[0];
  assert.equal(c.get('v')[0], kColour, 'both loop variables share the group');
  assert.equal(c.get('k')[1], kColour, 'the body use of k matches');
  assert.equal(c.get('v')[1], kColour, 'the body use of v matches');
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
