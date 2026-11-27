"""Kaynak koddaki arayuz metinlerinin Ingilizce hallerini toplar ve ceviri dosyalarini denetler.

    python tools/metinler.py cikar      -> diller/en.json (cevrilecek Ingilizce metinlerin listesi)
    python tools/metinler.py denetle    -> diller/*.json dosyalarini en.json'a gore denetler

Ingilizce metin; lang::t("tr", "en") ve tr!("tr", "en", ...) cagrilarinin ikinci argumani,
tweak.rs'teki t(id, grup, oneri, ad, ad_en, aciklama, aciklama_en, ...) cagrilarinin 5. ve 7.
argumani ve iki dilli tablolardaki Ingilizce sutunlardir. Ceviri dosyasi {"Ingilizce": "ceviri"}
biciminde bir JSON nesnesidir; ceviride yer tutucularin ({}, {:.1} ...) sirasi ve bicimi
Ingilizce ile ayni olmalidir.
"""

import json
import re
import sys
from pathlib import Path

KOK = Path(__file__).resolve().parent.parent
SRC = KOK / "src"
DILLER = KOK / "diller"


def tokenize(text):
    """Rust kaynagini kaba belirteclere ayirir: ('str', deger) / ('id', ad) / ('p', isaret)."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
            continue
        if text.startswith("/*", i):
            j = text.find("*/", i + 2)
            i = n if j < 0 else j + 2
            continue
        m = re.match(r'(b?)r(#*)"', text[i:])
        if m and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = m.group(2)
            start = i + m.end()
            end = text.find('"' + hashes, start)
            out.append(("raw", text[start:end]))
            i = end + 1 + len(hashes)
            continue
        if c == '"' or (c == "b" and text.startswith('b"', i)):
            if c == "b":
                i += 1
            j, buf = i + 1, []
            while text[j] != '"':
                if text[j] == "\\":
                    e = text[j + 1]
                    if e == "\n":  # satir devami: sonraki bosluklari atla
                        j += 2
                        while text[j] in " \t\r\n":
                            j += 1
                        continue
                    if e == "\r" and text[j + 2] == "\n":
                        j += 3
                        while text[j] in " \t\r\n":
                            j += 1
                        continue
                    if e == "u":
                        k = text.index("}", j)
                        buf.append(chr(int(text[j + 3 : k], 16)))
                        j = k + 1
                        continue
                    buf.append({"n": "\n", "t": "\t", "r": "\r", "0": "\0", "\\": "\\", '"': '"', "'": "'"}.get(e, e))
                    j += 2
                    continue
                buf.append(text[j])
                j += 1
            out.append(("str", "".join(buf)))
            i = j + 1
            continue
        if c == "'":
            # karakter sabiti ('a', '\n', '\'') ya da omur ('static)
            m = re.match(r"'(\\.|\\u\{[0-9a-fA-F]+\}|[^\\'])'", text[i:])
            if m:
                out.append(("chr", m.group(1)))
                i += m.end()
                continue
            m = re.match(r"'[A-Za-z_][A-Za-z0-9_]*", text[i:])
            out.append(("life", m.group(0)))
            i += m.end()
            continue
        if c.isalpha() or c == "_":
            m = re.match(r"[A-Za-z_][A-Za-z0-9_]*", text[i:])
            out.append(("id", m.group(0)))
            i += m.end()
            continue
        if c.isspace():
            i += 1
            continue
        out.append(("p", c))
        i += 1
    return out


def call_args(toks, k):
    """toks[k] '(' ise kapanan paranteze kadar ust duzey argumanlari belirtec listeleri olarak doner."""
    depth, args, cur = 0, [], []
    for j in range(k, len(toks)):
        kind, v = toks[j]
        if kind == "p" and v in "([{":
            depth += 1
            if depth == 1:
                continue
        elif kind == "p" and v in ")]}":
            depth -= 1
            if depth == 0:
                if cur:
                    args.append(cur)
                return args
        if depth == 1 and kind == "p" and v == ",":
            args.append(cur)
            cur = []
            continue
        cur.append((kind, v))
    return args


def lit(arg):
    if len(arg) == 1 and arg[0][0] in ("str", "raw"):
        return arg[0][1]
    return None


def extract():
    found = []
    for f in sorted(SRC.glob("*.rs")):
        toks = tokenize(f.read_text(encoding="utf-8"))
        for k, (kind, v) in enumerate(toks):
            # tr!( ... )
            if kind == "id" and v == "tr" and k + 2 < len(toks) and toks[k + 1] == ("p", "!") and toks[k + 2] == ("p", "("):
                a = call_args(toks, k + 2)
                if len(a) >= 2 and lit(a[0]) is not None and lit(a[1]) is not None:
                    found.append(lit(a[1]))
                continue
            if kind == "id" and v == "t" and k + 1 < len(toks) and toks[k + 1] == ("p", "("):
                if k > 0 and toks[k - 1] == ("id", "fn"):
                    continue
                a = call_args(toks, k + 1)
                if len(a) == 2 and lit(a[0]) is not None and lit(a[1]) is not None:
                    found.append(lit(a[1]))
                elif f.name == "tweak.rs" and len(a) == 8 and lit(a[4]) is not None and lit(a[6]) is not None:
                    found += [lit(a[4]), lit(a[6])]
                continue
            # iki dilli tablolar: demetler
            if kind == "p" and v == "(" and (k == 0 or toks[k - 1][0] != "id"):
                a = call_args(toks, k)
                lits = [lit(x) for x in a]
                if f.name == "svc.rs" and len(a) == 4 and a[1] and a[1][0] == ("id", "Group") and lits[2] and lits[3] is not None:
                    found.append(lits[3])
                elif f.name == "tasks.rs" and len(a) == 3 and all(x is not None for x in lits):
                    found.append(lits[2])
                elif f.name == "tweak.rs" and len(a) == 4 and all(x is not None for x in lits[:3]) and a[3] in ([("id", "true")], [("id", "false")]):
                    found.append(lits[2])
                elif f.name == "tweak.rs" and len(a) == 2 and all(x is not None for x in lits):
                    found += lits  # GROUPS / GROUPS_EN: Turkce olanlar da girer, zararsiz
                elif f.name == "main.rs" and len(a) == 3 and lits[0] is not None and lits[1] is not None and a[2] and a[2][0] == ("p", "&"):
                    found.append(lits[1])  # Tab::NAV bolum basliklari
                elif f.name == "ui6.rs" and len(a) == 5 and all(x is not None for x in lits[:4]):
                    found += [lits[1], lits[3]]
    seen, out = set(), []
    for s in found:
        # Turkce harf iceren (iki dilli tablolarin Turkce sutunu) metinler cevrilmez.
        if s and s.strip() and s not in seen and re.search(r"[A-Za-z]", s) and not re.search(r"[çğışöüÇĞİŞÖÜ]", s):
            seen.add(s)
            out.append(s)
    return out


PH = re.compile(r"\{\{|\}\}|\{[^{}]*\}")


def placeholders(s):
    return [m for m in PH.findall(s) if m not in ("{{", "}}")]


def check():
    en = json.loads((DILLER / "en.json").read_text(encoding="utf-8"))
    ok_all = True
    for f in sorted(DILLER.glob("*.json")):
        if f.name == "en.json" or f.name.startswith("_"):
            continue
        try:
            d = json.loads(f.read_text(encoding="utf-8"))
        except Exception as e:  # noqa: BLE001
            print(f"{f.name}: JSON okunamadi: {e}")
            ok_all = False
            continue
        missing = [s for s in en if s not in d]
        extra = [s for s in d if s not in set(en)]
        bad = [s for s in en if s in d and (not isinstance(d[s], str) or not d[s].strip() or placeholders(d[s]) != placeholders(s))]
        print(f"{f.name}: {len(en) - len(missing)}/{len(en)} cevrildi, eksik {len(missing)}, fazla {len(extra)}, yer tutucusu hatali {len(bad)}")
        for s in bad[:5]:
            print(f"   hatali: {s!r} -> {d[s]!r}")
        for s in missing[:5]:
            print(f"   eksik: {s!r}")
        if bad or missing:
            ok_all = False
    return ok_all


def merge(code):
    """diller/_<kod>_*.json parcalarini ({"sira": "ceviri"}) birlestirip diller/<kod>.json yazar."""
    en = json.loads((DILLER / "en.json").read_text(encoding="utf-8"))
    parts = sorted(DILLER.glob(f"_{code}_*.json"))
    by_index = {}
    for p in parts:
        by_index.update(json.loads(p.read_text(encoding="utf-8")))
    out, bad, missing = {}, [], []
    for i, s in enumerate(en):
        v = by_index.get(str(i))
        if not isinstance(v, str) or not v.strip():
            missing.append(i)
        elif placeholders(v) != placeholders(s):
            bad.append((i, s, v))
        else:
            out[s] = v
    (DILLER / f"{code}.json").write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"{code}: {len(parts)} parca, {len(out)}/{len(en)} gecerli, eksik {len(missing)}, yer tutucusu hatali {len(bad)}")
    for i in missing[:20]:
        print(f"   eksik {i}: {en[i]!r}")
    for i, s, v in bad[:20]:
        print(f"   hatali {i}: {s!r} -> {v!r}")
    return not missing and not bad


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "birlestir":
        sys.exit(0 if merge(sys.argv[2]) else 1)
    if sys.argv[1:] == ["cikar"]:
        DILLER.mkdir(exist_ok=True)
        strings = extract()
        (DILLER / "en.json").write_text(json.dumps(strings, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
        words = sum(len(s.split()) for s in strings)
        print(f"{len(strings)} metin, {words} sozcuk -> {DILLER / 'en.json'}")
    elif sys.argv[1:] == ["denetle"]:
        sys.exit(0 if check() else 1)
    else:
        print(__doc__)
