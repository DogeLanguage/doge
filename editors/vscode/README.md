# Doge for VS Code

Registers the `doge` language for `.doge` files and gives them the Doge mark as
their file icon in the Explorer and editor tabs. Also wires up `#` line comments
and `[]` / `()` bracket matching.

The icon renders as a fallback, so it shows up regardless of which file icon
theme you have active — no need to switch themes.

## Install from source

```sh
cd editors/vscode
npm install -g @vscode/vsce   # once, if you don't have it
vsce package                  # produces doge-<version>.vsix
code --install-extension doge-0.1.0.vsix
```

Or, for quick local iteration, copy this folder into `~/.vscode/extensions/`
and reload VS Code.

The mark is sourced from [`brand/assets/favicon.svg`](../../brand/assets/favicon.svg);
keep `icons/doge.svg` in sync if the brand mark changes.
