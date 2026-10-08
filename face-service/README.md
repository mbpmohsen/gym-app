# face-service

سرویس محلی تشخیص چهره برای نرم‌افزار مدیریت باشگاه. Rust + ONNX Runtime، مدل‌های YuNet و SFace.

## ساختار

```
src/
  lib.rs          init_runtime (لود DLL با مسیر مطلق)
  detector.rs     YuNet: letterbox → inference → decode → NMS
  align.rs        5-point similarity → چهره‌ی 112×112
  recognizer.rs   SFace: embedding نرمال‌شده‌ی ۱۲۸ بعدی + cosine
  camera.rs       منبع فریم (دوربین یا پوشه‌ی replay) روی thread جدا، فقط آخرین فریم
  quality.rs      رد چهره‌ی کوچیک، چرخیده، کج یا بریده
  gallery.rs      گالری اعضا (فعلاً پوشه: gallery/<member_id>/*.emb)
  engine.rs       tracker + رأی‌گیری + cooldown → رویدادها
  enrollment.rs   جلسه‌ی ثبت‌نام (مشترک بین CLI و API)
  config.rs       face-service.toml
  store.rs        SQLite: اعضا، embeddingها، لاگ رویدادها
  worker.rs       thread اصلی: دوربین + مدل‌ها + سقف FPS + preview + snapshot
  api.rs          HTTP API (axum)
  main.rs         باینری سرویس: run / install / uninstall / start / stop / status
  server.rs       راه‌اندازی کامل سرویس (مشترک بین ترمینال و Windows Service)
  service.rs      Windows Service (فقط ویندوز)
  logging.rs      لاگ فایل روزانه در data/logs (۱۴ روز) + کنسول
  web/index.html  صفحه‌ی تست داخلی
  bin/
    cam_probe.rs  M1: تست دوربین
    ort_probe.rs  M1: تست لود runtime و مدل‌ها
    face_dump.rs  M2: خروجی JSON کل pipeline (برای golden test)
    compare.rs    M2: «این دو عکس یک نفرن؟»
    enroll.rs     M3: ثبت‌نام عضو از دوربین
    live.rs       M3: حلقه‌ی تشخیص زنده
tools/
  golden.py, check_golden.py, golden_test.sh   مقایسه با OpenCV
testdata/         عکس‌های PNG برای golden test (حداکثر 640 پیکسل)
```

## پیش‌نیازها

- **Windows: toolchain MSVC** (`rustup default stable-x86_64-pc-windows-msvc`) + Build Tools for Visual Studio. GNU کار نمی‌کنه.
- **ONNX Runtime دقیقاً 1.22.x** (هم‌خوان با `ort = 2.0.0-rc.10`). نسخه‌ی DLL و crate همیشه با هم ارتقا پیدا می‌کنن.
  - Windows: `onnxruntime-win-x64-1.22.0.zip` → `lib/onnxruntime.dll`
  - Linux: `onnxruntime-linux-x64-1.22.0.tgz` → `lib/libonnxruntime.so`
- **مدل‌ها** در `models/` (از `media.githubusercontent.com/media/opencv/opencv_zoo/main/models/...`):
  - `face_detection_yunet_2023mar.onnx` (~230KB, MIT)
  - `face_recognition_sface_2021dec.onnx` (~37MB, Apache 2.0)

## اجرا

```sh
DLL="$(pwd -W)/onnxruntime.dll"     # Git Bash؛ همیشه مسیر مطلق

cargo run --release --bin cam_probe -- 1 200
cargo run --release --bin compare  -- "$DLL" ./models a.jpg b.jpg [threshold]
tools/golden_test.sh "$DLL"         # نیاز به: pip install opencv-python numpy

cargo run --release --bin enroll -- "$DLL" ./models ./gallery mohsen auto
cargo run --release --bin live   -- "$DLL" ./models ./gallery auto [--verbose] [--threshold 0.4] [--cooldown 60]
# source: auto | ایندکس دوربین | پوشه‌ی فریم برای replay (با --fps)
cargo test --release --lib          # تست‌های منطق رأی‌گیری
```

## سرویس

```sh
cargo run --release                      # اجرا در ترمینال (Ctrl+C برای توقف)
```

کانفیگ: `--config <path>`، وگرنه `./face-service.toml` اگه وجود داشته باشه، وگرنه کنار exe.

