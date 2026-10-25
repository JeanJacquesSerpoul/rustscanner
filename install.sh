#!/bin/sh
# Installation de RustScanner sous Linux : dépendances (facultatif), compilation,
# lanceur, icône et entrée de menu. Installation par utilisateur (~/.local) par
# défaut, ou pour tout le système (/usr/local) avec --system.
#
#   ./install.sh               compile et installe pour l'utilisateur courant
#   ./install.sh --deps        installe d'abord les paquets système nécessaires
#   ./install.sh --system      installe dans /usr/local (sudo)
#   ./install.sh --no-build    installe le binaire déjà compilé (target/release)
#   ./install.sh --uninstall   désinstalle (avec --system pour /usr/local)
set -eu

DIR="$(cd "$(dirname "$0")" && pwd)"
APP=rustscanner

# ---- messages bilingues (d'après la locale) ----
case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in
    fr*) FR=1 ;;
    *) FR=0 ;;
esac
say() { if [ "$FR" = 1 ]; then printf '%s\n' "$1"; else printf '%s\n' "$2"; fi; }
step() { printf '\033[1;34m==>\033[0m '; say "$1" "$2"; }
warn() { printf '\033[1;33m!!\033[0m '; say "$1" "$2"; }
die() { printf '\033[1;31mxx\033[0m ' >&2; say "$1" "$2" >&2; exit 1; }

usage() {
    say "Usage : ./install.sh [--deps] [--system] [--no-build] [--uninstall]

  --deps       installe les paquets système (apt, dnf, pacman ou zypper)
  --system     installe pour tous les utilisateurs dans /usr/local (sudo)
  --no-build   ne recompile pas : utilise target/release/$APP
  --uninstall  supprime l'application installée" \
"Usage: ./install.sh [--deps] [--system] [--no-build] [--uninstall]

  --deps       install system packages (apt, dnf, pacman or zypper)
  --system     install for every user under /usr/local (sudo)
  --no-build   do not rebuild: use target/release/$APP
  --uninstall  remove the installed application"
}

DEPS=0 SYSTEM=0 BUILD=1 UNINSTALL=0
for arg in "$@"; do
    case "$arg" in
        --deps) DEPS=1 ;;
        --system) SYSTEM=1 ;;
        --no-build) BUILD=0 ;;
        --uninstall) UNINSTALL=1 ;;
        -h | --help) usage; exit 0 ;;
        *) usage; exit 2 ;;
    esac
done

if [ "$SYSTEM" = 1 ]; then
    PREFIX=/usr/local
    if [ "$(id -u)" = 0 ]; then SUDO=""; else SUDO=sudo; fi
else
    PREFIX="$HOME/.local"
    SUDO=""
fi
BIN="$PREFIX/bin/$APP"
DESKTOP="$PREFIX/share/applications/$APP.desktop"
ICONS="$PREFIX/share/icons/hicolor"

refresh_menus() {
    $SUDO update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
    $SUDO gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
}

# ---- désinstallation ----
if [ "$UNINSTALL" = 1 ]; then
    step "Désinstallation de RustScanner ($PREFIX)" "Uninstalling RustScanner ($PREFIX)"
    $SUDO rm -f "$BIN" "$DESKTOP" "$ICONS/scalable/apps/$APP.svg"
    for size in 32x32 128x128 256x256; do
        $SUDO rm -f "$ICONS/$size/apps/$APP.png"
    done
    refresh_menus
    say "Terminé. Les réglages (~/.config/rustscanner) sont conservés ; supprimez ce dossier pour tout effacer." \
        "Done. Settings (~/.config/rustscanner) are kept; delete that folder to remove everything."
    exit 0
fi

# ---- dépendances système ----
install_deps() {
    step "Installation des paquets système" "Installing system packages"
    if [ "$(id -u)" = 0 ]; then S=""; else S=sudo; fi
    if command -v apt-get >/dev/null; then
        $S apt-get update
        $S apt-get install -y libsane1 sane-utils sane-airscan cups-client \
            build-essential pkg-config curl libwebkit2gtk-4.1-dev libgtk-3-dev \
            libayatana-appindicator3-dev librsvg2-dev libssl-dev
    elif command -v dnf >/dev/null; then
        $S dnf install -y sane-backends sane-backends-libs sane-airscan cups-client \
            gcc pkgconf-pkg-config curl webkit2gtk4.1-devel gtk3-devel \
            libappindicator-gtk3-devel librsvg2-devel openssl-devel
    elif command -v pacman >/dev/null; then
        $S pacman -S --needed --noconfirm sane sane-airscan cups \
            base-devel pkgconf curl webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg openssl
    elif command -v zypper >/dev/null; then
        $S zypper install -y sane-backends sane-airscan cups-client \
            gcc pkg-config curl webkit2gtk3-soup2-devel gtk3-devel \
            libappindicator3-devel librsvg-devel libopenssl-devel
    else
        warn "Gestionnaire de paquets non reconnu : installez SANE et WebKitGTK 4.1 vous-même." \
             "Unknown package manager: install SANE and WebKitGTK 4.1 yourself."
    fi
}
[ "$DEPS" = 1 ] && install_deps

