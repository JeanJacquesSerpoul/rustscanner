# RustScanner

*[English version](README.en.md)*

Application de numérisation de documents pour **Linux et Windows**, dans l'esprit de **Skanpage** (KDE), écrite entièrement en Rust : cœur **Tauri 2**, interface **Leptos** (WebAssembly). Interface en **français et en anglais**, thème clair / sombre.

## Fonctionnalités

- **Tous les scanners SANE** (Linux) **ou WIA** (Windows) : USB, réseau (eSCL / AirScan, saned, WSD), détection automatique, réouverture du dernier scanner utilisé.
- **Réglages essentiels en un coup d'œil** : source (vitre, chargeur, recto-verso), couleur / niveaux de gris / noir et blanc, résolution en pastilles avec conseil d'usage et taille estimée, formats de page (A4, A5, Letter, Legal, photo, carte…) avec bascule portrait / paysage.
- **Toutes les options du scanner** générées automatiquement (curseurs, interrupteurs, listes), titres traduits par le catalogue de sane-backends, valeurs mémorisées par scanner, retour aux réglages d'usine.
- **Aperçu et zone de numérisation** : aperçu basse résolution de toute la vitre, cadre à tracer, déplacer et redimensionner à la souris (8 poignées), dimensions en millimètres et format reconnu.
- **Documents multipages** : numérisation en série depuis le chargeur, vignettes avec sélection multiple (Ctrl / Maj), glisser-déposer pour réordonner, rotation, retournement, miroir, suppression avec **Annuler**.
- **Visionneuse** : page entière, pleine largeur, taille réelle (1:1), zoom à la molette (Ctrl).
- **Enregistrement** : PDF multipage à la taille physique réelle (pages au trait compressées sans perte en 1 bit), PNG, JPEG (densité en dpi), TIFF, WebP sans perte ; modèle de nom de fichier (`{date}`, `{time}`, `{n}`) sans écrasement ; ouverture du fichier ou du dossier depuis la notification.
- **Impression** via CUPS, ou les imprimantes de Windows (choix de l'imprimante, nombre de copies, pages sélectionnées).
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

## Prérequis (Linux)

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

## Installation sous Linux

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

## Windows

Sous Windows (10 et 11, x64), RustScanner pilote les scanners par **WIA** (Windows Image Acquisition), présent sur tout système : un scanner reconnu par Windows (USB, ou réseau ajouté dans *Paramètres › Bluetooth et appareils › Imprimantes et scanners*) apparaît dans RustScanner. L'impression passe par les imprimantes de Windows, chaque page à sa taille réelle.

### Installation

Le plus simple est l'installeur `RustScanner_0.1.0_x64-setup.exe` : il installe l'application pour l'utilisateur courant, sans droits d'administrateur, avec ses raccourcis dans le menu Démarrer, et se désinstalle depuis *Paramètres › Applications*. Pour un déploiement sur tous les postes ou tous les utilisateurs, `RustScanner_0.1.0_x64_fr-FR.msi` (ou `en-US`) s'installe pour toute la machine. Si WebView2 manque (Windows 10 ancien), l'installeur le télécharge.

Pour compiler et installer depuis les sources, le script `install.ps1` est l'équivalent de `install.sh` (`install.cmd` le lance depuis l'Explorateur ou `cmd.exe`, avec les mêmes options) :

```powershell
.\install.ps1 -Deps
```

