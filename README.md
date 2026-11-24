# Rust Browser Click & DOM Inspector Engine

A local, manual-trigger, DOM-based and coordinate-based browser controller & inspector for Windows. It inspects page structure and sends mouse and keyboard input to a selected Chrome/Brave/Chromium tab via Chrome DevTools Protocol (CDP).

## Key Features

- **DOM-Based Page Inspection (`inspect`)**: Extracts page metadata (title, URL, readyState, viewport, scroll dimensions) and discovers interactive elements (buttons, links, inputs, textareas, selects, checkboxes, radios) with bounding rectangles, visibility, text, roles, attributes, and 0-based indices.
- **Anti-Restriction & Stealth Mode (`stealth.rs`)**: Automatic injection via `Page.addScriptToEvaluateOnNewDocument` to spoof `navigator.webdriver`, mock `navigator.plugins`, `navigator.languages`, `chrome.runtime`, and patch `Permissions.query`.
- **Human-Like Input (`input.rs`)**: Natural cubic Bézier curve mouse paths, micro-jitter, hover pause before clicking, random click coordinates within element bounds (not dead center), and variable-speed natural page scrolling.
- **Realistic Behavioral Engine**: Profile browsing with exploratory scrolling before following, weighted inter-task delays (8-25s), random task skip simulation (5%), batch rest intervals (90-180s every 12-20 tasks), and long session breaks (15-30m every 70-90 tasks).
- **Session & Task Logging (`logger.rs`)**: Audit trails saved per-worker in JSONL format inside `logs/` directory for task monitoring and analytics.
- **Multi-Worker Orchestration (`--workers 4`)**: Concurrent asynchronous task runners with staggered desynchronization offsets.
- **Automatic Safety Break Detection:** Mendeteksi otomatis pesan `<h3>A break to keep your accounts safe</h3>` dari PR.Social dan langsung menghentikan worker demi mencegah akun terkena restriction/banned.
- **Post-Cooldown Auto-Reload:** Otomatis me-reload halaman PR.Social setelah jeda 1 menit (tiap 10 task) untuk menyegarkan daftar task dan sesi.
- **Coordinate & Element Interaction (`click`, `click-element`, `type`, `key`, `scroll`, `move`)**: Trigger input events directly via CDP.
- **Screenshot Capture (`screenshot`)**: Capture viewport screenshots into PNG files.
- **Target Filtering**: Seamlessly connect to specific tabs using `--target <substring>`.

## Quick 1-Click Setup (Untuk Laptop / PC Baru)

Jika Anda baru meng-clone proyek ini ke laptop atau PC baru, cukup jalankan skrip instalasi otomatis:

```powershell
powershell -ExecutionPolicy Bypass -File .\setup.ps1
```
Skrip ini akan otomatis memeriksa dan memasang Google Chrome, Rust Toolchain (`cargo`), menyiapkan folder `logs/`, dan mengompilasi engine ke versi rilis.

Panduan lengkap instalasi langkah demi langkah tersedia di: **[SETUP_NEW_LAPTOP.md](SETUP_NEW_LAPTOP.md)**.

## Requirements

- Rust stable toolchain
- Chrome, Brave, or Chromium with remote debugging enabled
- PowerShell on Windows

## Build

```powershell
cargo build --release
```

## Web Control Center (Dashboard GUI Ringan)

Jalankan dashboard antarmuka visual berbasis web lokal (ringan, tanpa dependensi tambahan, tanpa Node.js/Python):

```powershell
.\gui.ps1
```
Dashboard akan otomatis terbuka di browser Anda (`http://127.0.0.1:8888`) untuk:
- Memantau status ke-8 Worker & 4 Batch secara real-time.
- Membuka jendela Chrome per worker / per batch hanya dengan 1 klik.
- Menjalankan bot (2 Worker Cepat atau Rotasi 4 Batch).
- Menghentikan bot kapan saja.
- Melihat aktivitas task terakhir secara langsung.

