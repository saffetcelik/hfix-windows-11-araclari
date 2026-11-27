# Translator brief (HFix Windows 11 Tools)

You are translating the user interface of **HFix Windows 11 Tools**, a free, open-source Windows 11 maintenance and optimization utility (process and network monitor, RAM cleaner, disk cleanup, startup/scheduled-task/service manager, privacy and performance tweaks, power modes, risk scan for suspicious programs, Windows repair and DNS tools). The audience is ordinary home users.

## Input

`diller/en.json` is a JSON array of 947 English UI strings. The position in the array (0-based) is the string's id.

## Output

Write your translation as one or more JSON objects mapping the id (as a string) to the translated text, in files named `diller/_<code>_<n>.json` (for example `diller/_de_1.json` for ids 0-199, `diller/_de_2.json` for 200-399, and so on). Use chunks of about 200 ids per file so that no single file is too large. Every id from 0 to 946 must appear exactly once across your files. Write valid JSON (UTF-8, escape `"` and `\` properly, use `\n` for line breaks that appear in the source).

Then run `python -I tools/metinler.py birlestir <code>` from `D:\FanBekcisi\kaynak`. It merges your parts into `diller/<code>.json` and reports missing ids and placeholder errors. Fix every reported problem by editing your part files and run it again until it reports no missing and no wrong-placeholder entries. Do not edit `en.json`, other languages' files, or any source code. Do not run cargo.

## Rules

1. **Placeholders must stay identical and in the same order**: `{}`, `{:.0}`, `{:.1}` etc. are replaced by values at runtime in order. You may move a placeholder within the sentence only if it keeps the same order relative to the other placeholders. `{{` and `}}` are literal braces.
2. Keep leading and trailing spaces, leading "· " separators, trailing ":" and "…", and the symbols exactly: ■ ⬇ ⬆ ✔ ✖ − + · … » and the like. Some strings are fragments that are concatenated with other text or values (for example "· {} enabled items unsigned / suspicious", "Plan: {}", "{} running").
3. Do not translate: HFix, product and company names (Windows, Microsoft, Defender, Copilot, Recall, Edge, Chrome, Firefox, Office, OneDrive, Xbox, NVIDIA, VMware, Hyper-V, Bing, GitHub, Cloudflare, Google, Quad9, AdGuard, OpenDNS, Bluetooth, Wi-Fi, DNS, IP, TCP, UDP, CPU, GPU, RAM, SSD, BIOS, UAC, SFC, DISM, EPP, PID, MB/GB/KB), file and folder names, paths (`C:\Windows\Temp`, `%TEMP%`), registry paths, command names (`sfc /scannow`, `chkdsk`), service and task names.
4. Where Windows has an official localized name for a feature or place (Task Manager, File Explorer, Task Scheduler, Settings, Start menu, taskbar, lock screen, Control Panel, Recycle Bin, Storage Sense, Windows Security, System Restore, restore point, power plan, Game Mode, Snap layouts, AutoPlay, Clipboard history, Delivery Optimization, Widgets, Sticky Keys, ...), use Microsoft's official term in your language.
5. UI style: short, natural and clear, the way a native-language Windows utility would phrase it. Button labels are short imperatives. Do not add explanations that are not in the English. Keep the same tone (no exclamation marks unless the source has one).
6. Consistent terminology across all strings: process, service, scheduled task, startup, unsigned, suspicious, quarantine, throttle (lower a process's priority/efficiency mode), boost, tweak/setting, apply, undo, power mode names (Quiet, Smart Balance, Cool Performance, Full Power), "Limited mode" (running without administrator rights).
7. Plural forms: English sometimes uses "process(es)" or "N items"; use the natural form of your language that works for any number.
