# gym-app

نرم‌افزار متن‌باز مدیریت باشگاه: محلی، آفلاین، رایگان، با حضور و غیاب خودکار از طریق تشخیص چهره با یک وبکم معمولی.

| پوشه | محتوا |
|---|---|
| [`docs/SPEC.md`](docs/SPEC.md) | مشخصات کامل: مدل داده، قواعد کسب‌وکار، صفحه‌ها، مراحل |
| [`face-service/`](face-service/) | سرویس تشخیص چهره (Rust + ONNX Runtime) |
| `assets/voices/` | فایل‌های صوتی اعلام ورود، خروج و پایان شهریه (مرد / زن) |
| [`server/`](server/) | سرور اصلی (Rust + SQLite)؛ UI و صداها داخل exe جاسازی می‌شن |
| `web/` | رابط کاربری (React + TypeScript + Vite) |
| `installer/` | نصب‌کننده‌ی ویندوز (Inno Setup) |
| `spikes/` | آزمایش‌های دورریختنی (پخش صدا) |

## ساخت نصب‌کننده (ویندوز)

پیش‌نیازها: Rust (MSVC)، Node.js، [Inno Setup 6](https://jrsoftware.org/isinfo.php) (`winget install JRSoftware.InnoSetup`)، و `face-service/onnxruntime.dll` (نسخه‌ی 1.22) و مدل‌ها در `face-service/models` (راهنما در `face-service/README.md`).

```powershell
powershell -ExecutionPolicy Bypass -File installer\build.ps1
```

خروجی: `installer\Output\GymApp-Setup-<نسخه>.exe`

نصب‌کننده:
- برنامه‌ها رو در `C:\Program Files\GymApp` و داده‌ها رو در `C:\ProgramData\GymApp` می‌ذاره (به‌روزرسانی و حذف، داده رو دست نمی‌زنه؛ موقع حذف می‌پرسه)
- دو سرویس ویندوز `FaceService` و `GymServer` رو نصب و اجرا می‌کنه (با ویندوز بالا میان، بعد از crash دوباره اجرا می‌شن)
- میانبر دسکتاپ می‌سازه که برنامه رو در پنجره‌ی مستقل Edge باز می‌کنه
- لاگ‌ها: `C:\ProgramData\GymApp\*\data\logs`

**پشتیبان‌گیری:** کل پوشه‌ی `C:\ProgramData\GymApp` (اول هر دو سرویس رو از services.msc متوقف کنید).

## انتشار نسخه

```bash
git tag v1.0.0 && git push origin v1.0.0
```

workflow‏ `release` روی ویندوزِ GitHub همه چیز را می‌سازد (ONNX Runtime و مدل‌ها را خودش دانلود می‌کند) و `GymApp-Setup-1.0.0.exe` را همراه SHA256 در Releases می‌گذارد.

## سایت و دمو

`site/` صفحه‌ی معرفی است و `web` در حالت دمو (`npm run build:demo`) کل برنامه را بدون سرور و با داده‌ی ساختگی داخل مرورگر اجرا می‌کند. workflow‏ `pages` با هر push روی main هر دو را روی GitHub Pages منتشر می‌کند (یک بار: Settings ← Pages ← Source: GitHub Actions).
