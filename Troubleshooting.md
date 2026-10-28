# استكشاف الأخطاء

## لا يتغير الصوت عند تبديل المحرك

تحقق من اللوق: `%APPDATA%\A2DPWB\logs\a2dpwb-core.<date>.log`

```
spatial engine: warming ... in the background
spatial engine: the warmed stage takes over
spatial engine: READY mode=... backend=native-reconstructed   ← Sony
spatial engine: EXPERIMENTAL_ID config={...}                    ← Upmixer
```

| ما تراه | المعنى |
|---|---|
| لا سطر `READY` | المحرك لم يبدأ: راجع مسار المرحلة و`restore.ps1` |
| `READY` لكن الصوت لا يتغير | قد يكون المحرك في وضع bypass: راجع `correction` في `config` |
| `config={"correction":"sony-xm6-ba"}` بينما تريد AutoEQ | نسخة الإعدادات لم تُستخدم — حدّث التطبيق (أُصلح في `9f310c7`) |

## نافذة CMD سوداء تظهر

المحرك يُشغَّل بـ`CREATE_NO_WINDOW`. إذا ظهرت نافذة فهي غالباً من نسخة **قديمة** شغّالة: التطبيق single-instance، فلا يحمّل ملفات جديدة إن كان يعمل. أغلق `a2dpwb.exe` و`a2dpwb-core.exe` من Task Manager ثم افتح التطبيق.

## الصوت يطلع من اللابتوب والسماعة معاً

التطبيق يكتم مخرج الصوت الافتراضي أثناء البث. تحقق:

```powershell
# مكتوم؟
powershell -File tools\mute-check.ps1   # أو من إعدادات الصوت في ويندوز
```

إن كان غير مكتوم: تأكد أنك على البناء الذي فيه `speakers.rs`، وأن الحالة `Streaming` (لا `Reconnecting`).

> ملاحظة: لو أُغلق التطبيق بالقوة أثناء البث قد يبقى ويندوز مكتوماً — افتح التطبيق وأوقف البث، أو اكتم من إعدادات الصوت.

## تقطيع أو `Poor` عند تغيير الشريط

- الشريط يُكتب عند رفع الإصبع، لا أثناء السحب.
- محرك Upmixer ياخذ ~0.9 ثانية ليجهز؛ أثناءها يعمل المحرك القديم. إن سمعت فراغاً فأنت على بناء قديم (قبل `77fc9e6`).

## Upmixer لا يعمل بعد التبديل لسوني والعودة

كان البق أن `config_for` تُسأل عن `python.exe` لا عن مجلد المرحلة، فيضيع التصحيح. أُصلح في `9f310c7`. تحقق من اللوق أن `config` يحمل `"correction":"autoeq-wh1000xm6"`.

## التصحيح AutoEQ ما يبان أثره

AutoEQ يفترض: ANC شغّال، وEQ الداخلي للسماعة مطفي. وهو ليس معايرة شخصية للأذن.

## المراحل غير موجودة

```powershell
powershell -ExecutionPolicy Bypass -File tools/xm6/restore.ps1
```

ثم في التطبيق: **Codecs → Engine folder → Use default folder**.

## فشل البناء

- استخدم `cargo +stable-x86_64-pc-windows-gnu`.
- `aptx-adaptive` يحتاج rustc 1.99؛ MSVC الأقدم يفشل.
- مسافة في مسار المشروع تكسر `windres` — ابنِ من مسار بلا مسافات.

## أين الحالة

| الملف | ماذا يقول |
|---|---|
| `tools/xm6/STATE.json` | من أي وسم upstream بُني هذا |
| `%APPDATA%\A2DPWB\profiles.json` | البروفايلات وإعدادات التأثير |
| `%APPDATA%\A2DPWB\spatial\*.json` | نسخة إعدادات المرحلة التي يقرأها المحرك |
| `%APPDATA%\A2DPWB\logs\a2dpwb-ui.*.log` | بدء الواجهة والأخطاء |
