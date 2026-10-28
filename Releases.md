# الإصدارات

## الوسوم

`v<upstream>-xm6.<n>` — مثال: `v2.0.2-beta.9-xm6.1`

- الوسم على **merge commit** الذي دخلت فيه الميزة فوق وسم upstream.
- `Cargo.toml` يبقى على نسخة upstream عمداً (حتى لا يتعارض كل إصدار).
- من أي وسم بُني التطبيق مكتوب في `tools/xm6/STATE.json`.

## كيف يُصنع إصدار

```powershell
# بعد دمج وسم upstream على feat/xm6
git tag -a v2.0.2-beta.10-xm6.1 -m "xm6 feature on v2.0.2-beta.10"
git push origin v2.0.2-beta.10-xm6.1
```

دفع الوسم يشغّل `xm6-release.yml`: يبني، يضغط `a2dpwb-<tag>-win-x64.zip`، ويكتب `SHA256SUMS.txt`، وينشر prerelease.

## ما داخل الأرشيف

- `a2dpwb.exe`، `a2dpwb-core.exe`، `a2dpwb-driver.exe`
- `README.md`، `AGENTS.md` إن وُجد، وملفات الترخيص

**لا** يحتوي: المراحل (`%APPDATA%\A2DPWB\stages`) ولا runtime الـUpmixer. هذان يُنزّلان بـ`tools/xm6/restore.ps1`.

## الفروع

| الفرع | الحالة |
|---|---|
| `main` | مرآة upstream — لا تعديل |
| `feat/xm6` | الميزة، محمي، هو المنتج |
| `refactor/*`, `chore/*`, `ui/*` | قصيرة العمر، تُدمج في `feat/xm6` |
| `upstream-sync/*` | دمج وسم جديد، يُحذف بعد الدمج |

## سجل مختصر لهذا الخط

| التاريخ | الحدث |
|---|---|
| 2026-10-17 → 22 | تفكيك الملف الشخصي، بناء المحرك، منفذ ويندوز بايت-ببايت |
| 2026-10-27 | دمج `v2.0.2-beta.9` + بناء وتثبيت |
| 2026-10-28 | `v2.0.2-beta.9-xm6.1`: واجهة التأثير، أدوات المزامنة، CI، كتم سماعات الـPC، تبديل المحرك بدون قطع، إصلاح config التصحيح |
