# Panduan Setup Proyek di Laptop Baru (Windows)

Dokumen ini menjelaskan langkah-langkah memindahkan dan menjalankan bot **PR.Social Multi-Worker Engine** pada laptop/komputer baru dari nol.

---

## 1. Persiapan di Laptop Lama (Upload ke GitHub)

Jalankan perintah ini di PowerShell pada laptop Anda saat ini untuk mengunggah proyek ke GitHub:

```powershell
cd "d:\Arya Files\code\Automation\PR.Social\rust-browser-click-engine"

# 1. Inisialisasi git jika belum
git init

# 2. Tambahkan semua file (target/ dan logs/ otomatis di-ignore oleh .gitignore)
git add .

# 3. Buat commit
git commit -m "feat: complete PR.Social multi-worker engine with anti-checkpoint & ultra-low-ram"

# 4. Ganti nama branch utama ke main
git branch -M main

# 5. Hubungkan ke repository GitHub Anda (ganti URL di bawah dengan URL repo Anda)
git remote add origin https://github.com/USERNAME/REPO_NAME.git

# 6. Push ke GitHub
git push -u origin main
```

---

## 2. Instalasi Cepat di Laptop Baru (1-Click Setup)

Di laptop baru, buka **PowerShell** dan lakukan langkah berikut:

### Langkah A: Clone Repository
```powershell
# Clone repo Anda
git clone https://github.com/USERNAME/REPO_NAME.git
cd REPO_NAME
```

### Langkah B: Jalankan Skrip Setup Otomatis
```powershell
powershell -ExecutionPolicy Bypass -File .\setup.ps1
```
*Skrip `setup.ps1` akan otomatis:*
- Mengecek dan mengunduh Google Chrome resmi jika belum ada.
- Mengecek dan memasang Rust toolchain (`cargo` & `rustc`) jika belum ada.
- Membuat folder `logs/`.
- Mengompilasi engine ke mode rilis berkecepatan tinggi (`cargo build --release`).

---

## 3. Login Profil Worker (Hanya Sekali Saja)

Pada laptop baru, sesi login profil tersimpan permanen di folder `$HOME\.browser-automation-profile*`. Anda hanya perlu login sekali:

1. Jalankan Dashboard GUI lokal:
   ```powershell
   .\gui.ps1
   ```
2. Buka dashboard di browser Anda (`http://127.0.0.1:8888`).
3. Pada kartu **MANUAL WORKER LAUNCHER**:
   - Klik **Buka Chrome** untuk **Worker #1**.
   - Di jendela Chrome yang terbuka, login ke akun **PR.Social** dan akun **Instagram #1**.
   - Ulangi untuk **Worker #2**, **Worker #3**, dst., sesuai jumlah worker/batch yang Anda gunakan.
4. Tutup jendela Chrome setelah selesai login. Akun akan tersimpan permanen di laptop baru tersebut.

---

## 4. Menjalankan Bot 24 Jam di Laptop Baru

Setelah login profil selesai, pilih salah satu mode operasi berikut:

### Mode Rotasi 4 Batch (Paling Direkomendasikan 24/7)
Menjalankan 4 batch secara bergiliran (2 worker per batch, 50 task per worker). Browser otomatis dibuka dan ditutup per batch untuk menjaga RAM di bawah ~360 MB:
```powershell
cargo run --release -- batch-follow --workers-per-batch 2 --batches 4 --tasks 50
```

### Mode Cepat 2 Worker
```powershell
cargo run --release -- auto-follow --workers 2 --fast --max 50
```

### Menjalankan Lewat Web Dashboard GUI
```powershell
.\gui.ps1
```
Lalu klik tombol **"Jalankan Rotasi Batch 1..4"** atau **"Jalankan 2 Worker Cepat"** di dashboard browser.

---

## 5. Tips Performa untuk Laptop Bekas / RAM 2 GB

Jika laptop baru Anda memiliki spesifikasi rendah (RAM 2 GB):
1. **Perbesar Paging File (Virtual Memory)** ke 4–8 GB (di *System Properties $\rightarrow$ Performance $\rightarrow$ Virtual Memory*).
2. **Atur "When I close the lid" menjadi "Do nothing"** di Control Panel Power Options agar layar laptop bisa ditutup rapat saat bot berjalan.
3. Gunakan mode `batch-follow` dengan `--workers-per-batch 1` atau `2` agar pemakaian RAM selalu di bawah 400 MB.

