# Doge for VS Code

Registers the `doge` language for `.doge` files, gives them the Doge mark as
their file icon, wires up `#` line comments and `[]` / `()` bracket matching, and
paints the source in **rainbow syntax highlighting** that mirrors the doge meme.

## Rainbow highlighting

Doge-speak reads like scattered meme text, so it's coloured like it too. Each
doge statement forms a *group* — a keyword plus the names it binds — and every
group takes the next colour of an 8-step rainbow as the file is read top to
bottom. Because the cycle advances per group, adjacent groups never share a
colour and a whole file looks like the doge meme rather than one flat colour per
keyword:

```doge
so nerd                     # so + nerd  → colour 1
such greet much name:       # such greet → colour 2, much name → colour 3
    bark "much hello"       # bark       → colour 4
wow                         # wow        → colour 5
```

Groups: `so <name>`, `such <name>`, `much <params>`, `many <name>`,
`very <name>`, `oh no <name>`, and the standalone keywords `bark`, `wow`, `pls`,
`bonk`, `bork`. Strings and comments are never recoloured, and universal
keywords (`if`, `for`, `return`, …) and literals (`true`/`false`/`none`) keep
your theme's colours — the rainbow is exclusively doge-speak.

The colours ship as defaults so the rainbow shows in **any** theme (dark or
light). To change them, override `editor.semanticTokenColorCustomizations` →
`rules` → `dogeRainbow1`…`dogeRainbow8` in your settings.

Implementation: a TextMate grammar (`syntaxes/doge.tmLanguage.json`) provides the
base scopes, and a semantic-token provider (`src/`) computes the per-group
rainbow. The keyword list in `src/tokenizer.js` mirrors
[`crates/doge-compiler/src/keywords.rs`](../../crates/doge-compiler/src/keywords.rs)
(`lookup`) — keep them in sync when keywords change.

## Tests

```sh
cd editors/vscode
node --test                    # tokenizer unit tests, no dependencies
```

## Install from source

```sh
cd editors/vscode
npm install -g @vscode/vsce   # once, if you don't have it
vsce package                  # produces doge-<version>.vsix
code --install-extension doge-0.2.0.vsix
```

Or, for quick local iteration, run
`code --extensionDevelopmentPath=editors/vscode examples/tour.doge`, or copy this
folder into `~/.vscode/extensions/` and reload VS Code.

The mark is sourced from [`brand/assets/favicon.svg`](../../brand/assets/favicon.svg);
keep `icons/doge.svg` in sync if the brand mark changes.
