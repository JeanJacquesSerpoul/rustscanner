# RustScanner

*[English version](README.en.md)*

Application de numérisation de documents pour Linux, dans l'esprit de **Skanpage** (KDE), écrite entièrement en Rust : cœur **Tauri 2**, interface **Leptos** (WebAssembly). Interface en **français et en anglais**, thème clair / sombre.

## Fonctionnalités

- **Tous les scanners SANE** : USB, réseau (eSCL / AirScan, saned), détection automatique, réouverture du dernier scanner utilisé.
- **Réglages essentiels en un coup d'œil** : source (vitre, chargeur, recto-verso), couleur / niveaux de gris / noir et blanc, résolution en pastilles avec conseil d'usage et taille estimée, formats de page (A4, A5, Letter, Legal, photo, carte…) avec bascule portrait / paysage.
- **Toutes les options du scanner** générées automatiquement (curseurs, interrupteurs, listes), titres traduits par le catalogue de sane-backends, valeurs mémorisées par scanner, retour aux réglages d'usine.
- **Aperçu et zone de numérisation** : aperçu basse résolution de toute la vitre, cadre à tracer, déplacer et redimensionner à la souris (8 poignées), dimensions en millimètres et format reconnu.
- **Documents multipages** : numérisation en série depuis le chargeur, vignettes avec sélection multiple (Ctrl / Maj), glisser-déposer pour réordonner, rotation, retournement, miroir, suppression avec **Annuler**.
- **Visionneuse** : page entière, pleine largeur, taille réelle (1:1), zoom à la molette (Ctrl).
- **Enregistrement** : PDF multipage à la taille physique réelle (pages au trait compressées sans perte en 1 bit), PNG, JPEG (densité en dpi), TIFF, WebP sans perte ; modèle de nom de fichier (`{date}`, `{time}`, `{n}`) sans écrasement ; ouverture du fichier ou du dossier depuis la notification.
- **Impression** via CUPS (choix de l'imprimante, nombre de copies, pages sélectionnées).
- **Sûreté** : confirmation avant de quitter ou de vider un document non enregistré, annulation d'une numérisation à tout moment (Échap), fichiers temporaires nettoyés.

## Raccourcis clavier

| Action | Raccourci |
|---|---|
| Numériser / aperçu | `Ctrl+Entrée` / `Ctrl+Maj+Entrée` |
| Annuler la numérisation | `Échap` |
| Enregistrer / imprimer / nouveau | `Ctrl+S` / `Ctrl+P` / `Ctrl+N` |
| Page précédente / suivante | flèches, `Page↑` / `Page↓` |
| Pivoter à gauche / à droite | `Ctrl+L` / `Ctrl+R` |
| Déplacer la page | `Alt+↑` / `Alt+↓` |
| Supprimer / annuler | `Suppr` / `Ctrl+Z` |
| Zoom | `+` `−` `0` (page entière) `W` (largeur) |
| Page / zone de numérisation | `Tab` |
| Tous les raccourcis | `F1` |

## Prérequis

- SANE (`libsane1`) ; pour les scanners réseau récents : `sane-airscan`.
- Pour compiler : Rust (édition 2024), la cible `wasm32-unknown-unknown`, `trunk`, `tauri-cli` 2, et les bibliothèques WebKitGTK 4.1 de Tauri.

```bash
sudo apt install libsane1 sane-utils sane-airscan libwebkit2gtk-4.1-dev
```

```bash
rustup target add wasm32-unknown-unknown
```

```bash
cargo install trunk tauri-cli
```

libsane est chargée dynamiquement : sans SANE, l'application démarre et explique comment l'installer.

## Installation

Le script `install.sh` compile l'application et l'installe avec son icône et son entrée de menu (dans `~/.local` par défaut) :

```bash
./install.sh --deps
```

| Option | Effet |
|---|---|
| `--deps` | installe d'abord les paquets système (apt, dnf, pacman ou zypper) |
| `--system` | installe pour tous les utilisateurs dans `/usr/local` (sudo) |
| `--no-build` | installe le binaire déjà compilé (`target/release/rustscanner`) |
| `--uninstall` | désinstalle (avec `--system` pour `/usr/local`) |

Il installe au besoin la cible WebAssembly, `trunk` et `tauri-cli`, puis signale ce qui manque à l'exécution (SANE, groupe `scanner` pour les scanners USB, `PATH`).

## Compilation et lancement

```bash
cargo tauri dev
```

```bash
cargo tauri build --bundles deb
```

Le paquet est produit dans `target/release/bundle/deb/`.

### Travailler l'interface dans un navigateur

`trunk serve` sert l'interface sur http://localhost:1420 ; hors de Tauri, `frontend/devshim.js` simule le cœur (scanners, numérisation, pages) pour itérer sur l'interface sans scanner.

### Scanner virtuel pour les essais

sane-backends fournit un scanner de test. Pour l'utiliser sans toucher à la configuration du système :

```bash
mkdir -p /tmp/sane-test && echo test > /tmp/sane-test/dll.conf
```

```bash
SANE_CONFIG_DIR=/tmp/sane-test cargo tauri dev
```

## Tests

```bash
cargo test
```

Le test d'intégration `src-tauri/tests/sane_test_backend.rs` pilote le scanner virtuel de SANE (couleur, trait 1 bit, aperçu, chargeur, bourrage simulé, annulation) ; il est ignoré si ce backend n'est pas installé.

## Architecture

```
shared/      types communs (options, pages, événements, réglages) et i18n (tr!, trf!)
src-tauri/   cœur Tauri
  sane.rs      liaison avec libsane (chargement dynamique)
  scanner.rs   fil dédié au scanner : commandes, numérisation, aperçu, annulation
  frames.rs    trames SANE (gris, RVB, 3 passes ; 1, 8, 16 bits) → image
  document.rs  pages de la session (PNG sur disque, vignettes en mémoire)
  export.rs    PDF, images, impression CUPS
  gettext.rs   traductions des options (catalogue .mo de sane-backends)
  commands.rs  commandes invoquées par l'interface
frontend/    interface Leptos (header, sidebar, viewer, pages, dialogs…)
```

Les images sont servies à l'interface par le protocole `scan://` (vignettes et affichage en JPEG), sans transiter par l'IPC.

## Licence

MIT
