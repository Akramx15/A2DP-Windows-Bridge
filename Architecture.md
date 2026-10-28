# البنية

## مسار الصوت

```
التقاط (48 kHz أو 96 kHz)
   → [إعادة عيّنة إلى 48 kHz]   فقط إذا كان الالتقاط 96 kHz
   → محرك مكاني (عملية فرعية، بلوكات 1024، 48 kHz)
   → [إعادة عيّنة للخلف]
   → المشفّر → LDAC / aptX / AAC …
```

- المحرك **عملية فرعية** لا مكتبة: لا يُسقط التطبيق، ويُستبدل بلا إعادة بناء.
- فشل المحرك = مرور الصوت كما هو + سطر في اللوق؛ لا يموت البث.
- المحرك ثابت على 48 kHz / 1024. مسار 96 kHz موجود في `resample.rs` لكنه غير مفعّل افتراضياً.

## أين الكود

| المسار | المسؤولية |
|---|---|
| `app/src/crates/core/src/spatial.rs` | اكتشاف المراحل، المانيفست، التحكم، أمر التشغيل |
| `app/src/crates/pipeline/src/spatial.rs` | العملية الفرعية: PCM داخل/خارج، `CREATE_NO_WINDOW` |
| `app/src/crates/pipeline/src/resample.rs` | إعادة العيّنة حول المحرك |
| `app/src/crates/pipeline/src/lib.rs` | حقن واحد في `encode_thread` + قناة `PENDING_STAGE` |
| `app/src/crates/core/src/speakers.rs` | كتم سماعات الـPC أثناء البث |
| `app/src/ui/src/components/DeviceEffects.tsx` | أزرار التأثير والمرحلة والتصحيح والشريطين |
| `app/src/ui/src/components/stream.tsx` | سطر الاتصال الموحّد (`ConnectionLine`) والشارات |
| `app/src/crates/core/src/service.rs` | `notify()`: كتم/رفع السماعات حسب حالة البث |

## تبديل المرحلة بدون قطع

1. طلب التبديل يصل إلى `PENDING_STAGE` (خانة واحدة، آخر طلب يفوز).
2. خيط البث يبني المحرك الجديد في **خيط منفصل** ويسخّنه ببلوك صامت.
3. المحرك القديم يظل يعالج الصوت إلى أن يردّ الجديد.
4. عندها يتم التبديل عند حدّ بلوك، مع تفريغ المخازن المؤقتة.

لهذا لا يوجد فراغ عند التبديل: Sony ~50 مللي، Upmixer ~0.9 ثانية.

## إعدادات المرحلة

- كل مرحلة تعلن تحكمها في `STAGE-MANIFEST.json` (`controls.sliders` و`controls.selects`).
- قيم المستخدم تُحفظ في البروفايل (`spatial_controls`) وتُكتب نسخة في
  `%APPDATA%\A2DPWB\spatial\<profile>.json` — وهي التي يقرأها المحرك عبر `--config`.
- `config_for` تُسأل عن **مسار المرحلة** لا عن برنامج الأمر (مرحلة بايثون تشغّل `python.exe` ومجلده بلا مانيفست — وهذا كان سبب ضياع AutoEQ).

## المراحل من المانيفست

```json
{
  "label": "Personalized HPP + XM6 BA",
  "family": "sony",
  "command": ["./sony_native_stream.exe", "-", "stream", "1", "1024"]
}
```

إضافة مرحلة = مجلد فيه برنامج ومانيفست. لا تعديل كود.
