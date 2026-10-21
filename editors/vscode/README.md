# Crust pour VS Code

Fichiers `.rsc` (Rust avec `class`, `over`, `import`) dans VS Code, via
rust-analyzer et `crust-lsp`.

- `.rsc` associé au langage Rust ; coloration des mots-clés Crust.
- Dans un projet Crust (`crust-build` en dépendance de build), l'extension
  propose de régler `rust-analyzer.server.path` sur `crust-lsp` pour ce
  workspace : complétion, navigation, renommage, diagnostics dans les `.rsc`.
- Commande « Crust : afficher le Rust généré » (bouton dans la barre de l'éditeur).
- Débogage avec CodeLLDB : points d'arrêt dans les `.rsc`.

Prérequis : extension rust-analyzer, `crust-lsp` et `cargo-crust` installés
(`cargo install --path crates/crust-lsp` et `--path crates/cargo-crust`).

## Installation depuis WSL, côté Windows

`code --install-extension` refuse un chemin `\\wsl.localhost\…` : copier
d'abord le `.vsix` sur un disque Windows, puis l'installer.

```powershell
copy \\wsl.localhost\Ubuntu\home\rtx\crust\editors\vscode\crust-0.1.1.vsix $env:TEMP
code --install-extension $env:TEMP\crust-0.1.1.vsix
```

## Débogage

Points d'arrêt dans les `.rsc` avec CodeLLDB : l'extension ajoute le `sourceMap`
des copies générées vers `src/`, et choisit `"breakpointMode": "file"` si la
configuration ne précise rien (le paquet est compilé plusieurs fois : binaire,
tests).
