'use strict';

// Regression tests for the TextMate grammar's non-rainbow scopes (builtins,
// `self`, and stdlib module members). The rainbow layer is covered by
// tokenizer.test.js; this file guards the lexical categories the grammar themes.
//
// The grammar's `match` patterns are Oniguruma, but the constructs the tested
// patterns use (\b, character classes, fixed-length lookbehind/ahead) are all
// valid JS RegExp too, so we compile each pattern with `new RegExp` and assert
// against lines drawn from examples/. Anchored `^(?:…)$`-free: we test that a
// pattern matches the intended token at the intended spot, not the whole line.

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const grammarPath = path.join(__dirname, '..', '..', 'syntaxes', 'doge.tmLanguage.json');
const grammar = JSON.parse(fs.readFileSync(grammarPath, 'utf8'));

// A pattern's `match` string, compiled to a global RegExp so we can find the
// token's offset within a line.
function pattern(name) {
  const src = grammar.repository[name].match;
  return new RegExp(src, 'g');
}

// The offset of the first match of `re` in `line`, or -1. `re` must be global.
function matchAt(re, line) {
  re.lastIndex = 0;
  const m = re.exec(line);
  return m ? m.index : -1;
}

test('builtins match at call sites', () => {
  const re = pattern('builtins');
  for (const line of ['len("x")', 'range(3)', 'dec("0.1")', 'bark str(age)', 'such n = int(x)']) {
    assert.notEqual(matchAt(re, line), -1, `should match a builtin in: ${line}`);
  }
});

test('builtins do not match a dot-prefixed member or a bare non-call use', () => {
  const re = pattern('builtins');
  assert.equal(matchAt(re, 'such x = dog.int'), -1, 'field named int is not the builtin');
  assert.equal(matchAt(re, 'such x = d.len'), -1, 'method named len is not the builtin');
  assert.equal(matchAt(re, 'such int = 3'), -1, 'bare int without a call is not the builtin');
});

test('self matches the receiver', () => {
  const re = pattern('self');
  assert.notEqual(matchAt(re, '        self.name = name'), -1);
  assert.equal(matchAt(re, 'such myself = 1'), -1, 'self is a whole word, not a substring');
});

test('modules match name plus member, capturing both', () => {
  const re = pattern('modules');
  for (const [line, mod, member] of [
    ['such text = fetch.read(path)', 'fetch', 'read'],
    ['bark strings.beeg(name)', 'strings', 'beeg'],
    ['such n = roll.int(1, 6)', 'roll', 'int'],
  ]) {
    re.lastIndex = 0;
    const m = re.exec(line);
    assert.ok(m, `should match a module call in: ${line}`);
    assert.equal(m[1], mod);
    assert.equal(m[2], member);
  }
});

test('roll.int is claimed by the module pattern, not the builtin pattern', () => {
  // `int` after a module dot must read as a module member, so the builtin
  // pattern (which forbids a preceding dot) must not fire on it.
  const line = 'such n = roll.int(1, 6)';
  const builtins = pattern('builtins');
  assert.equal(matchAt(builtins, line), -1, 'builtin must not match roll.int');
  const modules = pattern('modules');
  assert.notEqual(matchAt(modules, line), -1, 'module pattern must match roll.int');
});

test('grammar lists every stdlib module name', () => {
  // Mirror of crates/doge-compiler/src/stdlib.rs MODULES — keep in sync when a
  // module is added or removed.
  const expected = [
    'nerd', 'strings', 'hunt', 'fetch', 'env', 'howl',
    'json', 'dson', 'nap', 'pack', 'chase', 'roll',
  ];
  const src = grammar.repository.modules.match;
  for (const name of expected) {
    assert.ok(new RegExp(`\\b${name}\\b`).test(src), `module ${name} listed in grammar`);
  }
});

test('grammar lists every builtin name', () => {
  // Mirror of crates/doge-compiler/src/builtins.rs BUILTINS — keep in sync.
  const expected = ['len', 'str', 'int', 'float', 'bytes', 'dec', 'range', 'gib'];
  const src = grammar.repository.builtins.match;
  for (const name of expected) {
    assert.ok(new RegExp(`\\b${name}\\b`).test(src), `builtin ${name} listed in grammar`);
  }
});
