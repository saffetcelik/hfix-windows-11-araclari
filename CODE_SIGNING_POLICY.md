# Kod imzalama politikası / Code signing policy

## Durum / Status

**TR:** Yayımlanan `HFixWin11.exe` dosyaları şu an Authenticode ile imzalı **değildir**. Ücretsiz açık kaynak kod imzalama için [SignPath Foundation](https://signpath.org)'a başvuru hazırlığı sürmektedir; başvuru onaylanana kadar bu belge yalnızca nasıl çalışacağımızı tanımlar.

Otomatik güncellemeler ise bundan bağımsız olarak bugün de korunur: her sürümün yanındaki `HFixWin11.exe.sig` dosyası sürüm numarasını, boyutu ve SHA-256 özetini bir Ed25519 anahtarıyla imzalar; program bunlar tutmadıkça hiçbir güncellemeyi kurmaz (bkz. [README](README.md#otomatik-güncelleme)).

**EN:** Released `HFixWin11.exe` files are **not** Authenticode-signed yet. We are preparing an application to the [SignPath Foundation](https://signpath.org) for free open-source code signing; until it is approved, this document only describes how we intend to work.

Automatic updates are protected independently of this today: the `HFixWin11.exe.sig` file next to every release signs the version, size and SHA-256 digest with an Ed25519 key, and the program installs no update unless they match.

## Kaynak ve derleme / Source and build

- Bütün kaynak kod bu depodadır ve [MIT](LICENSE) lisanslıdır; kapalı kaynak bileşen yoktur.
  All source code is in this repository under the [MIT](LICENSE) license; there are no proprietary components.
- Yayımlanan dosya yalnızca bu depodaki koddan, [`.github/workflows/surum.yml`](.github/workflows/surum.yml) iş akışıyla (ya da aynı adımları yapan [`yayimla.ps1`](yayimla.ps1) ile) derlenir.
  Released binaries are built only from the code in this repository, by the [`surum.yml`](.github/workflows/surum.yml) workflow (or [`yayimla.ps1`](yayimla.ps1), which runs the same steps).

## Roller / Team roles

| Rol / Role | Kim / Who |
|---|---|
| Committers and reviewers | [@saffetcelik](https://github.com/saffetcelik) |
| Approvers (sürüm onayı / release approval) | [@saffetcelik](https://github.com/saffetcelik) |

Depoya dışarıdan gelen katkılar ancak bir "committer" tarafından incelenip birleştirildikten sonra sürüme girer.
Outside contributions enter a release only after a committer has reviewed and merged them.

## Gizlilik / Privacy

**TR:** Program, kullanıcı açıkça istemedikçe başka ağ sistemlerine hiçbir bilgi aktarmaz. İki istisna vardır ve ikisi de Ayarlar'dan kapatılabilir ya da yalnızca düğmeye basınca çalışır: güncelleme denetimi için GitHub'a yapılan istek (yalnızca sürüm bilgisi indirilir, kullanıcıya ait veri gönderilmez) ve kullanıcının Risk Analizi sayfasında başlattığı Microsoft Defender taraması (Windows'un kendi aracı).

**EN:** This program will not transfer any information to other networked systems unless specifically requested by the user. There are two exceptions, both of which can be turned off in Settings or only run when a button is pressed: the request to GitHub for the update check (only release information is downloaded; no user data is sent), and the Microsoft Defender scan the user starts on the Risk Analysis page (Windows' own tool).
