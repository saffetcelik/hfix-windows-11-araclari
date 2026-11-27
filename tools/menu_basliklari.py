"""Sol menudeki bolum basliklarinin cevirilerini diller/<kod>.json dosyalarina ekler (bir kez calistirildi)."""

import json
from pathlib import Path

DILLER = Path(__file__).resolve().parent.parent / "diller"

T = {
    "MONITOR": {"zh": "监视", "es": "SUPERVISIÓN", "pt": "MONITORAMENTO", "de": "ÜBERWACHUNG", "fr": "SURVEILLANCE", "ru": "МОНИТОРИНГ", "it": "MONITORAGGIO", "ja": "監視", "ko": "모니터링", "ar": "المراقبة"},
    "SPEED UP": {"zh": "加速", "es": "ACELERAR", "pt": "ACELERAR", "de": "BESCHLEUNIGEN", "fr": "ACCÉLÉRER", "ru": "УСКОРЕНИЕ", "it": "VELOCIZZA", "ja": "高速化", "ko": "속도 향상", "ar": "التسريع"},
    "MAINTENANCE": {"zh": "维护", "es": "MANTENIMIENTO", "pt": "MANUTENÇÃO", "de": "WARTUNG", "fr": "MAINTENANCE", "ru": "ОБСЛУЖИВАНИЕ", "it": "MANUTENZIONE", "ja": "メンテナンス", "ko": "유지 관리", "ar": "الصيانة"},
    "SECURITY": {"zh": "安全", "es": "SEGURIDAD", "pt": "SEGURANÇA", "de": "SICHERHEIT", "fr": "SÉCURITÉ", "ru": "БЕЗОПАСНОСТЬ", "it": "SICUREZZA", "ja": "セキュリティ", "ko": "보안", "ar": "الأمان"},
    "PROGRAM": {"zh": "程序", "es": "PROGRAMA", "pt": "PROGRAMA", "de": "PROGRAMM", "fr": "PROGRAMME", "ru": "ПРОГРАММА", "it": "PROGRAMMA", "ja": "プログラム", "ko": "프로그램", "ar": "البرنامج"},
}

for code in ["zh", "es", "pt", "de", "fr", "ru", "it", "ja", "ko", "ar"]:
    p = DILLER / f"{code}.json"
    d = json.loads(p.read_text(encoding="utf-8"))
    for en, tr in T.items():
        d[en] = tr[code]
    p.write_text(json.dumps(d, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(code, len(d))