# ---- compilation ----
cd "$DIR"
if [ "$BUILD" = 1 ]; then
    command -v cargo >/dev/null || die \
        "Rust est introuvable : installez-le depuis https://rustup.rs puis relancez ce script." \
        "Rust not found: install it from https://rustup.rs and run this script again."
    if command -v rustup >/dev/null && ! rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
        step "Ajout de la cible wasm32-unknown-unknown" "Adding the wasm32-unknown-unknown target"
        rustup target add wasm32-unknown-unknown
    fi
    if ! command -v trunk >/dev/null; then
        step "Installation de trunk" "Installing trunk"
        cargo install --locked trunk
    fi
    if ! cargo tauri --version >/dev/null 2>&1; then
        step "Installation de tauri-cli" "Installing tauri-cli"
        cargo install --locked tauri-cli --version '^2'
    fi
    pkg-config --exists webkit2gtk-4.1 2>/dev/null || warn \
        "WebKitGTK 4.1 (paquet de développement) semble absent ; relancez avec --deps en cas d'échec." \
        "WebKitGTK 4.1 (development package) seems missing; run again with --deps if the build fails."
    step "Compilation (quelques minutes la première fois)" "Building (a few minutes the first time)"
    cargo tauri build --no-bundle
fi
[ -x "target/release/$APP" ] || die \
    "Binaire introuvable : target/release/$APP (lancez sans --no-build)." \
    "Binary not found: target/release/$APP (run without --no-build)."

# ---- installation ----
step "Installation dans $PREFIX" "Installing into $PREFIX"
$SUDO mkdir -p "$PREFIX/bin" "$PREFIX/share/applications" "$ICONS/scalable/apps"
$SUDO install -m 755 "target/release/$APP" "$BIN"
$SUDO install -m 644 assets/icon.svg "$ICONS/scalable/apps/$APP.svg"
for pair in "32x32:32x32.png" "128x128:128x128.png" "256x256:128x128@2x.png"; do
    size="${pair%%:*}"
    file="src-tauri/icons/${pair#*:}"
    if [ -f "$file" ]; then
        $SUDO mkdir -p "$ICONS/$size/apps"
        $SUDO install -m 644 "$file" "$ICONS/$size/apps/$APP.png"
    fi
done
TMP="$(mktemp)"
cat > "$TMP" <<DESKTOP
[Desktop Entry]
Type=Application
Name=RustScanner
GenericName=Document scanner
GenericName[fr]=Numérisation de documents
Comment=Scan documents and photos to PDF or images
Comment[fr]=Numériser des documents et des photos en PDF ou en images
Exec=$BIN
Icon=$APP
Terminal=false
Categories=Graphics;Scanning;
Keywords=scan;scanner;sane;pdf;skanpage;document;numériser;numérisation;
StartupWMClass=$APP
DESKTOP
$SUDO install -m 644 "$TMP" "$DESKTOP"
rm -f "$TMP"
refresh_menus

# ---- vérifications utiles à l'exécution ----
has_sane() {
    for ldc in ldconfig /sbin/ldconfig /usr/sbin/ldconfig; do
        "$ldc" -p 2>/dev/null | grep -q 'libsane\.so\.1' && return 0
    done
    for d in /usr/lib /usr/lib64 /usr/lib/x86_64-linux-gnu /usr/lib/aarch64-linux-gnu /usr/local/lib; do
        [ -e "$d/libsane.so.1" ] && return 0
    done
    return 1
}
if ! has_sane; then
    warn "SANE n'est pas installé : RustScanner ne pourra pas piloter de scanner (relancez avec --deps)." \
         "SANE is not installed: RustScanner will not be able to drive scanners (run again with --deps)."
fi
if getent group scanner >/dev/null 2>&1 && ! id -nG | tr ' ' '\n' | grep -qx scanner; then
    warn "Pour les scanners USB, ajoutez-vous au groupe « scanner » : sudo usermod -aG scanner $USER (puis reconnectez-vous)." \
         "For USB scanners, add yourself to the 'scanner' group: sudo usermod -aG scanner $USER (then log in again)."
fi
case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) warn "$PREFIX/bin n'est pas dans le PATH : lancez RustScanner depuis le menu ou avec $BIN." \
            "$PREFIX/bin is not in PATH: start RustScanner from the menu or with $BIN." ;;
esac

say "RustScanner est installé. Lancez « $APP » ou cherchez RustScanner dans le menu des applications." \
    "RustScanner is installed. Run '$APP' or look for RustScanner in the applications menu."
