'use strict';

// Rainbow tokenizer for Doge source. Pure (no vscode dependency) so it can be
// unit-tested with `node --test`. It walks the document top-to-bottom, forms a
// "group" for each doge-speak statement (a keyword plus the names bound to it),
// and hands each group the next colour of an 8-step rainbow. Sequential cycling
// means adjacent groups never share a colour, so a file reads as scattered
// doge-meme text rather than one colour per keyword.
//
// The doge keyword set mirrors crates/doge-compiler/src/keywords.rs (`lookup`).
// Universal keywords (if/for/return/...) and literals (true/false/none) are
// intentionally NOT rainbow-coloured — the TextMate grammar themes those, so the
// rainbow stays exclusively doge-speak. Keep this list in sync with keywords.rs.

const PALETTE_SIZE = 8;

// Doge keywords that stand alone as their own group.
const KEYWORD_ALONE = new Set(['bark', 'wow', 'pls', 'bonk', 'bork']);

// A keyword followed by a single bound name: `so nerd`, `such age`,
// `many Shibe`, `very age`.
const KEYWORD_WITH_NAME = new Set(['so', 'such', 'many', 'very']);

// Every reserved word, so a group never swallows a following keyword as if it
// were a bound name (highlighting stays sane on syntactically invalid input).
const RESERVED = new Set([
  'pls', 'bork', 'bonk', 'bark', 'wow', 'such', 'much', 'many', 'so', 'very',
  'oh', 'no',
  'if', 'elif', 'else', 'for', 'while', 'in', 'return', 'continue',
  'and', 'or', 'not', 'true', 'false', 'none',
  'def', 'class', 'amaze',
]);

function isIdentStart(ch) {
  return (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || ch === '_';
}

function isIdentPart(ch) {
  return isIdentStart(ch) || (ch >= '0' && ch <= '9');
}

// Scan one line into tokens, skipping comments and string literals (their
// contents can contain keyword-looking text and `{expr}` interpolation holes,
// none of which should be rainbow-coloured). Each token is
// { start, length, text, kind } where kind is 'word' (identifier) or 'other'
// (a single operator/punctuation/digit character used only for grouping).
function scanLine(line) {
  const tokens = [];
  const n = line.length;
  let i = 0;
  while (i < n) {
    const ch = line[i];
    if (ch === ' ' || ch === '\t' || ch === '\r') {
      i++;
      continue;
    }
    if (ch === '#') {
      break; // comment runs to end of line
    }
    if (ch === '"') {
      i++;
      while (i < n) {
        if (line[i] === '\\') {
          i += 2;
          continue;
        }
        if (line[i] === '"') {
          i++;
          break;
        }
        i++;
      }
      continue;
    }
    if (isIdentStart(ch)) {
      const start = i;
      i++;
      while (i < n && isIdentPart(line[i])) {
        i++;
      }
      tokens.push({ start, length: i - start, text: line.slice(start, i), kind: 'word' });
      continue;
    }
    tokens.push({ start: i, length: 1, text: ch, kind: 'other' });
    i++;
  }
  return tokens;
}

// Tokenize a whole document into rainbow tokens:
// [{ line, start, length, colorIndex }], colorIndex in 0..PALETTE_SIZE-1.
function tokenize(text) {
  const lines = text.split('\n');
  const flat = [];
  for (let ln = 0; ln < lines.length; ln++) {
    for (const tok of scanLine(lines[ln])) {
      flat.push({ ...tok, line: ln });
    }
  }

  const out = [];
  let color = 0;
  const emit = (tok) => {
    out.push({ line: tok.line, start: tok.start, length: tok.length, colorIndex: color % PALETTE_SIZE });
  };
  const isName = (tok) => tok && tok.kind === 'word' && !RESERVED.has(tok.text);

  let i = 0;
  while (i < flat.length) {
    const tok = flat[i];
    if (tok.kind !== 'word') {
      i++;
      continue;
    }
    const word = tok.text;

    if (word === 'oh' && flat[i + 1] && flat[i + 1].kind === 'word' && flat[i + 1].text === 'no') {
      // `oh no <name>!` — the lexer fuses `oh no`; colour both plus the bound name.
      emit(tok);
      emit(flat[i + 1]);
      i += 2;
      if (isName(flat[i])) {
        emit(flat[i]);
        i++;
      }
      color++;
      continue;
    }

    if (KEYWORD_WITH_NAME.has(word)) {
      emit(tok);
      i++;
      if (isName(flat[i])) {
        emit(flat[i]);
        i++;
      }
      color++;
      continue;
    }

    if (word === 'much') {
      // `much a, b, c` — the parameter list is one group with the `much` keyword.
      emit(tok);
      i++;
      while (isName(flat[i])) {
        emit(flat[i]);
        i++;
        if (flat[i] && flat[i].kind === 'other' && flat[i].text === ',') {
          i++;
          continue;
        }
        break;
      }
      color++;
      continue;
    }

    if (KEYWORD_ALONE.has(word)) {
      emit(tok);
      color++;
      i++;
      continue;
    }

    i++;
  }

  return out;
}

module.exports = { tokenize, scanLine, PALETTE_SIZE };
