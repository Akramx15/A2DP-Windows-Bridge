# التحديث من upstream

عندما ينشر `SeiyaFunaokaJP/A2DP-Windows-Bridge` إصداراً جديداً (وسم `v2.0.2-beta.N`)، لا تُعدّل الميزة يدوياً. أمر واحد:

```powershell
python tools/xm6/sync_upstream.py
```

بدون `--tag` يأخذ أحدث وسم. لتحديد وسم:

```powershell
python tools/xm6/sync_upstream.py --tag v2.0.2-beta.11
```

## ماذا يفعل

1. يتأكد أن الشجرة نظيفة وأنك على `feat/xm6`.
2. `git fetch upstream --tags`.
3. ينشئ `upstream-sync/<tag>` من `feat/xm6`.
4. `git merge --no-ff --no-commit <tag>`.
5. يحلّ **التعارضات الميكانيكية فقط**:
   - `app/lang/*.json` — دمج ثلاثي لكل مفتاح: نص upstream الجديد يفوز إذا لم نغيّره، ومفاتيحنا تبقى.
   - `app/src/ui/src/ipc/bindings/*.ts` — مولّدة بـts-rs: تُعاد توليدها بدل دمجها كنص.
6. يترك أي تعارض حقيقي ويوقف، ويطبع الملفات.
7. يشغّل البوابات: `cargo check` + `tsc` + `vitest` + `check_lang.py`.
8. يلتزم ويكتب `tools/xm6/STATE.json`، ويطبع وسم الإصدار التالي `v<tag>-xm6.<n>`.

خيارات: `--dry-run` (يعرض التعارضات بلا تغيير)، `--no-gates`، `--keep`، `--push`.

## إن توقف على تعارض حقيقي

| الملف | الحل المعتاد |
|---|---|
| `ipc/src/lib.rs` | خُذ الجانبين (exports الجديدة من الطرفين) |
| `ui/screens/Codecs.tsx` | أبقِ هيكل upstream وأعد تركيب `<SpatialStage>` |
| `ui/screens/Home.tsx` | ادمج الاستيرادات، وأبقِ شارة التأثير وسطر الاتصال |
| bindings | `cargo test -p ipc export_bindings` |

بعد الحل: `git add`، ثم أكمل الدمج يدوياً وحدّث `STATE.json`.

## الفروع

| الفرع | المعنى |
|---|---|
| `main` | مرآة `upstream/main` — ممنوع أي تعديل |
| `feat/xm6` | الميزة الدائمة — ممنوع rebase أو force-push |
| `upstream-sync/<tag>` | فرع دمج قصير العمر، يُحذف بعد الدمج |

الإصدارات وسوم على merge commit: `v2.0.2-beta.10-xm6.1`.

## CI

| Workflow | متى يعمل | ماذا يفعل |
|---|---|---|
| `xm6-ci.yml` | PR/push على `feat/xm6` | Rust (fmt/clippy/test) + UI (tsc/vitest) + **feature-intact** + **bindings-fresh** + مفاتيح اللغة |
| `upstream-watch.yml` | كل 6 ساعات | يفتح Issue عند وسم جديد مع أمر التحديث |
| `xm6-release.yml` | دفع وسم `v*-xm6.*` | يبني zip + SHA256SUMS وينشر prerelease |

`feat/xm6` محمي: ممنوع force-push، والدمج يحتاج الفحوص الخمسة.
