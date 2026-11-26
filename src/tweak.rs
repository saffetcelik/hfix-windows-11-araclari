//! Windows hizlandirma ayarlari (Win11Debloat ve AtlasOS'tan derlenen,
//! guvenligi azaltmayan ayarlar) ve gereksiz uygulamalarin kaldirilmasi.
//! Her ayarin onceki degeri ayar-yedek.txt dosyasina yazilir ve geri alinabilir.

use crate::reg::{Key, HKCU, HKLM};
use crate::win;
use std::path::{Path, PathBuf};

pub enum V {
    D(u32),
    S(&'static str),
}

pub enum Op {
    /// Kayit degeri: (HKLM mi, anahtar, ad, deger)
    R(bool, &'static str, &'static str, V),
    /// HKCU altinda bos varsayilan degerli anahtar (eski sag tik menusu).
    Key(&'static str),
    /// Devre disi birakilacak zamanlanmis gorev.
    Task(&'static str),
    /// Komut: (uygula, geri al, durum icin HKLM anahtari, ad, uygulanmis degeri)
    Cmd(&'static [&'static str], &'static [&'static str], &'static str, &'static str, u32),
}

pub struct Tweak {
    pub id: &'static str,
    pub group: usize,
    pub name: &'static str,
    pub desc: &'static str,
    /// Cogu kullanici icin onerilir.
    pub rec: bool,
    pub ops: Vec<Op>,
}

pub const GROUPS: [(&str, &str); 5] = [
    ("Gizlilik ve telemetri", "Windows'un Microsoft'a kullanım verisi göndermesini ve arka planda veri toplamasını kapatır."),
    ("Reklam, öneri ve yapay zekâ", "Başlat, kilit ekranı ve Ayarlar'daki reklamları, Copilot'u ve otomatik kurulan uygulamaları kapatır."),
    ("Performans", "Arka planda boşuna çalışan özellikleri kapatır, açılış ve menü gecikmelerini kaldırır."),
    ("Görünüm ve kullanım", "Kişisel tercihler: eski sağ tık menüsü, görev çubuğu, Gezgin ayarları."),
    ("Microsoft Edge", "Edge'in arka planda çalışmasını, reklamlarını ve veri göndermesini kapatır."),
];

/// GROUPS ile ayni sirada Ingilizce basliklar ve aciklamalar.
pub const GROUPS_EN: [(&str, &str); 5] = [
    ("Privacy and telemetry", "Stops Windows from sending usage data to Microsoft and collecting data in the background."),
    ("Ads, suggestions and AI", "Turns off ads in Start, the lock screen and Settings, Copilot and automatically installed apps."),
    ("Performance", "Turns off features that run in the background for nothing and removes startup and menu delays."),
    ("Appearance and usability", "Personal preferences: classic right-click menu, taskbar, File Explorer settings."),
    ("Microsoft Edge", "Stops Edge from running in the background, showing ads and sending data."),
];

/// Grubun gecerli dildeki (basligi, aciklamasi).
pub fn group(i: usize) -> (&'static str, &'static str) {
    if crate::lang::en() {
        GROUPS_EN[i]
    } else {
        GROUPS[i]
    }
}

const CDM: &str = r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";
const ADV: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
const EDGE: &str = r"SOFTWARE\Policies\Microsoft\Edge";
const WAI: &str = r"Software\Policies\Microsoft\Windows\WindowsAI";

fn u(path: &'static str, name: &'static str, v: u32) -> Op {
    Op::R(false, path, name, V::D(v))
}
fn m(path: &'static str, name: &'static str, v: u32) -> Op {
    Op::R(true, path, name, V::D(v))
}
fn us(path: &'static str, name: &'static str, v: &'static str) -> Op {
    Op::R(false, path, name, V::S(v))
}

/// Ad ve aciklama iki dilde verilir (once Turkce, sonra Ingilizce); gecerli dildeki secilir.
#[allow(clippy::too_many_arguments)]
fn t(
    id: &'static str,
    group: usize,
    rec: bool,
    name: &'static str,
    name_en: &'static str,
    desc: &'static str,
    desc_en: &'static str,
    ops: Vec<Op>,
) -> Tweak {
    let en = crate::lang::en();
    Tweak {
        id,
        group,
        name: if en { name_en } else { name },
        desc: if en { desc_en } else { desc },
        rec,
        ops,
    }
}

pub fn all() -> Vec<Tweak> {
    vec![
        // ------------------------------------------------ gizlilik
        t("telemetri", 0, true, "Telemetriyi en düşük düzeye indir", "Reduce telemetry to the minimum",
          "Windows'un kullanım ve tanılama verisi göndermesini sürümün izin verdiği en düşük düzeye çeker.",
          "Lowers the usage and diagnostic data Windows sends to the lowest level your edition allows.",
          vec![
              m(r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "AllowTelemetry", 0),
              m(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection", "AllowTelemetry", 0),
          ]),
        t("telgorev", 0, true, "Veri toplayan zamanlanmış görevleri kapat", "Disable data-collecting scheduled tasks",
          "Uyumluluk değerlendirici, müşteri deneyimi programı ve disk tanılama görevleri; arka planda disk ve işlemci kullanır.",
          "Compatibility appraiser, customer experience program and disk diagnostic tasks; they use disk and CPU in the background.",
          vec![
              Op::Task(r"\Microsoft\Windows\Application Experience\Microsoft Compatibility Appraiser"),
              Op::Task(r"\Microsoft\Windows\Application Experience\ProgramDataUpdater"),
              Op::Task(r"\Microsoft\Windows\Customer Experience Improvement Program\Consolidator"),
              Op::Task(r"\Microsoft\Windows\Customer Experience Improvement Program\UsbCeip"),
              Op::Task(r"\Microsoft\Windows\DiskDiagnostic\Microsoft-Windows-DiskDiagnosticDataCollector"),
              Op::Task(r"\Microsoft\Windows\Feedback\Siuf\DmClient"),
              Op::Task(r"\Microsoft\Windows\Feedback\Siuf\DmClientOnScenarioDownload"),
          ]),
        t("reklamkimligi", 0, true, "Reklam kimliğini ve kişiselleştirilmiş deneyimleri kapat", "Turn off the advertising ID and tailored experiences",
          "Uygulamaların size özel reklam göstermek için kullandığı kimliği ve tanılama verisine dayalı önerileri kapatır.",
          "Turns off the ID apps use to show you personalized ads and the suggestions based on diagnostic data.",
          vec![
              u(r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled", 0),
              u(r"Software\Microsoft\Windows\CurrentVersion\Privacy", "TailoredExperiencesWithDiagnosticDataEnabled", 0),
          ]),
        t("etkinlik", 0, true, "Etkinlik geçmişini kapat", "Turn off activity history",
          "Açtığınız dosya ve uygulamaların kaydının tutulup Microsoft hesabına gönderilmesini kapatır.",
          "Stops Windows from recording the files and apps you open and sending that record to your Microsoft account.",
          vec![
              m(r"SOFTWARE\Policies\Microsoft\Windows\System", "PublishUserActivities", 0),
              m(r"SOFTWARE\Policies\Microsoft\Windows\System", "UploadUserActivities", 0),
          ]),
        t("yazma", 0, true, "Yazma, mürekkep ve konuşma verisi toplamayı kapat", "Turn off typing, inking and speech data collection",
          "Klavye yazım verisi, el yazısı ve çevrimiçi konuşma tanıma verisinin toplanmasını kapatır.",
          "Stops the collection of keyboard typing, handwriting and online speech recognition data.",
          vec![
              u(r"Software\Microsoft\Input\TIPC", "Enabled", 0),
              u(r"Software\Microsoft\InputPersonalization", "RestrictImplicitInkCollection", 1),
              u(r"Software\Microsoft\InputPersonalization", "RestrictImplicitTextCollection", 1),
              u(r"Software\Microsoft\InputPersonalization\TrainedDataStore", "HarvestContacts", 0),
              u(r"Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy", "HasAccepted", 0),
          ]),
        t("geribildirim", 0, true, "Geri bildirim isteklerini kapat", "Turn off feedback requests",
          "Windows'un \"deneyiminizi değerlendirin\" sorularını kapatır.",
          "Stops Windows from asking you to \"rate your experience\".",
          vec![u(r"Software\Microsoft\Siuf\Rules", "NumberOfSIUFInPeriod", 0)]),
        t("hataraporu", 0, true, "Hata raporlarının Microsoft'a gönderilmesini kapat", "Stop sending error reports to Microsoft",
          "Çöken programların raporları artık gönderilmez.",
          "Reports about crashed programs are no longer sent.",
          vec![m(r"SOFTWARE\Microsoft\Windows\Windows Error Reporting", "Disabled", 1)]),
        t("uygulamaizleme", 0, false, "Uygulama başlatma izlemesini kapat", "Turn off app launch tracking",
          "Başlat menüsündeki \"en çok kullanılanlar\" listesi de kapanır.",
          "The \"most used\" list in the Start menu is turned off as well.",
          vec![u(ADV, "Start_TrackProgs", 0)]),
        t("konum", 0, false, "Konum servisini kapat", "Turn off location services",
          "Hava durumu ve harita gibi uygulamalar konumunuzu bulamaz.",
          "Apps such as weather and maps can no longer find your location.",
          vec![m(r"SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "DisableLocation", 1)]),
        // ------------------------------------------------ reklam / yapay zeka
        t("oneriler", 1, true, "Reklam, ipucu ve önerileri kapat", "Turn off ads, tips and suggestions",
          "Başlat menüsü, kilit ekranı, Ayarlar ve Gezgin'deki önerilen uygulamalar, ipuçları ve Microsoft 365 reklamları.",
          "Suggested apps, tips and Microsoft 365 ads in the Start menu, lock screen, Settings and File Explorer.",
          vec![
              u(CDM, "SubscribedContent-310093Enabled", 0),
              u(CDM, "SubscribedContent-338387Enabled", 0),
              u(CDM, "SubscribedContent-338388Enabled", 0),
              u(CDM, "SubscribedContent-338389Enabled", 0),
              u(CDM, "SubscribedContent-338393Enabled", 0),
              u(CDM, "SubscribedContent-353694Enabled", 0),
              u(CDM, "SubscribedContent-353696Enabled", 0),
              u(CDM, "SubscribedContent-353698Enabled", 0),
              u(CDM, "SystemPaneSuggestionsEnabled", 0),
              u(CDM, "SoftLandingEnabled", 0),
              u(CDM, "RotatingLockScreenOverlayEnabled", 0),
              u(ADV, "Start_IrisRecommendations", 0),
              u(ADV, "ShowSyncProviderNotifications", 0),
              u(ADV, "Start_AccountNotifications", 0),
              u(r"Software\Microsoft\Windows\CurrentVersion\UserProfileEngagement", "ScoobeSystemSettingEnabled", 0),
              m(r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableConsumerAccountStateContent", 1),
          ]),
        t("otokurulum", 1, true, "Tanıtım uygulamalarının kendiliğinden kurulmasını kapat", "Stop promotional apps from installing themselves",
          "Candy Crush gibi uygulamaların siz istemeden yüklenmesini engeller.",
          "Prevents apps such as Candy Crush from being installed without your consent.",
          vec![
              u(CDM, "SilentInstalledAppsEnabled", 0),
              u(CDM, "PreInstalledAppsEnabled", 0),
              u(CDM, "OemPreInstalledAppsEnabled", 0),
              m(r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableWindowsConsumerFeatures", 1),
          ]),
        t("bing", 1, true, "Başlat aramasında Bing web sonuçlarını kapat", "Turn off Bing web results in Start search",
          "Başlat'ta arama yalnızca bilgisayarınızda yapılır; daha hızlı açılır.",
          "Start search looks only on your computer and opens faster.",
          vec![
              u(r"Software\Policies\Microsoft\Windows\Explorer", "DisableSearchBoxSuggestions", 1),
              u(r"Software\Microsoft\Windows\CurrentVersion\Search", "BingSearchEnabled", 0),
          ]),
        t("copilot", 1, true, "Copilot'u kapat", "Turn off Copilot",
          "Görev çubuğundaki Copilot düğmesini ve Windows Copilot'u kapatır.",
          "Turns off the Copilot button on the taskbar and Windows Copilot.",
          vec![
              u(ADV, "ShowCopilotButton", 0),
              u(r"Software\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot", 1),
              m(r"SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot", 1),
          ]),
        t("recall", 1, true, "Recall ve Click to Do'yu kapat", "Turn off Recall and Click to Do",
          "Ekranın düzenli görüntüsünü alıp yapay zekâyla çözümleyen özellikleri engeller.",
          "Blocks the features that take regular snapshots of your screen and analyze them with AI.",
          vec![
              u(WAI, "DisableAIDataAnalysis", 1),
              u(WAI, "DisableClickToDo", 1),
              m(r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableAIDataAnalysis", 1),
              m(r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "AllowRecallEnablement", 0),
              m(r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableClickToDo", 1),
          ]),
        t("widgets", 1, true, "Pencere öğelerini (Widgets / haberler) kapat", "Turn off Widgets (news)",
          "Görev çubuğundaki hava durumu ve haber panelini ve arka plan sürecini kapatır.",
          "Turns off the weather and news panel on the taskbar and its background process.",
          vec![m(r"SOFTWARE\Policies\Microsoft\Dsh", "AllowNewsAndInterests", 0)]),
        t("uygulamaai", 1, false, "Paint ve Not Defteri'ndeki yapay zekâ özelliklerini kapat", "Turn off AI features in Paint and Notepad",
          "Cocreator, üretken doldurma ve Not Defteri yapay zekâ düğmeleri.",
          "Cocreator, generative fill and the AI buttons in Notepad.",
          vec![
              m(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Paint", "DisableCocreator", 1),
              m(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Paint", "DisableGenerativeFill", 1),
              m(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Paint", "DisableImageCreator", 1),
              m(r"SOFTWARE\Policies\WindowsNotepad", "DisableAIFeatures", 1),
          ]),
        // ------------------------------------------------ performans
        t("arkaplan", 2, true, "Mağaza uygulamalarının arka planda çalışmasını kapat", "Stop Store apps from running in the background",
          "Kullanmadığınız Store uygulamaları arka planda bellek ve işlemci harcamaz. Bu uygulamaların bildirimleri gecikebilir.",
          "Store apps you are not using no longer consume memory and CPU in the background. Their notifications may be delayed.",
          vec![m(r"SOFTWARE\Policies\Microsoft\Windows\AppPrivacy", "LetAppsRunInBackground", 2)]),
        t("dvr", 2, true, "Oyun kaydını (Game DVR) kapat", "Turn off game recording (Game DVR)",
          "Arka planda ekran kaydına hazır bekleyen Xbox oyun kaydedicisini kapatır.",
          "Turns off the Xbox game recorder that waits in the background ready to record the screen.",
          vec![
              u(r"System\GameConfigStore", "GameDVR_Enabled", 0),
              u(r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled", 0),
              m(r"SOFTWARE\Policies\Microsoft\Windows\GameDVR", "AllowGameDVR", 0),
          ]),
        t("acilisgecikme", 2, true, "Başlangıç programlarının gecikmesini kaldır", "Remove the startup program delay",
          "Windows oturum açılınca başlangıç programlarını bekletmeden başlatır.",
          "Windows launches startup programs without waiting once you sign in.",
          vec![u(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize", "StartupDelayInMSec", 0)]),
        t("menugecikme", 2, true, "Menü açılma gecikmesini kaldır", "Remove the menu opening delay",
          "Alt menüler 400 ms beklemeden açılır. Oturumu kapatıp açınca geçerli olur.",
          "Submenus open without the 400 ms wait. Takes effect after you sign out and back in.",
          vec![us(r"Control Panel\Desktop", "MenuShowDelay", "0")]),
        t("teslim", 2, true, "Güncellemeleri başka bilgisayarlara yüklemeyi kapat", "Stop uploading updates to other computers",
          "Teslim İyileştirme'nin internetinizi kullanarak güncellemeleri başkalarına dağıtmasını kapatır.",
          "Stops Delivery Optimization from using your internet connection to share updates with others.",
          vec![m(r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization", "DODownloadMode", 0)]),
        t("saydamlik", 2, false, "Saydamlık efektlerini kapat", "Turn off transparency effects",
          "Görev çubuğu ve menülerdeki cam efekti; ekran kartına küçük bir yük bindirir.",
          "The glass effect on the taskbar and menus; it puts a small load on the graphics card.",
          vec![u(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency", 0)]),
        t("animasyon", 2, false, "Pencere ve görev çubuğu animasyonlarını kapat", "Turn off window and taskbar animations",
          "Pencereler anında açılır. Oturumu kapatıp açınca geçerli olur.",
          "Windows open instantly. Takes effect after you sign out and back in.",
          vec![
              us(r"Control Panel\Desktop\WindowMetrics", "MinAnimate", "0"),
              u(ADV, "TaskbarAnimations", 0),
          ]),
        t("agkisma", 2, false, "Multimedya ağ kısıtlamasını kaldır", "Remove multimedia network throttling",
          "Windows'un ses/video oynatırken ağ hızını sınırlamasını kaldırır ve ön plandaki programa daha çok işlemci ayırır.",
          "Stops Windows from limiting network speed while playing audio/video and gives the foreground program more CPU time.",
          vec![
              m(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile", "NetworkThrottlingIndex", 0xffff_ffff),
              m(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile", "SystemResponsiveness", 10),
          ]),
        t("hizlibaslat", 2, false, "Hızlı başlatmayı kapat", "Turn off fast startup",
          "Bilgisayar her kapatılışta gerçekten sıfırdan açılır; açılış biraz uzar ama uzun süre açık kalmanın yol açtığı yavaşlama birikmez.",
          "The computer truly starts from scratch after every shutdown; startup takes a little longer, but the slowdown caused by long uptime does not build up.",
          vec![m(r"SYSTEM\CurrentControlSet\Control\Session Manager\Power", "HiberbootEnabled", 0)]),
        t("hazirdabeklet", 2, false, "Hazırda bekletmeyi kapat", "Turn off hibernation",
          "C: diskinde RAM boyutuna yakın yer kaplayan hiberfil.sys dosyasını siler. Hazırda beklet ve hızlı başlatma kullanılamaz; uyku çalışır.",
          "Deletes hiberfil.sys, which takes up nearly as much space on drive C: as your RAM. Hibernate and fast startup become unavailable; sleep still works.",
          vec![Op::Cmd(&["powercfg", "/h", "off"], &["powercfg", "/h", "on"], r"SYSTEM\CurrentControlSet\Control\Power", "HibernateEnabled", 0)]),
        // ------------------------------------------------ gorunum
        t("uzanti", 3, true, "Dosya uzantılarını göster", "Show file extensions",
          "\"fatura.pdf.exe\" gibi belge kılığındaki programları fark etmenizi sağlar.",
          "Helps you spot programs disguised as documents, such as \"invoice.pdf.exe\".",
          vec![u(ADV, "HideFileExt", 0)]),
        t("gorevsonlandir", 3, true, "Görev çubuğu sağ tık menüsüne \"Görevi sonlandır\" ekle", "Add \"End task\" to the taskbar right-click menu",
          "Donan programı görev çubuğundan sağ tıklayıp kapatabilirsiniz.",
          "You can close a frozen program by right-clicking it on the taskbar.",
          vec![u(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings", "TaskbarEndTask", 1)]),
        t("yapiskan", 3, true, "Yapışkan Tuşlar kısayolunu kapat", "Turn off the Sticky Keys shortcut",
          "Shift'e 5 kez basınca çıkan pencere artık açılmaz.",
          "The window that pops up when you press Shift 5 times no longer appears.",
          vec![us(r"Control Panel\Accessibility\StickyKeys", "Flags", "506")]),
        t("eskimenu", 3, false, "Eski (tam) sağ tık menüsünü geri getir", "Bring back the classic (full) right-click menu",
          "\"Daha fazla seçenek göster\"e basmadan tüm seçenekler görünür. Oturumu kapatıp açınca geçerli olur.",
          "All options are shown without clicking \"Show more options\". Takes effect after you sign out and back in.",
          vec![Op::Key(r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32")]),
        t("solahizala", 3, false, "Görev çubuğunu sola hizala", "Align the taskbar to the left",
          "Başlat düğmesi Windows 10'daki gibi sol köşeye gelir.",
          "The Start button moves to the left corner, as in Windows 10.",
          vec![u(ADV, "TaskbarAl", 0)]),
        t("aramakutusu", 3, false, "Görev çubuğundaki arama kutusunu gizle", "Hide the search box on the taskbar",
          "Arama, Başlat'a basıp yazarak yine çalışır.",
          "Search still works: open Start and type.",
          vec![u(r"Software\Microsoft\Windows\CurrentVersion\Search", "SearchboxTaskbarMode", 0)]),
        t("gorevgorunumu", 3, false, "Görev görünümü düğmesini gizle", "Hide the Task view button",
          "Win+Tab kısayolu çalışmaya devam eder.",
          "The Win+Tab shortcut keeps working.",
          vec![u(ADV, "ShowTaskViewButton", 0)]),
        t("bubilgisayar", 3, false, "Gezgin \"Bu bilgisayar\" ile açılsın", "Open File Explorer to \"This PC\"",
          "Dosya Gezgini, Giriş sayfası yerine disklerin göründüğü sayfayla açılır.",
          "File Explorer opens to the page showing your drives instead of the Home page.",
          vec![u(ADV, "LaunchTo", 1)]),
        t("gizlidosya", 3, false, "Gizli dosyaları göster", "Show hidden files",
          "Gezgin gizli dosya ve klasörleri de listeler.",
          "File Explorer also lists hidden files and folders.",
          vec![u(ADV, "Hidden", 1)]),
        t("fareivme", 3, false, "Fare ivmesini kapat", "Turn off mouse acceleration",
          "\"İşaretçi hassasiyetini artır\" kapanır; imleç hareketi el hareketiyle birebir olur.",
          "\"Enhance pointer precision\" is turned off; the pointer moves exactly as your hand does.",
          vec![
              us(r"Control Panel\Mouse", "MouseSpeed", "0"),
              us(r"Control Panel\Mouse", "MouseThreshold1", "0"),
              us(r"Control Panel\Mouse", "MouseThreshold2", "0"),
          ]),
        // ------------------------------------------------ edge
        t("edgearka", 4, true, "Edge'in arka planda çalışmasını kapat", "Stop Edge from running in the background",
          "Edge kapalıyken bellekte bekleyen \"başlangıç artırma\" ve arka plan süreçlerini kapatır.",
          "Turns off \"startup boost\" and the background processes that stay in memory while Edge is closed.",
          vec![m(EDGE, "StartupBoostEnabled", 0), m(EDGE, "BackgroundModeEnabled", 0)]),
        t("edgereklam", 4, true, "Edge reklam ve önerilerini kapat", "Turn off Edge ads and suggestions",
          "Yeni sekme içerikleri, alışveriş yardımcısı, ödül programı ve \"varsayılan tarayıcı yap\" uyarıları.",
          "New tab content, the shopping assistant, the rewards program and \"set as default browser\" prompts.",
          vec![
              m(EDGE, "NewTabPageContentEnabled", 0),
              m(EDGE, "EdgeShoppingAssistantEnabled", 0),
              m(EDGE, "ShowRecommendationsEnabled", 0),
              m(EDGE, "HideFirstRunExperience", 1),
              m(EDGE, "ShowMicrosoftRewards", 0),
              m(EDGE, "DefaultBrowserSettingEnabled", 0),
              m(EDGE, "DefaultBrowserSettingsCampaignEnabled", 0),
              m(EDGE, "SpotlightExperiencesAndRecommendationsEnabled", 0),
              m(EDGE, "UserFeedbackAllowed", 0),
          ]),
        // ------------------------------------------------ winutil / optimizer'dan
        t("uzunyol", 2, false, "Uzun dosya yollarına izin ver", "Allow long file paths",
          "260 karakterden uzun yollarda \"yol çok uzun\" hatasını kaldırır (destekleyen programlarda).",
          "Removes the \"path too long\" error for paths over 260 characters (in programs that support it).",
          vec![m(r"SYSTEM\CurrentControlSet\Control\FileSystem", "LongPathsEnabled", 1)]),
        t("ayrintilibsod", 3, false, "Mavi ekranda ayrıntılı hata bilgisi göster", "Show detailed error information on the blue screen",
          "Üzgün yüz yerine hata kodunu ve parametrelerini gösterir; sorunun kaynağını bulmayı kolaylaştırır.",
          "Shows the error code and its parameters instead of the sad face; makes the cause easier to find.",
          vec![
              m(r"SYSTEM\CurrentControlSet\Control\CrashControl", "DisplayParameters", 1),
              m(r"SYSTEM\CurrentControlSet\Control\CrashControl", "DisableEmoticon", 1),
          ]),
        t("ayrintiliacilis", 3, false, "Açılış ve kapanışta ne yapıldığını göster", "Show what is happening during startup and shutdown",
          "\"Lütfen bekleyin\" yerine o an çalışan adımı yazar; yavaş açılışın nedenini görmeye yarar.",
          "Shows the step that is running instead of \"Please wait\"; helps you see why startup is slow.",
          vec![m(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "VerboseStatus", 1)]),
        t("pilyuzde", 3, false, "Görev çubuğunda pil yüzdesini göster", "Show the battery percentage in the taskbar",
          "Pil simgesinin yanına yüzdeyi yazar (yeni Windows 11 sürümlerinde).",
          "Shows the percentage next to the battery icon (on recent Windows 11 builds).",
          vec![u(ADV, "IsBatteryPercentageEnabled", 1)]),
        t("edgeveri", 4, true, "Edge'in tanılama verisi göndermesini kapat", "Stop Edge from sending diagnostic data",
          "Edge'in kullanım verisini ve gezinme geçmişine dayalı kişiselleştirmeyi kapatır.",
          "Turns off Edge usage data and personalization based on your browsing history.",
          vec![m(EDGE, "PersonalizationReportingEnabled", 0), m(EDGE, "DiagnosticData", 0)]),
        t("edgeai", 4, false, "Edge'deki Copilot ve yapay zekâ özelliklerini kapat", "Turn off Copilot and AI features in Edge",
          "Kenar çubuğu, Copilot düğmesi ve yazım yardımcısı.",
          "The sidebar, the Copilot button and the writing assistant.",
          vec![
              m(EDGE, "HubsSidebarEnabled", 0),
              m(EDGE, "CopilotPageContext", 0),
              m(EDGE, "ComposeInlineEnabled", 0),
              m(EDGE, "NewTabPageBingChatEnabled", 0),
              m(EDGE, "EdgeHistoryAISearchEnabled", 0),
          ]),
    ]
}

// ------------------------------------------------------------------ durum

fn task_file(path: &str) -> PathBuf {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    Path::new(&windir).join(format!("System32\\Tasks{path}"))
}

fn task_disabled(path: &str) -> Option<bool> {
    let b = std::fs::read(task_file(path)).ok()?;
    let text = if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        let w: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&w)
    } else {
        String::from_utf8_lossy(&b).into_owned()
    };
    let a = text.find("<Settings>")?;
    Some(text[a..].contains("<Enabled>false</Enabled>"))
}

fn root(hklm: bool) -> isize {
    if hklm {
        HKLM
    } else {
        HKCU
    }
}

/// Kayit degerinin simdiki hali: "-" (yok), "d:sayi" ya da "s:metin".
fn current(hklm: bool, path: &str, name: &str) -> String {
    let Some(k) = Key::open(root(hklm), path, false) else {
        return "-".into();
    };
    match k.get(name) {
        Some((4, d)) if d.len() >= 4 => format!("d:{}", u32::from_le_bytes([d[0], d[1], d[2], d[3]])),
        Some((1 | 2, d)) => format!("s:{}", crate::reg::to_string(&d)),
        _ => "-".into(),
    }
}

fn wanted(v: &V) -> String {
    match v {
        V::D(n) => format!("d:{n}"),
        V::S(s) => format!("s:{s}"),
    }
}

fn write(hklm: bool, path: &str, name: &str, val: &str) -> bool {
    if val == "-" {
        return match Key::open(root(hklm), path, true) {
            Some(k) => k.delete_value(name) || k.get(name).is_none(),
            None => true,
        };
    }
    let Some(k) = Key::create(root(hklm), path) else {
        return false;
    };
    match val.split_once(':') {
        Some(("d", n)) => k.set_u32(name, n.parse().unwrap_or(0)),
        Some(("s", s)) => k.set_str(name, s),
        _ => false,
    }
}

fn op_applied(op: &Op) -> bool {
    match op {
        Op::R(h, p, n, v) => current(*h, p, n) == wanted(v),
        Op::Key(p) => Key::open(HKCU, p, false).is_some(),
        // Bu Windows surumunde olmayan gorev: yapilacak bir sey yok.
        Op::Task(p) => task_disabled(p).unwrap_or(true),
        Op::Cmd(_, _, p, n, v) => Key::open(HKLM, p, false).and_then(|k| k.get_u32(n)) == Some(*v),
    }
}

pub fn applied(t: &Tweak) -> bool {
    t.ops.iter().all(op_applied)
}

// ------------------------------------------------------------------ yedek

fn backup_file() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("ayar-yedek.txt"))
}

fn backup() -> Vec<(String, usize, String)> {
    backup_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| {
            t.lines()
                .filter_map(|l| {
                    let mut p = l.splitn(3, '\t');
                    Some((p.next()?.to_string(), p.next()?.parse().ok()?, p.next()?.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn save_backup(b: &[(String, usize, String)]) {
    if let Some(p) = backup_file() {
        let t: String = b.iter().map(|x| format!("{}\t{}\t{}\n", x.0, x.1, x.2)).collect();
        let _ = std::fs::write(p, t);
    }
}

/// Ayari uygular; onceki degerleri yedekler. Tum adimlar basariliysa true.
pub fn apply(t: &Tweak) -> bool {
    let mut b = backup();
    let mut ok = true;
    for (i, op) in t.ops.iter().enumerate() {
        match op {
            Op::R(h, p, n, v) => {
                let cur = current(*h, p, n);
                let want = wanted(v);
                if cur == want {
                    continue;
                }
                if !b.iter().any(|x| x.0 == t.id && x.1 == i) {
                    b.push((t.id.to_string(), i, cur));
                }
                ok &= write(*h, p, n, &want);
            }
            Op::Key(p) => {
                ok &= Key::create(HKCU, p).is_some_and(|k| k.set_str("", ""));
            }
            Op::Task(p) => {
                if task_disabled(p) == Some(false) {
                    ok &= win::run_status("schtasks", &["/change", "/tn", p, "/disable"]);
                }
            }
            Op::Cmd(a, ..) => {
                ok &= win::run_status(a[0], &a[1..]);
            }
        }
    }
    save_backup(&b);
    ok
}

/// Ayari uygulanmadan onceki haline dondurur.
pub fn undo(t: &Tweak) -> bool {
    let mut b = backup();
    let mut ok = true;
    for (i, op) in t.ops.iter().enumerate() {
        match op {
            Op::R(h, p, n, _) => {
                // Yedek yoksa deger silinir: Windows varsayilani gecerli olur.
                let old = b
                    .iter()
                    .find(|x| x.0 == t.id && x.1 == i)
                    .map(|x| x.2.clone())
                    .unwrap_or_else(|| "-".into());
                ok &= write(*h, p, n, &old);
            }
            Op::Key(p) => {
                let parent = p.rsplit_once('\\').map_or(*p, |x| x.0);
                ok &= Key::delete_tree(HKCU, parent);
            }
            Op::Task(p) => {
                if task_disabled(p) == Some(true) {
                    ok &= win::run_status("schtasks", &["/change", "/tn", p, "/enable"]);
                }
            }
            Op::Cmd(_, u, ..) => {
                ok &= win::run_status(u[0], &u[1..]);
            }
        }
    }
    b.retain(|x| x.0 != t.id);
    save_backup(&b);
    ok
}

/// Sistem geri yukleme noktasi olusturur.
pub fn restore_point() -> bool {
    win::run_status(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            "Checkpoint-Computer -Description 'HFix Windows 11' -RestorePointType MODIFY_SETTINGS -ErrorAction Stop",
        ],
    )
}

// ------------------------------------------------------- gereksiz uygulamalar

#[derive(Clone)]
pub struct App {
    pub name: String,
    pub full: String,
    pub label: &'static str,
    pub rec: bool,
}

/// (paket adi on eki, aciklama, Ingilizce aciklama, kaldirilmasi onerilir mi)
const BLOAT: &[(&str, &str, &str, bool)] = &[
    ("clipchamp.clipchamp", "Clipchamp video düzenleyici", "Clipchamp video editor", true),
    ("microsoft.549981c3f5f10", "Cortana", "Cortana", true),
    ("microsoft.bing", "Bing haber / hava / finans uygulamaları", "Bing news / weather / finance apps", true),
    ("microsoft.copilot", "Copilot uygulaması", "Copilot app", true),
    ("microsoft.windows.aihub", "Yapay zekâ merkezi", "AI hub", true),
    ("microsoft.getstarted", "İpuçları", "Tips", true),
    ("microsoft.messaging", "Mesajlaşma", "Messaging", true),
    ("microsoft.microsoft3dviewer", "3B Görüntüleyici", "3D Viewer", true),
    ("microsoft.3dbuilder", "3D Builder", "3D Builder", true),
    ("microsoft.print3d", "Print 3D", "Print 3D", true),
    ("microsoft.microsoftofficehub", "Microsoft 365 (Office tanıtım uygulaması)", "Microsoft 365 (Office promotional app)", true),
    ("microsoft.microsoftsolitairecollection", "Solitaire oyunları", "Solitaire games", true),
    ("microsoft.mixedreality.portal", "Karma Gerçeklik Portalı", "Mixed Reality Portal", true),
    ("microsoft.office.onenote", "OneNote (Store sürümü)", "OneNote (Store version)", true),
    ("microsoft.oneconnect", "Mobil planlar", "Mobile plans", true),
    ("microsoft.powerautomatedesktop", "Power Automate", "Power Automate", true),
    ("microsoft.skypeapp", "Skype", "Skype", true),
    ("microsoft.todos", "Microsoft To Do", "Microsoft To Do", true),
    ("microsoft.windows.devhome", "Dev Home", "Dev Home", true),
    ("microsoft.windowsfeedbackhub", "Geri Bildirim Merkezi", "Feedback Hub", true),
    ("microsoft.windowsmaps", "Haritalar", "Maps", true),
    ("microsoft.xboxapp", "Xbox (eski uygulama)", "Xbox (old app)", true),
    ("microsoft.zunevideo", "Filmler ve TV", "Movies & TV", true),
    ("microsoftcorporationii.microsoftfamily", "Microsoft Aile Güvenliği", "Microsoft Family Safety", true),
    ("microsoftteams", "Microsoft Teams (kişisel)", "Microsoft Teams (personal)", true),
    ("msteams", "Microsoft Teams", "Microsoft Teams", true),
    ("microsoft.microsoftjournal", "Journal", "Journal", true),
    ("microsoft.pcmanager", "PC Manager", "PC Manager", true),
    ("king.com.", "King oyunları (Candy Crush vb.)", "King games (Candy Crush etc.)", true),
    ("spotifyab.spotifymusic", "Spotify (tanıtım kurulumu)", "Spotify (promotional install)", true),
    ("facebook.", "Facebook / Instagram", "Facebook / Instagram", true),
    ("bytedancepte.ltd.tiktok", "TikTok", "TikTok", true),
    ("4df9e0f8.netflix", "Netflix", "Netflix", true),
    ("amazonvideo.primevideo", "Prime Video", "Prime Video", true),
    ("disney.", "Disney+", "Disney+", true),
    ("linkedinforwindows", "LinkedIn", "LinkedIn", true),
    // Kullaniyor olabileceginiz uygulamalar: onerilmez, istege bagli.
    ("microsoft.outlookforwindows", "Yeni Outlook", "New Outlook", false),
    ("microsoft.windowscommunicationsapps", "Posta ve Takvim", "Mail and Calendar", false),
    ("microsoft.people", "Kişiler", "People", false),
    ("microsoft.yourphone", "Telefon Bağlantısı", "Phone Link", false),
    ("microsoftwindows.crossdevice", "Cihazlar arası deneyim", "Cross-device experience", false),
    ("microsoftwindows.client.webexperience", "Pencere öğeleri (Widgets) altyapısı", "Widgets platform", false),
    ("microsoft.gamingapp", "Xbox uygulaması", "Xbox app", false),
    ("microsoft.xboxgamingoverlay", "Xbox Game Bar", "Xbox Game Bar", false),
    ("microsoft.xboxgameoverlay", "Xbox oyun katmanı", "Xbox game overlay", false),
    ("microsoft.zunemusic", "Media Player", "Media Player", false),
    ("microsoft.windowsalarms", "Saat ve Alarmlar", "Clock and Alarms", false),
    ("microsoft.windowssoundrecorder", "Ses Kaydedici", "Sound Recorder", false),
    ("microsoft.microsoftstickynotes", "Yapışkan Notlar", "Sticky Notes", false),
    ("microsoft.windowscamera", "Kamera", "Camera", false),
    ("microsoftcorporationii.quickassist", "Hızlı Yardım", "Quick Assist", false),
    ("microsoft.whiteboard", "Whiteboard", "Whiteboard", false),
    ("e046963f.lenovocompanion", "Lenovo Vantage", "Lenovo Vantage", false),
    ("lenovocompanylimited.lenovovantageservice", "Lenovo Vantage servisi", "Lenovo Vantage service", false),
    ("dolbylaboratories.", "Dolby ses uygulaması", "Dolby audio app", false),
];

/// Yuklu Store uygulamalarindan gereksiz sayilabilecek olanlar.
pub fn apps() -> Vec<App> {
    let Some(out) = win::run_hidden(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            "Get-AppxPackage | ForEach-Object { $_.Name + '|' + $_.PackageFullName }",
        ],
    ) else {
        return Vec::new();
    };
    let en = crate::lang::en();
    let mut v: Vec<App> = out
        .lines()
        .filter_map(|l| l.trim().split_once('|'))
        .filter_map(|(name, full)| {
            let low = name.to_lowercase();
            let b = BLOAT.iter().find(|b| low.starts_with(b.0))?;
            Some(App {
                name: name.to_string(),
                full: full.to_string(),
                label: if en { b.2 } else { b.1 },
                rec: b.3,
            })
        })
        .collect();
    v.sort_by(|a, b| b.rec.cmp(&a.rec).then_with(|| a.label.cmp(b.label)));
    v
}

pub fn remove_app(a: &App) -> bool {
    // Paket adi yalnizca harf, rakam, nokta, alt cizgi ve tire icerir.
    if !a.full.chars().all(|c| c.is_ascii_alphanumeric() || "._-~".contains(c)) {
        return false;
    }
    win::run_status(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            &format!("Remove-AppxPackage -Package '{}' -ErrorAction Stop", a.full),
        ],
    )
}
