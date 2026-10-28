# التركيب

## 1. التطبيق

نزّل آخر إصدار من صفحة [Releases](https://github.com/Akramx15/A2DP-Windows-Bridge/releases) (ملف `a2dpwb-<tag>-win-x64.zip`)، أو ابنِه من المصدر:

```powershell
git clone https://github.com/Akramx15/A2DP-Windows-Bridge.git
cd A2DP-Windows-Bridge
git checkout feat/xm6
cd app/src/ui; pnpm install; pnpm build; cd ../../..
cargo +stable-x86_64-pc-windows-gnu build --release --workspace
```

ثم انسخ `a2dpwb.exe` و`a2dpwb-core.exe` إلى مجلد التثبيت:

```
%LOCALAPPDATA%\A2DP Windows Bridge\
```

> البناء على هذا الجهاز يحتاج toolchain **GNU**: `rustup toolchain install stable-x86_64-pc-windows-gnu`. السبب: `aptx-adaptive` يحتاج rustc أحدث من MSVC المثبّت.

## 2. المراحل (Stages)

المراحل ليست داخل git — تُنزّل من مستودع المحرك:

```powershell
powershell -ExecutionPolicy Bypass -File tools/xm6/restore.ps1
```

هذا ينسخ المراحل إلى:

```
%APPDATA%\A2DPWB\stages\
  personal-hpp-ba      ← Sony XM6 BA
  personal-hpp-no-ba   ← Sony HPP بدون تصحيح
  personal-hps         ← Sony HPS
  sony-native-reference← Sony — المرجع القياسي
  experimental-stft    ← Upmixer (يحتاج runtime بايثون)
```

`restore.ps1` يبني أيضاً `%APPDATA%\A2DPWB\runtime\upmixer-venv` (بايثون + numpy/scipy) ويتحقق من كل مرحلة بالبايت.

## 3. أول تشغيل

1. افتح التطبيق، أضف السماعة من **Audio Devices** واتصل.
2. اذهب لـ**Home** — بطاقة الجهاز فيها: الاتصال، الكوديك، أزرار التأثير، المرحلة، التصحيح، الشريطين.
3. إن لم تظهر المراحل: **Codecs → Engine folder → Use default folder**.

## الملفات التي تُنشأ

| المسار | المحتوى |
|---|---|
| `%APPDATA%\A2DPWB\profiles.json` | البروفايلات (منها إعدادات التأثير) |
| `%APPDATA%\A2DPWB\spatial\<profile>.json` | نسخة التطبيق من إعدادات المرحلة |
| `%APPDATA%\A2DPWB\logs\` | لوق الكور والواجهة |
| `tools/xm6/STATE.json` | من أي إصدار upstream بُني هذا |
