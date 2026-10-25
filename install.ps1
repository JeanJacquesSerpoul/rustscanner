<#
Installation de RustScanner sous Windows : outils (facultatif), compilation,
puis installation par l'installeur NSIS (pour l'utilisateur courant, sans
droits d'administrateur) ou MSI (pour tous les utilisateurs).

  .\install.ps1               compile et installe pour l'utilisateur courant
  .\install.ps1 -Deps         installe d'abord les outils manquants (winget)
  .\install.ps1 -System       installe pour tous les utilisateurs (MSI, administrateur)
  .\install.ps1 -NoBuild      utilise l'installeur déjà construit (target\release\bundle)
  .\install.ps1 -Silent       installe sans fenêtre
  .\install.ps1 -Uninstall    désinstalle

Depuis l'Explorateur ou cmd.exe : install.cmd accepte les mêmes options.
#>
param(
    [switch]$Deps,
    [switch]$System,
    [switch]$NoBuild,
    [switch]$Silent,
    [switch]$Uninstall,
    [switch]$Help
)
$ErrorActionPreference = 'Stop'
$Dir = $PSScriptRoot
$App = 'RustScanner'

# ---- messages bilingues (d'après la langue de Windows) ----
$FR = (Get-UICulture).TwoLetterISOLanguageName -eq 'fr'
function Say($fr, $en) { if ($FR) { $fr } else { $en } }
function Step($fr, $en) { Write-Host '==> ' -ForegroundColor Blue -NoNewline; Write-Host (Say $fr $en) }
function Warn($fr, $en) { Write-Host '!! ' -ForegroundColor Yellow -NoNewline; Write-Host (Say $fr $en) }
function Die($fr, $en) { Write-Host 'xx ' -ForegroundColor Red -NoNewline; Write-Host (Say $fr $en); exit 1 }
function Has($cmd) { [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }

if ($Help) {
    Say @"
Usage : .\install.ps1 [-Deps] [-System] [-NoBuild] [-Silent] [-Uninstall]

  -Deps       installe les outils manquants avec winget (Rust, outils de compilation C++)
  -System     installe pour tous les utilisateurs (MSI, droits d'administrateur)
  -NoBuild    ne recompile pas : utilise l'installeur de target\release\bundle
  -Silent     installation ou désinstallation sans fenêtre
  -Uninstall  supprime l'application installée
"@ @"
Usage: .\install.ps1 [-Deps] [-System] [-NoBuild] [-Silent] [-Uninstall]

  -Deps       install missing tools with winget (Rust, C++ build tools)
  -System     install for every user (MSI, administrator rights)
  -NoBuild    do not rebuild: use the installer in target\release\bundle
  -Silent     install or uninstall without any window
  -Uninstall  remove the installed application
"@
    exit 0
}

# ---- désinstallation ----
function Installed {
    $keys = @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
    )
    Get-ItemProperty $keys -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq $App }
}