## Start browser with a dedicated profile

For Brave:

```powershell
.\start-brave.ps1
```

For Chrome:

```powershell
.\start-chrome.ps1
```

The debugging endpoint defaults to `http://127.0.0.1:9222`.

## Commands

### Page Information & DOM Inspection

```powershell
# Show active tab info (title, url, viewport, readyState)
cargo run --release -- info

# Inspect DOM interactive elements (buttons, links, inputs, textareas, selects, checkboxes)
cargo run --release -- inspect

# Inspect DOM with limit
cargo run --release -- inspect --limit 20

# Query specific elements using CSS selector
cargo run --release -- inspect --selector "button"
cargo run --release -- inspect --selector "input[type='text'], textarea"

# Only return elements that are currently visible
cargo run --release -- inspect --visible-only

# Filter elements by keyword (searches text, id, class, aria-label, etc.)
cargo run --release -- inspect --filter "submit"

# Inspect specific element by index
cargo run --release -- inspect --index 3

# Inspect entire DOM tree (including non-interactive tags)
cargo run --release -- inspect --all

# Output compact single-line JSON
cargo run --release -- inspect --compact
```

### Sistem Rotasi 4 Batch (`batch-follow`)

Jalankan rotasi 4 batch berurutan (2 worker per batch, 50 task per worker). Dilengkapi deteksi otomatis safety break `<span data-until="...">` + jitter 1-2 menit dan hitung mundur:

```powershell
# 1. Jalankan rotasi 4 batch standar (2 worker/batch, 50 task/worker, continuous loop)
cargo run --release -- batch-follow

# 2. Jalankan rotasi dengan target task kustom (misal 30 task per worker)
cargo run --release -- batch-follow --tasks 30

# 3. Batasi jumlah siklus rotasi penuh (misal 2 siklus)
cargo run --release -- batch-follow --cycles 2
```

#### Menjalankan 8 Chrome untuk 4 Batch:
```powershell
# Buka semua 8 jendela Chrome (Batch 1 s/d Batch 4)
.\start-8-chromes.ps1

# Atau buka batch tertentu saja (misal hanya Batch 1)
.\start-8-chromes.ps1 -Batch 1
```

### PR.Social Instagram Auto-Follow Loop (Stealth & Anti-Restriction)

Jalankan automation end-to-end task follow Instagram dengan opsi Stealth biasa atau Fast Mode:

```powershell
# 1. Mode Cepat / Fast Mode (Jeda 1-2s, jitter 1.5-2.5s pasca-follow, cooldown 1 menit tiap 10 task)
cargo run --release -- auto-follow --fast

# 2. Mode Cepat dengan custom delay minimal (misal 1 detik) dan batas task
cargo run --release -- auto-follow --fast --min-delay 1 --max 30

# 3. Mode Stealth Normal (1 worker, default delay aman: 8-25 detik acak + human behavior)
cargo run --release -- auto-follow

# 4. Mode 4 worker paralel
cargo run --release -- auto-follow --workers 4
```

#### Menjalankan 4 Browser Chrome Otomatis (Grid 2x2):

```powershell
.\start-4-chromes.ps1
```
Script ini akan membuka 4 jendela Chrome yang tersusun rapi di layar monitor Anda pada port 9222 s/d 9225 dengan profil terpisah `~/.browser-automation-profile-1` s/d `profile-4` serta flag stealth (`--disable-blink-features=AutomationControlled`, `--disable-infobars`, dll).

