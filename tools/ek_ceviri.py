"""Sonradan eklenen az sayidaki metnin cevirilerini diller/<kod>.json dosyalarina yazar.

    python tools/ek_ceviri.py

Yeni bir metin eklendiginde buraya Ingilizce metni ve dillerdeki karsiliklarini ekleyin.
"""

import json
from pathlib import Path

DILLER = Path(__file__).resolve().parent.parent / "diller"

T = {
    "The Windows display language": {
        "zh": "Windows 显示语言", "zh-tw": "Windows 顯示語言", "es": "El idioma de visualización de Windows",
        "pt": "O idioma de exibição do Windows", "de": "Die Anzeigesprache von Windows", "fr": "La langue d'affichage de Windows",
        "ru": "Язык интерфейса Windows", "uk": "Мова інтерфейсу Windows", "it": "La lingua di visualizzazione di Windows",
        "nl": "De weergavetaal van Windows", "pl": "Język wyświetlania systemu Windows", "cs": "Jazyk zobrazení systému Windows",
        "id": "Bahasa tampilan Windows", "vi": "Ngôn ngữ hiển thị của Windows", "ja": "Windows の表示言語",
        "ko": "Windows 표시 언어", "ar": "لغة عرض Windows", "fa": "زبان نمایش Windows",
    },
    "Windows 10: {} settings that exist only on Windows 11 are not shown.": {
        "zh": "Windows 10：仅在 Windows 11 中提供的 {} 项设置未显示。",
        "es": "Windows 10: no se muestran {} ajustes que solo existen en Windows 11.",
        "pt": "Windows 10: {} configurações que só existem no Windows 11 não são exibidas.",
        "de": "Windows 10: {} Einstellungen, die es nur unter Windows 11 gibt, werden nicht angezeigt.",
        "fr": "Windows 10 : {} réglages propres à Windows 11 ne sont pas affichés.",
        "ru": "Windows 10: параметры, доступные только в Windows 11, скрыты: {}.",
        "it": "Windows 10: {} impostazioni disponibili solo in Windows 11 non vengono mostrate.",
        "ja": "Windows 10: Windows 11 にのみ存在する {} 個の設定は表示されません。",
        "ko": "Windows 10: Windows 11에만 있는 설정 {}개는 표시되지 않습니다.",
        "ar": "Windows 10: لا يتم عرض {} من الإعدادات المتوفرة في Windows 11 فقط.",
    },
    "This computer: {} (build {})": {
        "zh": "此电脑：{}（版本 {}）",
        "es": "Este equipo: {} (compilación {})",
        "pt": "Este computador: {} (compilação {})",
        "de": "Dieser PC: {} (Build {})",
        "fr": "Cet ordinateur : {} (build {})",
        "ru": "Этот компьютер: {} (сборка {})",
        "it": "Questo PC: {} (build {})",
        "ja": "この PC: {} (ビルド {})",
        "ko": "이 PC: {}(빌드 {})",
        "ar": "هذا الكمبيوتر: {} (الإصدار {})",
    },
}

for p in sorted(DILLER.glob("*.json")):
    code = p.stem
    if code == "en" or code.startswith("_"):
        continue
    d = json.loads(p.read_text(encoding="utf-8"))
    for en, tr in T.items():
        if code in tr:
            d[en] = tr[code]
    p.write_text(json.dumps(d, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