### Windows Service (ترمینال Administrator)
```sh
target/release/face-service.exe install --config "$(pwd -W)/face-service.toml"
target/release/face-service.exe start
target/release/face-service.exe status
target/release/face-service.exe stop
target/release/face-service.exe uninstall
```
- اجرا با LocalSystem، شروع خودکار با ویندوز، ری‌استارت ۵ ثانیه بعد از کرش (۳ بار، شمارنده روزانه صفر می‌شه)
- لاگ‌ها: `data/logs/face-service.YYYY-MM-DD.log` (به وقت محلی)
- اگه سرویس قبل از راه افتادن لاگ fail بشه: `face-service-error.txt` کنار exe
- installer جدا نداره: installer نرم‌افزار مدیریت باشگاه فایل‌ها رو کپی می‌کنه و `face-service.exe install` رو صدا می‌زنه
بار اول `face-service.toml` با توکن تصادفی ساخته می‌شه. لینک صفحه‌ی تست (با توکن) در خروجی چاپ می‌شه:
`listening on http://127.0.0.1:7480/?token=...`

| Method | Path | توضیح |
|---|---|---|
| GET | `/v1/health` | وضعیت دوربین، FPS، تعداد اعضا، enrollment جاری |
| GET | `/v1/gallery` | لیست اعضای ثبت‌شده (برای reconcile) |
| DELETE | `/v1/gallery/{member_id}` | حذف عضو |
| POST | `/v1/enroll/{member_id}/start` | شروع ثبت‌نام، body اختیاری `{"samples": 8}` |
| GET | `/v1/enroll` | وضعیت: `collecting` / `ready` / `failed` / `idle` + راهنما |
| POST | `/v1/enroll/{member_id}/commit` | ذخیره‌ی ثبت‌نام آماده |
| DELETE | `/v1/enroll` | لغو |
| GET | `/v1/events` | SSE؛ ادامه با `Last-Event-ID` یا `?since=<id>` |
| GET | `/v1/preview` | MJPEG با کادر چهره‌ها |
| GET | `/v1/snapshots/{id}.jpg` | عکس لحظه‌ی رویداد |
| GET / PUT | `/v1/config` | `threshold`، `cooldown_secs`، `max_fps` (در فایل هم ذخیره می‌شه) |

احراز هویت: `Authorization: Bearer <token>` یا `?token=` (برای `<img>` و `EventSource`).

رویداد نمونه:
```json
{"id":42,"ts":1791467169243,"type":"recognized","member_id":"1042","score":0.91,"track_id":7,"snapshot":"/v1/snapshots/42.jpg"}
{"id":43,"ts":1791467182753,"type":"unknown","track_id":8,"snapshot":"/v1/snapshots/43.jpg"}
{"id":44,"ts":1791467190000,"type":"uncertain","track_id":9,"candidates":[{"member_id":"1042","score":0.41}],"snapshot":"/v1/snapshots/44.jpg"}
```

## تصمیم‌های ثابت‌شده

| موضوع | تصمیم | دلیل |
|---|---|---|
| Toolchain ویندوز | MSVC | GNU به `dlltool` نیاز داره؛ DLL مایکروسافت هم MSVC ـه |
| لود DLL | فقط مسیر مطلق، پیش‌فرض کنار exe | ویندوز ۱۱ نسخه‌ی 1.17 خودش رو در System32 داره |
| فرمت دوربین | 640×480 MJPEG + زنجیره‌ی fallback | روی Integrated Camera پایدار، ۱۷ FPS، صفر خطا |
| دوربین مجازی | پیش‌فرض رد می‌شه | EShare و مشابه‌ها ایندکس ۰ رو می‌گیرن |
| FPS | خودمون اندازه می‌گیریم | `nokhwa` روی MSMF عدد غلط گزارش می‌ده (`@1FPS`) |
| ورودی YuNet | ثابت 640×640، letterbox بالا-چپ، بدون بزرگ‌نمایی | مدل shape داینامیک نداره |
| ورودی SFace | **RGB** (نه BGR)، 0..255 | OpenCV با `swapRB=true` می‌سازه؛ golden test تأییدش کرد |
| Embedding | L2-normalize قبل از ذخیره/مقایسه | خروجی خام norm حدود ۴ تا ۶ داره |
| Threshold | 0.363 پیش‌فرض، قابل تنظیم | باید روی دوربین واقعی کالیبره بشه |
| سقف FPS پردازش | ۸ (قابل تنظیم) | بدون سقف ~۳۰ FPS و یک هسته‌ی کامل CPU مصرف می‌شد |
| Bind | فقط loopback؛ غیر از اون سرویس اجرا نمی‌شه | سرویس نباید از شبکه در دسترس باشه |
| Enrollment | نمونه‌ی جدید باید با میانگین نمونه‌های قبلی ≥ 0.5 شبیه باشه | تست نشون داد چهره‌ی آدم دیگه‌ای که وارد کادر بشه به‌عنوان نمونه‌ی عضو ذخیره می‌شد |
| Enrollment | commit صریح لازمه | برنامه‌ی اصلی (یا پذیرش) تأیید می‌کنه، بعد ذخیره می‌شه |
| Snapshot | فقط crop چهره (≤ 256px)؛ با پاک شدن رویداد از لاگ (۲۰۰۰ تای آخر) پاک می‌شه | حجم کم |
| دوربین قطع شد | worker هر ۳ ثانیه دوباره تلاش می‌کنه؛ health = `degraded` | سرویس نباید با جدا شدن USB بمیره |