Alur kerja `auto-follow` (Stealth Engine):
1. **Stealth Script Injection:** Otomatis menyembunyikan CDP flag (`navigator.webdriver = false`, fake plugins, permissions mock).
2. **Desinkronisasi Start:** Tiap worker mulai dengan jeda bertahap (0s, 3s, 6s, 9s) agar request tidak serempak.
3. **Simulasi Membaca Task:** Jeda 1.2–3 detik sebelum mengambil aksi di PR.Social.
4. **Random Skip (5%):** Melewati task sesekali seperti pengguna manusia normal.
5. Fokus tab PR.Social & pilih task row / klik tombol **Start** (`[data-go="start"]`).
6. Menangani warning **"All places are taken"** otomatis dengan mengklik **Next task**.
7. Ekstrak link target Instagram dari tombol copy (`[data-copy]`) dan trigger klik copy.
8. Switch tab ke Instagram dan navigate ke URL profil.
9. **Eksplorasi Profil (Human Scroll):** Melakukan scrolling ke bawah melihat feed (3–7 detik) lalu scroll balik ke area Follow button.
10. **Human-like Mouse Movement:** Menggerakkan kursor mouse dengan kurva cubic Bézier, micro-jitter, hover pause (80–200ms), dan klik pada offset acak tombol.
11. **Post-Follow Delay:** Berhenti sejenak 2–5 detik di halaman Instagram.
12. Kembali ke tab PR.Social, klik tombol **Check my follow** (`[data-go="apicheck"]`).
13. Tunggu verifikasi API selesai lalu klik **Next task** (`[data-go="next"]`).
14. **Audit Logging:** Mencatat setiap task ke `logs/worker<ID>_<timestamp>.jsonl`.
15. **Weighted Delay:** Jeda antar-task 8–25 detik dengan distribusi normal/segitiga.
16. **Batch Cooldown:** Istirahat 90–180 detik setiap 12–20 task selesai.
17. **Session Limit:** Istirahat panjang 15–30 menit setelah mencapai target session (default ~80 task).

### Element & Coordinate Actions

```powershell
# Click an element by index found from inspect (clicks the center of its bounding box)
cargo run --release -- click-element 2

# Click at CSS viewport coordinates
cargo run --release -- click 350 220
cargo run --release -- click 350 220 --count 2

# Move pointer
cargo run --release -- move 350 220

# Scroll viewport
cargo run --release -- scroll 600 --x 500 --y 500

# Type text into focused element
cargo run --release -- type "Hello world"

# Press keyboard key (ENTER, TAB, ESC, BACKSPACE, SPACE)
cargo run --release -- key ENTER

# Save screenshot
cargo run --release -- screenshot "screen.png"

# Navigate tab
cargo run --release -- open "https://example.com"
```

### Inspect Output Format

The `inspect` command outputs structured JSON:

```json
{
  "page": {
    "title": "PR.SOCIAL — Dashboard",
    "url": "https://www.pr.social/dashboard",
    "readyState": "complete",
    "viewport": {
      "width": 1536.0,
      "height": 782.0
    },
    "scroll": {
      "width": 1521.0,
      "height": 991.0
    }
  },
  "elements": [
    {
      "index": 0,
      "tag": "button",
      "text": "Sign In",
      "role": "button",
      "id": "login-btn",
      "class": "btn btn-primary",
      "ariaLabel": "Sign in to account",
      "placeholder": null,
      "name": null,
      "type": "submit",
      "href": null,
      "disabled": false,
      "visible": true,
      "rect": {
        "x": 100.5,
        "y": 200.0,
        "width": 120.0,
        "height": 40.0
      }
    }
  ]
}
```

### Monitoring Dashboard (`monitor.ps1`)

Pantau status proses engine, port worker browser, ketersediaan tab (PR.Social & Instagram), serta aktivitas log task secara langsung:

```powershell
# Cek status sekali (one-shot)
.\monitor.ps1

# Mode pemantauan langsung / real-time dashboard (auto-refresh setiap 3 detik)
.\monitor.ps1 -Watch

# Custom interval refresh (misal tiap 5 detik)
.\monitor.ps1 -Watch -Interval 5
```

## Safety

Use only on websites and workflows where automation and inspection are allowed. Keep remote debugging ports bound to localhost.
