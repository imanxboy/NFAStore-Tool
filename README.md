<div align="center">

<img src="src-tauri/icons/128x128.png" width="96" alt="NFAStore Tool" />

# NFAStore Tool

**ورود سریع به اکانت‌های استیم با توکن — بدون رمز عبور، بدون مرورگر**

[![Version](https://img.shields.io/github/v/release/imanxboy/NFAStore-Tool?style=flat-square&color=F26F21&label=version)](https://github.com/imanxboy/NFAStore-Tool/releases/latest)
[![Platform](https://img.shields.io/badge/Windows-x64-0078D6?style=flat-square&logo=windows11&logoColor=white)](https://github.com/imanxboy/NFAStore-Tool/releases/latest)
[![Tauri](https://img.shields.io/badge/Tauri-2-FFC131?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)
[![Source](https://img.shields.io/badge/source-open-2ea44f?style=flat-square&logo=github&logoColor=white)](https://github.com/imanxboy/NFAStore-Tool)

[دانلود آخرین نسخه](https://github.com/imanxboy/NFAStore-Tool/releases/latest) · [فروشگاه NFAStore](https://nfastore.org) · [English](#english)

</div>

---

## این برنامه چیست

وقتی از [NFAStore](https://nfastore.org) اکانت می‌خرید، چیزی که تحویل می‌گیرید **رمز عبور نیست** — یک **توکن ورود** است. اکانت‌های NFA رمز و ایمیل در اختیار شما نمی‌گذارند؛ به‌جایش یک توکن دارید که استیم آن را می‌پذیرد.

مشکل اینجاست که استیم جایی ندارد که این توکن را دستی وارد کنید. NFAStore Tool همین کار را می‌کند: توکن را می‌گیرد، آن را دقیقاً همان‌جایی می‌نویسد که استیم بعد از یک ورود موفق خودش می‌نوشت، و استیم را باز می‌کند. استیم بالا می‌آید و از قبل وارد شده است.

**سورس این برنامه باز است.** همین‌جا می‌توانید تک‌تک خطوطش را بخوانید و ببینید چه کار می‌کند و چه کار نمی‌کند.

---

## دانلود و نصب

۱. به [صفحهٔ Releases](https://github.com/imanxboy/NFAStore-Tool/releases/latest) بروید.
۲. فایل `nfastore-tool-setup.exe` را دانلود کنید، یا مستقیم از [این لینک](https://github.com/imanxboy/NFAStore-Tool/releases/latest/download/nfastore-tool-setup.exe).
۳. اجرایش کنید. نصب چند ثانیه طول می‌کشد.

> **ویندوز ممکن است هشدار بدهد.** پیام «Windows protected your PC» به این معنا نیست که فایل ویروس دارد — به این معناست که این نصاب گواهی امضای دیجیتال (code signing certificate) ندارد، که برای یک برنامهٔ رایگان چیز عادی است. روی **More info** و بعد **Run anyway** بزنید.

---

## طرز استفاده

### قدم صفر: یک بار استیم را باز کنید

اگر تازه ویندوز نصب کرده‌اید یا تا حالا با استیم وارد نشده‌اید، **اول یک بار استیم را باز کنید و با هر اکانتی وارد شوید**. استیم فقط بعد از اولین ورود، فایل‌های کانفیگی را می‌سازد که این برنامه باید در آن‌ها بنویسد. اگر این کار را نکنید، برنامه پیام می‌دهد که اول وارد استیم شوید.

### قدم یک: توکن را کپی کنید

بعد از خرید، در [پنل کاربری](https://nfastore.org/fa/dashboard) اطلاعات اکانت را می‌بینید. کل متن را کپی کنید. شکلش این‌طوری است:

```
76561199843081825----eyJ0eXAiOiJKV1QiLCJhbGciOiJFZERTQSJ9.eyJpc3MiOiJzdGVhbSIsInN1YiI6...
```

نگران فرمتش نباشید. برنامه هر شکلی را قبول می‌کند: با شمارهٔ استیم یا بدون آن، با فاصله وسط توکن، با خط شکسته، یا حتی وقتی اطلاعات اضافه مثل `csgoRank` و `vacStatus` هم چسبیده باشد.

### قدم دو: وارد کنید

برنامه را باز کنید و **Import account** را بزنید. متن را بچسبانید و تأیید کنید.

می‌توانید **چند اکانت را با هم** وارد کنید — هر اکانت را در یک خط جدا بگذارید.

### قدم سه: تمام

برنامه استیم را می‌بندد، توکن را می‌نویسد و استیم را دوباره باز می‌کند. استیم که بالا آمد، وارد شده‌اید.

### دفعات بعد

اکانت‌های واردشده در لیست می‌مانند. برای عوض کردن اکانت فقط روی دکمهٔ **←** کنار اسمش بزنید. لازم نیست دوباره توکن را وارد کنید.

برنامه با بستن پنجره از بین نمی‌رود و کنار ساعت ویندوز (system tray) می‌ماند. از همان‌جا هم می‌توانید اکانت عوض کنید.

### گرفتن دوبارهٔ توکن

کنار هر اکانت یک دکمهٔ **کپی** هست که توکن ورود همان اکانت را دوباره روی کلیپ‌بورد می‌گذارد — مثلاً اگر بخواهید اکانت را به دوستتان بدهید، یا صفحهٔ سفارش دیگر دم دستتان نیست.

توکن از همان جایی خوانده می‌شود که خود برنامه نگهش داشته (قفل‌شده با DPAPI ویندوز) و **مستقیم از سمت Rust روی کلیپ‌بورد می‌رود** — یعنی اصلاً وارد رابط کاربری نمی‌شود و هیچ‌جا هم لاگ نمی‌شود.

> ⚠️ توکن یعنی خود اکانت. هرکس آن را داشته باشد می‌تواند وارد شود.

### تاریخ انقضای توکن

زیر اسم هر اکانت نوشته می‌شود توکنش تا کی کار می‌کند:

| نمایش | یعنی |
|---|---|
| `Token valid until …` | وقت دارد |
| `Token expires in N days` (نارنجی) | کمتر از دو هفته مانده |
| `Token expired …` (قرمز) | تمام شده و دیگر وارد نمی‌شود |

این تاریخ **داخل خود توکن** نوشته شده، پس برنامه آن را همان‌جا روی کامپیوتر می‌خواند — نه به اینترنت وصل می‌شود و نه جایی وارد می‌شود. اگر اکانتی تاریخ نداشته باشد، این خط اصلاً نشان داده نمی‌شود.

در حالت Streamer mode این خط هم مثل بقیهٔ مشخصات مخفی می‌شود.

### بررسی وضعیت اکانت — رنک و کول‌داون

کنار هر اکانت یک دکمهٔ **◎** هست. با زدنش، برنامه رنک Premier و لِوِلِ CS2 (رنکِ
پروفایل) و وضعیت کول‌داون رقابتی همان اکانت را می‌آورد و زیر اسمش نشان می‌دهد:

| نمایش | یعنی |
|---|---|
| `Premier 21,345` | امتیاز CS Rating پریمیر |
| `CS2 Level 37` | لِوِلِ پروفایلِ اکانت (۱ تا ۴۰) — همان «Rank 37» که بازی نشان می‌دهد |
| `No cooldown` (سبز) | آماده — کول‌داونی ندارد |
| `Cooldown 4h` (نارنجی) | تا این مدت نمی‌تواند رقابتی بازی کند |
| `Permanent ban` (قرمز) | بن دائمی رقابتی |
| `No rating yet` / `Not read` | پریمیر هنوز باز نشده / لِوِل خوانده نشد |

این کار برخلاف بقیهٔ برنامه **به اینترنت وصل می‌شود**: با همان توکن یک ورود به استیم
می‌زند (بدون عوض کردن یا مصرف توکن)، صفحهٔ رسمی «Game Coordinator Player Data» خودِ
استیم را — همان صفحه‌ای که صاحب اکانت روی سایت استیم می‌بیند — فقط می‌خواند (Premier،
کول‌داون و VAC از این صفحه می‌آید)، و لِوِلِ CS2 را چون در آن صفحه نیست، در همان ورود از
**Game Coordinatorِ CS2** می‌پرسد. چون یک ورود واقعی است چند ثانیه طول می‌کشد؛ برای
همین فقط وقتی خودتان دکمه را بزنید انجام می‌شود، نه خودکار. در حالت Streamer mode این
بخش هم مخفی است.

**دکمهٔ «Check all» (پایین پنجره)** همهٔ اکانت‌ها را پشتِ‌سرِهم چک می‌کند (یکی‌یکی، نه
هم‌زمان، تا استیم ریت‌لیمیت نکند) و کنار هر ردیف نتیجه را می‌زند: `Signs in` (لاگین
می‌شود)، کول‌داون/بن اگر داشته باشد، یا `Token expired` اگر توکنش پریده باشد.

**اکانت‌هایی که خودتان با Steam واردشان شده‌اید** (و توکن دستی نداده‌اید) هم چک می‌شوند:
اگر توکنی از خودمان برای اکانت نداشته باشیم، توکنی که **خودِ Steam** موقع لاگین در
`local.vdf` (ConnectCache) ذخیره کرده — که با DPAPIِ همین کاربر رمز شده — بازیابی می‌شود
و همان برای چک استفاده می‌شود. اگر نه توکن خودمان باشد و نه توکن ذخیره‌شدهٔ Steam، پیام
می‌دهد که اکانت را import کن یا یک‌بار در Steam واردش شو.

---

## تنظیمات

| گزینه | کارش چیست |
|---|---|
| **Always invisible** | استیم را در حالت نامرئی بالا می‌آورد. پیش‌فرض روشن است. |
| **Cancel downloads on login** | جلوی دانلود خودکار آپدیت بازی‌ها را موقع ورود می‌گیرد. بازی وقتی خودتان اجرایش کنید آپدیت می‌شود. |
| **Streamer mode** | اسم اکانت‌ها را در لیست مخفی می‌کند. برای وقتی که استریم می‌کنید یا صفحه‌تان را share کرده‌اید. |
| **Launch Steam minimized** | استیم را مینیمایز باز می‌کند. |
| **Mute notifications on login** | صدا و نوتیفیکیشن‌های دوستان استیم را خاموش می‌کند. |

بالای همین پنجره هم نسخهٔ فعلی برنامه نوشته شده و یک دکمهٔ **Check for updates** دارد.

---

## آپدیت برنامه

در Settings دکمهٔ **Check for updates** را بزنید. اگر نسخهٔ جدیدی باشد، شمارهٔ آن را
نشان می‌دهد و دکمه تبدیل به **Update now** می‌شود. با زدنش:

۱. برنامه خودش نصاب را دانلود می‌کند و درصدش را نشان می‌دهد
۲. چک می‌کند فایل واقعاً نصاب ویندوز باشد
۳. نصاب را در حالت بی‌سؤال اجرا می‌کند و برنامه بسته می‌شود
۴. بعد از نصب، برنامه خودش دوباره باز می‌شود

هیچ سؤالی وسط کار پرسیده نمی‌شود — نه «نسخهٔ قبلی را حذف کنم؟» و نه چیز دیگر.
اکانت‌ها و تنظیماتتان هم سر جایشان می‌مانند.

**اگر دانلود نشد چه؟** سروری که فایل نصب روی آن است از ایران بدون فیلترشکن باز
نمی‌شود (اندازه گرفتیم: صفر بایت در ۲۰ ثانیه، و ۶۴ کیلوبایت در ۱٫۵ ثانیه با
فیلترشکن). در این حالت برنامه خودش مرورگر شما را باز می‌کند تا از آنجا دانلود
کنید — یعنی همان راه قبلی، ولی به‌عنوان راه دوم نه راه اصلی. اگر فیلترشکن روشن
باشد، دانلود داخل برنامه انجام می‌شود.

---

## این برنامه با کامپیوتر شما چه می‌کند

چون سورس باز است، دقیقاً می‌شود گفت. هر چیزی که در ادامه آمده در همین کدها قابل بررسی است:

**چه چیزهایی را می‌خواند**

- مسیر نصب استیم، از رجیستری (`HKCU\SOFTWARE\Valve\Steam`)
- فایل‌های کانفیگ استیم: `config.vdf`، `loginusers.vdf`، `local.vdf`
- عکس پروفایل اکانت‌ها از کش خود استیم، فقط برای نشان دادن در لیست

**چه چیزهایی را می‌نویسد**

- توکن را، **رمزشده با DPAPI ویندوز**، در `local.vdf` استیم — همان روشی که خود استیم توکنش را ذخیره می‌کند
- اکانت را به لیست اکانت‌های استیم اضافه می‌کند
- `AutoLoginUser` را در رجیستری ست می‌کند تا استیم بداند با کدام اکانت بالا بیاید
- تنظیمات و لیست اکانت‌های خودش را در `%APPDATA%\ir.nfastore.tool\`

**چه پروسه‌هایی را اجرا می‌کند**

فقط دو تا: `taskkill` برای بستن استیم، و `steam.exe` برای باز کردنش.

**و مهم‌تر: چه کار نمی‌کند**

- رمز عبوری نمی‌خواهد و جایی نمی‌فرستد. اصلاً رمزی در کار نیست.
- به فایل‌های شخصی‌تان کاری ندارد. فقط پوشهٔ استیم و پوشهٔ خودش.
- چیزی را برای همیشه پاک نمی‌کند. حتی دکمهٔ **Clear** فقط کش ورود را خالی می‌کند و اکانت‌های ذخیره‌شده سر جایشان می‌مانند.
- آماری از شما جایی نمی‌فرستد — نه به سرور ما، نه به هیچ سوم‌شخصی.

**کجا به اینترنت وصل می‌شود**

سه جا، و هر سه در همین کد قابل بررسی است:

- **بررسی آپدیت:** به GitHub نگاه می‌کند ببیند نسخهٔ تازه‌ای هست، و اگر خودتان بزنید، نصاب را از GitHub دانلود می‌کند. توکن اکانت‌ها در این کار هیچ نقشی ندارد.
- **بررسی وضعیت اکانت (دکمهٔ ◎):** فقط وقتی بزنید — با توکن همان اکانت یک ورود به استیم می‌زند و صفحهٔ رنک/کول‌داونِ خودِ استیم را می‌خواند. توکن **به سرور خودِ استیم** می‌رود (همان‌جایی که برای ورود به اکانت هم می‌رود)، نه به سرور ما و نه به هیچ سوم‌شخصی، و **عوض یا باطل نمی‌شود**.

> ورود به استیم برای بررسی وضعیت را یک برنامهٔ جانبی کوچک انجام می‌دهد
> (`cs2-rank-sidecar.exe` که کنار برنامه نصب می‌شود؛ سورس پایتونش در پوشهٔ
> `sidecar/`). کتابخانهٔ `ureq` در `src-tauri/Cargo.toml` هم فقط برای دانلود نصابِ
> آپدیت است. تا پیش از این دو قابلیت، این برنامه هیچ کد شبکه‌ای نداشت.

---

## مشکلات رایج

**«Please open Steam and login to any account first»**
هنوز یک بار با استیم وارد نشده‌اید. استیم را باز کنید، با هر اکانتی وارد شوید، بعد دوباره امتحان کنید. (قدم صفر بالا)

**«Token does not look like a valid JWT»**
متن کامل کپی نشده. برگردید و کل خط را از اول تا آخر کپی کنید.

**«Steam is not installed or its registry path could not be read»**
برنامه نتوانست استیم را پیدا کند. اگر استیم نصب است، یک بار بازش کنید و ببندید تا رجیستری ساخته شود.

**استیم باز شد ولی وارد نشدم**
معمولاً یعنی توکن منقضی شده. توکن‌های استیم عمر محدودی دارند. با پشتیبانی NFAStore تماس بگیرید.

**ویندوز نصاب را بلاک کرد**
بخش دانلود و نصب را ببینید — گواهی امضا ندارد، ویروس ندارد.

**اکانتی از لیست پاک نمی‌شود**
تا نسخهٔ ۱.۰.۰ این یک باگ بود: اگر اکانتی از خود استیم حذف شده بود، حذفش از این
لیست با پیام «Account was not found in loginusers.vdf» شکست می‌خورد و هیچ‌وقت پاک
نمی‌شد. از نسخهٔ ۱.۱.۰ حذف همیشه انجام می‌شود؛ اگر استیم هم بسته نشده باشد یا نصب
نباشد، اکانت از لیست می‌رود و برنامه فقط می‌گوید که استیم را نتوانست تمیز کند.

---


## لینک‌های مفید

- [راهنمای ورود به اکانت NFA](https://nfastore.org/fa/how-to-login)
- [بررسی توکن استیم، داخل مرورگر خودتان](https://nfastore.org/fa/tools/steam-token-checker)
- [اکانت NFA چیست؟](https://nfastore.org/fa/guides/nfa-account)
- [ویدیوی آموزش ورود در آپارات](https://www.aparat.com/v/sujd5f2)

---

<div dir="ltr">

## English

**Log in to an NFA Steam account with its token: no password, no browser.**

When you buy an account from [NFAStore](https://nfastore.org/en), you get a login token, not a password. NFA accounts come without the password or the email; the token is what Steam accepts instead. Steam has no box to paste a token into, so NFAStore Tool writes it exactly where Steam itself saves a login after a successful sign-in, then starts Steam already signed in. The source is open: every line it runs is in this repository.

### Download

Get [`nfastore-tool-setup.exe`](https://github.com/imanxboy/NFAStore-Tool/releases/latest/download/nfastore-tool-setup.exe) from the [latest release](https://github.com/imanxboy/NFAStore-Tool/releases/latest) (Windows x64). The installer is not code-signed, so Windows may say "Windows protected your PC": click **More info**, then **Run anyway**.

### Use

1. If Steam has never been signed in on this PC, open it once and sign in with any account, so its config files exist.
2. Copy the whole login from your NFAStore order page. Any shape works: with or without the SteamID, with spaces or line breaks in it, or with extra fields attached.
3. Open the tool, click **Import account**, paste and confirm. One account per line imports several at once.
4. The tool closes Steam, writes the token and starts Steam again, signed in.

Imported accounts stay in the list; switch with the **←** button next to a name. Each account shows when its token expires, read from the token itself on your PC. The **◎** button reads the account's CS2 Premier rating, profile level and competitive cooldown, and **Check all** checks every account in turn.

> ⚠️ Never sign out of Steam on an NFA account: that invalidates the token for good. When you are done, just close Steam.

### What it does to your PC

- Reads Steam's install path from the registry, and Steam's `config.vdf`, `loginusers.vdf` and `local.vdf`.
- Writes the token into `local.vdf`, sealed with Windows DPAPI, the same way Steam stores its own; adds the account to Steam's list and sets `AutoLoginUser`. Its own settings live in `%APPDATA%\ir.nfastore.tool\`.
- Runs only two processes: `taskkill` to close Steam, and `steam.exe` to open it.
- Goes online only to check GitHub for updates, and, when you press ◎, to sign in to Steam's own servers and read the account's rank. The token goes to Steam and nowhere else; nothing is sent to us or to anyone else.

### More

- [How to log in to an NFA Steam account](https://nfastore.org/en/how-to-login)
- [Steam token checker: decode a login token in your browser](https://nfastore.org/en/tools/steam-token-checker)
- [What is an NFA account?](https://nfastore.org/en/guides/nfa-account)

</div>

---

<div align="center">

ساخته‌شده برای [NFAStore](https://nfastore.org)

</div>