| Option | Effet |
|---|---|
| `-Deps` | installe d'abord les outils manquants avec winget (Rust, outils de compilation C++ de Visual Studio) |
| `-System` | installe pour tous les utilisateurs (MSI, droits d'administrateur) |
| `-NoBuild` | utilise l'installeur déjà construit (`target\release\bundle`) |
| `-Silent` | installe ou désinstalle sans fenêtre |
| `-Uninstall` | désinstalle |

Si Windows refuse d'exécuter le script, passez par `install.cmd`, ou lancez `powershell -ExecutionPolicy Bypass -File .\install.ps1`. Le script installe au besoin la cible WebAssembly, `trunk` et `tauri-cli`, puis vérifie le service WIA et la présence d'un scanner.

### Construire les installeurs

```powershell
cargo tauri build
```

`src-tauri/tauri.windows.conf.json` complète la configuration sous Windows : l'installeur NSIS (bilingue) est produit dans `target\release\bundle\nsis\`, les MSI (un par langue) dans `target\release\bundle\msi\`. Les outils NSIS et WiX sont téléchargés par Tauri à la première construction.

## Scanners compatibles

**Sous Linux**, RustScanner fonctionne avec tous les scanners pris en charge par SANE, soit la grande majorité des modèles USB et réseau (Canon, Epson, HP, Brother…) ; pour les scanners réseau récents (eSCL / AirScan), installez `sane-airscan`. Le code ne contient rien de propre à un modèle.

**Sous Windows**, il fonctionne avec tout scanner doté d'un pilote WIA, ce qui est le cas de presque tous les scanners grand public et des imprimantes multifonctions, ainsi que des scanners réseau ajoutés dans les paramètres de Windows. Quelques limites :

- **Scanners sans pilote WIA** : certains scanners professionnels n'ont qu'un pilote TWAIN, que RustScanner ne gère pas, et les Fujitsu ScanSnap ne passent que par leur propre logiciel. Ces modèles n'apparaissent pas dans la liste.
- **Chargeur et recto-verso** : pris en charge par le code, mais pas encore essayés sur un vrai scanner (seule la vitre d'un Canon MG3600 l'a été).
- **Pilotes inégaux** : chaque fabricant interprète WIA à sa façon (le pilote Canon rognait par exemple la zone selon l'ordre des réglages). Les réglages secondaires sont donc appliqués au mieux : un réglage refusé par le pilote est ignoré au lieu de bloquer la numérisation. Avec un pilote très particulier, la zone ou la résolution obtenues peuvent légèrement différer de celles demandées.

Pour vérifier un autre modèle sous Windows, branchez-le et lancez le test sur matériel réel ; il liste les options du scanner, fait un aperçu et numérise une zone en noir et blanc :

```powershell
cargo test -p rustscanner --test wia_backend -- --include-ignored --nocapture
```

Si un modèle pose problème, joignez la sortie de ce test au signalement.

## Compilation et lancement

```bash
cargo tauri dev
```

```bash
cargo tauri build --bundles deb
```

Le paquet est produit dans `target/release/bundle/deb/` (sous Windows : voir ci-dessus).

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

Sous Windows, `src-tauri/tests/wia_backend.rs` liste les scanners WIA et lit les options du premier ; l'aperçu et une numérisation au trait dans une zone, qui font fonctionner le scanner, ne se lancent qu'à la demande :

```powershell
cargo test -p rustscanner --test wia_backend -- --include-ignored --nocapture
```

## Architecture

```
shared/      types communs (options, pages, événements, réglages) et i18n (tr!, trf!)
src-tauri/   cœur Tauri
  sane.rs      liaison avec libsane (chargement dynamique ; Linux, macOS)
  wia.rs       scanners WIA 2.0 présentés comme des options SANE (Windows)
  scanner.rs   fil dédié au scanner : commandes, numérisation, aperçu, annulation
  frames.rs    trames SANE (gris, RVB, 3 passes ; 1, 8, 16 bits) → image
  document.rs  pages de la session (PNG sur disque, vignettes en mémoire)
  export.rs    PDF, images, impression CUPS
  winprint.rs  impression GDI (Windows)
  gettext.rs   traductions des options (catalogue .mo de sane-backends)
  commands.rs  commandes invoquées par l'interface
frontend/    interface Leptos (header, sidebar, viewer, pages, dialogs…)
```

Les images sont servies à l'interface par le protocole `scan://` (vignettes et affichage en JPEG), sans transiter par l'IPC.

## Licence

MIT