## وضعیت

### Milestone 1 ✅ (ویندوز، Integrated Camera)
- دوربین: ۱۷.۱ FPS، ۰ خطا در ۲۰۰ فریم
- YuNet: ۵.۴ms — SFace: ۱۶ms

### Milestone 2 ✅ (golden test روی لینوکس)
- روی هر ۳ عکس تست: bbox و landmarkها Δ0.000px، score Δ0، cosine(Rust, OpenCV) = 1.00000
- تست حساسه: با برعکس کردن RGB/BGR، cosine به 0.970 می‌رسه و FAIL می‌شه
- compare: آدم‌های متفاوت 0.08 تا 0.13 (خیلی زیر 0.363)

### Milestone 3 ✅ (replay روی لینوکس؛ تست زنده روی ویندوز مونده)
- سناریوی replay ‏۶۶ فریمی: ورود عضو → RECOGNIZED (0.97)، غریبه → UNKNOWN، برگشت عضو → cooldown
- ۲۲ms برای هر فریم، بدون فریم drop‌شده در ۱۰ FPS
- ۴ unit test برای رأی‌گیری: recognized، unknown، uncertain، ارتقای uncertain به recognized، عدم اعلام تکراری
- قواعد: ۳ از ۵ رأی بالای threshold → recognized؛ پنجره‌ی پر و هیچ رأی بالای threshold → unknown؛ بقیه → uncertain با ۳ کاندید
- enroll: فقط وقتی یک چهره در تصویره، کیفیت قبوله، ≥300ms از نمونه‌ی قبلی گذشته و شباهت با نمونه‌های قبلی < 0.93 ـه

| پارامتر | پیش‌فرض |
|---|---|
| threshold | 0.363 |
| رأی | ۳ از ۵ |
| cooldown | ۶۰۰ ثانیه |
| track timeout | ۱.۵ ثانیه |
| حداقل فاصله‌ی چشم‌ها | ۲۸ پیکسل |
| چرخش سر (yaw) | بینی در بازه‌ی 0.2..0.8 بین دو چشم |
| کج بودن (roll) | ≤ ۲۵ درجه |

### Milestone 4 ✅ (تست end-to-end روی لینوکس با دوربین replay)
- auth (۴۰۱ بدون توکن یا با توکن غلط)، اعتبارسنجی member_id، ۴۰۹ برای enroll همزمان یا commit بدون session
- enrollment از API: ready → commit → اعضا؛ مسیر timeout → failed بدون ذخیره
- SSE: recognized / unknown / uncertain؛ resume با `Last-Event-ID` و `?since=` از دیتابیس
- snapshot و MJPEG preview؛ رد مسیرهای مشکوک snapshot
- PUT config: رد فیلد read-only و مقدار خارج از بازه؛ تغییر بلافاصله اعمال و در فایل ذخیره شد
- restart: گالری از SQLite برگشت؛ delete → 204 بعد 404
- Ctrl+C: خاموش شدن تمیز

### Milestone 4 روی ویندوز ✅
- enroll از صفحه‌ی مرورگر → `RECOGNIZED 1254 (0.937)` بلافاصله، بدون ری‌استارت

### Milestone 5 (در حال انجام)
- ✅ فرمان‌های install / uninstall / start / stop / status، لاگ فایل
- ⏳ **تست مهم:** دسترسی سرویس (Session 0، LocalSystem) به دوربین. اگه نشد، plan B: اجرا با Task Scheduler موقع login کاربر
