# Programı bu bilgisayarda derler, imzalar ve GitHub sürümü olarak yayımlar. GitHub Actions
# kullanılamadığında (ya da imza anahtarını GitHub'a vermek istemediğinizde) iş akışının yaptığını
# elle yapar.
#
#   powershell -ExecutionPolicy Bypass -File yayimla.ps1 -Anahtar <gizli anahtar dosyası>
#   powershell -ExecutionPolicy Bypass -File yayimla.ps1 -Anahtar <dosya> -YalnizcaDerle   # yayımlamaz
#
# Sürüm numarası Cargo.toml'daki `version` satırından okunur; o sürüm zaten yayımlanmışsa durur.
# Gerekli: Rust (stable-x86_64-pc-windows-gnu) ve yayımlamak için GitHub CLI (gh).
param([Parameter(Mandatory)][string]$Anahtar, [switch]$YalnizcaDerle)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$depo = 'saffetcelik/hfix-windows-11-araclari'

function Adim([string]$ne, [scriptblock]$komut) {
    Write-Host "==> $ne"
    & $komut
    if ($LASTEXITCODE -ne 0) { throw "$ne başarısız" }
}
# Program pencere alt sistemiyle derlendiği için çıkış kodu Start-Process ile alınır.
function Calistir([string]$exe, [string[]]$arg) {
    (Start-Process $exe -ArgumentList $arg -Wait -PassThru).ExitCode
}

if (-not (Test-Path $Anahtar)) { throw "imza anahtarı bulunamadı: $Anahtar" }
$Anahtar = (Resolve-Path $Anahtar).Path
$surum = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value

Adim 'testler' { cargo +stable-x86_64-pc-windows-gnu test --release --locked }
Adim "HFixWin11.exe $surum" { cargo +stable-x86_64-pc-windows-gnu build --release --locked }

$hedef = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PSScriptRoot 'target' }
New-Item -ItemType Directory -Force release | Out-Null
Copy-Item (Join-Path $hedef 'release\HFixWin11.exe') release\HFixWin11.exe -Force
$exe = (Resolve-Path release\HFixWin11.exe).Path
$sig = "$exe.sig"

Write-Host '==> imzalama'
$kod = Calistir $exe @('--imzala', "`"$Anahtar`"", "`"$exe`"", "`"$sig`"")
if ($kod -eq 2) { throw 'Bu anahtar programa gömülü açık anahtarla eşleşmiyor; kurulu programlar bu sürümü kabul etmez.' }
if ($kod -ne 0) { throw "imzalama başarısız (kod $kod)" }
Write-Host '==> imza programdaki açık anahtarla doğrulanıyor'
if ((Calistir $exe @('--imza-dogrula', "`"$exe`"", "`"$sig`"")) -ne 0) { throw 'imza doğrulanamadı' }
Get-ChildItem release | ForEach-Object { '{0,-22} {1,10:N1} KB' -f $_.Name, ($_.Length / 1KB) }

if ($YalnizcaDerle) { return }

gh release view "v$surum" --repo $depo *> $null
if ($LASTEXITCODE -eq 0) { throw "v$surum zaten yayımlanmış. Cargo.toml'daki sürümü artırın." }
if (git status --porcelain) { throw 'Kaydedilmemiş değişiklik var; önce commit edip gönderin.' }
Adim 'gönderim (git push)' { git push origin HEAD }

$not = @'
**Kurulum:** `HFixWin11.exe` dosyasını indirip çalıştırın. Program kendini kurar, masaüstüne ve Başlat menüsüne kısayol koyar. Kurulu sürümler bu sürüme kendiliğinden güncellenir.

**Install:** download and run `HFixWin11.exe`. It installs itself and puts a shortcut on the desktop and in the Start menu. Installed copies update to this release automatically.

Program henüz kod imzalı olmadığı için Windows SmartScreen ilk açılışta "Ek bilgi → Yine de çalıştır" sorabilir. / The program is not code-signed yet, so Windows SmartScreen may ask "More info → Run anyway" on first start.
'@
$notDosyasi = Join-Path $env:TEMP 'hfix-surum-notu.md'
[IO.File]::WriteAllText($notDosyasi, $not, (New-Object Text.UTF8Encoding($false)))
Adim "v$surum yayımlanıyor" {
    gh release create "v$surum" release\HFixWin11.exe release\HFixWin11.exe.sig --repo $depo `
        --target (git rev-parse HEAD) --title "HFix Windows 11 Araçları $surum" --notes-file $notDosyasi --generate-notes
}
Remove-Item $notDosyasi -Force