if ($Uninstall) {
    $entries = @(Installed)
    if ($entries.Count -eq 0) {
        Say "RustScanner n'est pas installé." 'RustScanner is not installed.'
        exit 0
    }
    foreach ($e in $entries) {
        Step "Désinstallation de RustScanner $($e.DisplayVersion)" "Uninstalling RustScanner $($e.DisplayVersion)"
        if ($e.UninstallString -match '\{[0-9A-Fa-f-]+\}') {
            # MSI : droits d'administrateur demandés par msiexec
            $quiet = if ($Silent) { '/qn' } else { '/qb' }
            Start-Process msiexec.exe -ArgumentList "/x $($Matches[0]) $quiet" -Wait -Verb RunAs
        } else {
            $run = @{ FilePath = $e.UninstallString.Trim('"'); Wait = $true }
            if ($Silent) { $run.ArgumentList = '/S' }
            Start-Process @run
        }
    }
    Say "Terminé. Les réglages ($env:APPDATA\rustscanner) sont conservés ; supprimez ce dossier pour tout effacer." `
        "Done. Settings ($env:APPDATA\rustscanner) are kept; delete that folder to remove everything."
    exit 0
}

# ---- outils de compilation ----
function Find-Msvc {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { return $false }
    $path = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    [bool]$path
}

function Refresh-Path {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
}

if ($Deps -and -not $NoBuild) {
    Step 'Installation des outils manquants' 'Installing missing tools'
    if (-not (Has winget)) {
        Die "winget est introuvable : installez « App Installer » depuis le Microsoft Store, ou installez Rust (https://rustup.rs) vous-même." `
            "winget not found: install 'App Installer' from the Microsoft Store, or install Rust (https://rustup.rs) yourself."
    }
    if (-not (Find-Msvc)) {
        Step 'Outils de compilation C++ (Visual Studio Build Tools)' 'C++ build tools (Visual Studio Build Tools)'
        winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --accept-source-agreements `
            --override '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
    }
    if (-not (Has cargo)) {
        Step 'Rust (rustup)' 'Rust (rustup)'
        winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
        Refresh-Path
        $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
        if (Test-Path $cargoBin) { $env:Path = "$cargoBin;$env:Path" }
    }
}

# ---- compilation ----
Set-Location $Dir
$bundle = if ($System) { 'msi' } else { 'nsis' }
if (-not $NoBuild) {
    if (-not (Has cargo)) {
        Die 'Rust est introuvable : relancez avec -Deps, ou installez-le depuis https://rustup.rs.' `
            'Rust not found: run again with -Deps, or install it from https://rustup.rs.'
    }
    if (-not (Find-Msvc)) {
        Warn 'Les outils de compilation C++ (MSVC) semblent absents ; relancez avec -Deps en cas d''échec.' `
             'The C++ build tools (MSVC) seem missing; run again with -Deps if the build fails.'
    }
    if ((Has rustup) -and -not (rustup target list --installed | Select-String -Quiet 'wasm32-unknown-unknown')) {
        Step 'Ajout de la cible wasm32-unknown-unknown' 'Adding the wasm32-unknown-unknown target'
        rustup target add wasm32-unknown-unknown
    }
    if (-not (Has trunk)) {
        Step 'Installation de trunk' 'Installing trunk'
        cargo install --locked trunk
    }
    $tauri = $false
    try { cargo tauri --version *> $null; $tauri = $LASTEXITCODE -eq 0 } catch { }
    if (-not $tauri) {
        Step 'Installation de tauri-cli' 'Installing tauri-cli'
        cargo install --locked tauri-cli --version '^2'
    }
    Step 'Compilation (quelques minutes la première fois)' 'Building (a few minutes the first time)'
    cargo tauri build --bundles $bundle
    if ($LASTEXITCODE -ne 0) { Die 'La compilation a échoué.' 'The build failed.' }
}

# un MSI par langue (fr-FR, en-US) ; un seul installeur NSIS bilingue
$pattern = if ($System) { 'target\release\bundle\msi\*.msi' } else { 'target\release\bundle\nsis\*-setup.exe' }
$lang = if ($FR) { 'fr-FR' } else { 'en-US' }
$installer = Get-ChildItem (Join-Path $Dir $pattern) -ErrorAction SilentlyContinue |
    Sort-Object @{ Expression = { $_.Name -match $lang }; Descending = $true }, @{ Expression = 'LastWriteTime'; Descending = $true } |
    Select-Object -First 1
if (-not $installer) {
    Die "Installeur introuvable ($pattern) : lancez sans -NoBuild." "Installer not found ($pattern): run without -NoBuild."
}

# ---- installation ----
Step "Installation : $($installer.Name)" "Installing: $($installer.Name)"
if ($System) {
    $quiet = if ($Silent) { '/qn' } else { '/qb' }
    $p = Start-Process msiexec.exe -ArgumentList "/i `"$($installer.FullName)`" $quiet" -Wait -PassThru -Verb RunAs
} else {
    $run = @{ FilePath = $installer.FullName; Wait = $true; PassThru = $true }
    if ($Silent) { $run.ArgumentList = '/S' }
    $p = Start-Process @run
}
if ($p.ExitCode -ne 0) {
    Die "L'installeur s'est terminé avec le code $($p.ExitCode)." "The installer exited with code $($p.ExitCode)."
}

# ---- vérifications utiles à l'exécution ----
$wia = Get-Service stisvc -ErrorAction SilentlyContinue
if (-not $wia) {
    Warn "Le service « Acquisition d'image Windows (WIA) » est absent : RustScanner ne pourra pas piloter de scanner." `
         "The 'Windows Image Acquisition (WIA)' service is missing: RustScanner will not be able to drive scanners."
} elseif ($wia.StartType -eq 'Disabled') {
    Warn "Le service « Acquisition d'image Windows (WIA) » est désactivé : réactivez-le dans services.msc." `
         "The 'Windows Image Acquisition (WIA)' service is disabled: enable it again in services.msc."
} else {
    try {
        $scanners = @((New-Object -ComObject WIA.DeviceManager).DeviceInfos | Where-Object { $_.Type -eq 1 })
        if ($scanners.Count -eq 0) {
            Warn 'Aucun scanner détecté : branchez-le, ou ajoutez-le dans Paramètres > Bluetooth et appareils > Imprimantes et scanners.' `
                 'No scanner found: plug it in, or add it in Settings > Bluetooth & devices > Printers & scanners.'
        }
    } catch { }
}

Say 'RustScanner est installé. Lancez-le depuis le menu Démarrer.' 'RustScanner is installed. Start it from the Start menu.'
