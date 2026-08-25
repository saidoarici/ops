# Personal Ops

> English version: [README.md](README.md)

macOS için local-first kişisel operasyon yöneticisi: görevler, projeler,
bekleyen işler, hatırlatmalar ve rutinler; git ve dosya hareketlerini kanıta
dönüştüren salt-okunur bir gözlemci; kendi Claude Code / Codex CLI'ını sıkı
yetki modlarında çalıştıran bir asistan ve hiçbir koşulda komut çalıştıramayan
bir Telegram gelen kutusu.

Bunu kendi günlük kullanımım için yaptım; çünkü aynı anda birkaç projeyi
yürütmenin pahalı kısmı görev girmek değil, işlerin durumunu kafanda taşımaktır.
Personal Ops bu durumu makinede tutar, gerçek sinyallerden (commit'ler, dosya
değişiklikleri, asistan oturumları) çıkarabildiğini çıkarır ve bir şeyi
değiştirmeden önce sorar.

Ürün arayüzü Türkçedir (kişisel bir araç); kod tanımlayıcıları ve `docs/`
altındaki belgeler İngilizce, kod yorumları Türkçedir.

## Ekran görüntüleri

Henüz ekran görüntüsü yok. Gerçek veriye dokunmadan, kurgusal bir çalışma
alanıyla kendin almak için:

```bash
export PERSONAL_OPS_DATA_DIR=/tmp/po-demo          # kısa yol (socket uzunluk sınırı)
cargo run -p ops-daemon -- seed-demo && cargo run -p ops-daemon &
cd apps/desktop && pnpm tauri dev                  # dev URL'sine #tasks, #assistant, … ekle
```

İyi adaylar: Bugün, Görevler, Asistan, Rutinler, Güvenlik Merkezi.

## Öne çıkanlar

* **Local-first.** `~/Library/Application Support/PersonalOps` altında SQLite,
  launchd altında bir Rust daemon'ı, Tauri masaüstü kabuğu. Bulut arka ucu,
  hesap ya da dinleyen bir ağ portu yok.
* **Görevler, projeler, bekleyenler, hatırlatmalar, rutinler.** Dokuz görev
  durumu; "birinden cevap bekleme" birinci sınıf izlenir; pencere kapalıyken
  de tetiklenen tek seferlik ve tekrarlı hatırlatmalar; sabah / akşam /
  haftalık brifler.
* **Deterministik Bugün görünümü.** Odak (en fazla üç görev, her biri "neden
  şimdi" gerekçeli), dikkat listesi (geciken, uzun süredir bekleyen, bloklu,
  durgun) ve zaman çizgisi — hepsi veriden hesaplanır, döngüde model yok.
* **Gözlemci.** Yalnızca onayladığın klasörleri izler (FSEvents + periyodik
  git2 taraması), commit'leri ve dosya hareketlerini kanıt olarak kaydeder,
  proje sağlığını hesaplar ve yarım kalmış işleri (commit'lenmemiş
  değişiklikler, push'lanmamış commit'ler, durgun süren görevler) göreve
  çevirebileceğin ya da yoksayabileceğin öneriler olarak yüzeye çıkarır.
* **Yetki modlu asistan.** Zaten kurulu `claude` ya da `codex` CLI'ını, açık
  araç allowlist'lerine ve sandbox'lara eşlenmiş ASK / READ / EDIT / ACT / FULL
  (Sor / Oku / Düzenle / Uygula / Tam Erişim) modlarında çalıştırır. ACT yerel
  onay, FULL ise Keychain'de Argon2 özeti olarak saklanan yerel bir parola ister.
* **Sınırlı uzak giriş.** Allowlist'teki tek göndericiden gelen Telegram
  mesajları gelen kutusuna görev ve not ekleyebilir, hatırlatma *önerisi*
  bırakabilir ve görev sorgulayabilir. O yüzeyde başka hiçbir şey yoktur.
  WhatsApp yalnızca giden yönlüdür.
* **Kurcalamaya dayanıklı audit günlüğü.** Her değişiklik aynı transaction
  içinde bir audit satırı yazar; satırlar SHA-256 ile zincirlenir ve CLI'dan
  ya da Güvenlik Merkezi ekranından doğrulanabilir.
* **Masaüstü cilası.** ⌘K komut paleti, ⌘N / ⌥Space hızlı yakalama, menü
  çubuğu simgesi, açık ve koyu tema, neyin bağlı olduğunu ve neyin
  reddedildiğini gösteren Güvenlik Merkezi.

## Nasıl çalışır

```text
┌──────────────────────────────┐
│  Masaüstü kabuğu (Tauri 2)   │  React + TypeScript, ince IPC istemcisi
└──────────────┬───────────────┘
               │ Unix domain socket (0600), NDJSON
┌──────────────▼───────────────┐
│  personal-opsd (Rust daemon) │  launchd kullanıcı ajanı, asla root
│  store · today · scheduler   │
│  observer · agent · remote   │
└──┬──────────┬──────────┬─────┘
   │          │          │
 git repoları  claude /   Telegram (giden long-poll)
 (onaylı       codex CLI  WhatsApp botu (yalnızca giden)
  klasörler)
```

Daemon tüm durumu ve arka plan işini üstlenir; pencere isteğe bağlıdır. Rust
çalışma alanı, sorumluluklar ayrık kalsın diye altı crate ve Tauri kabuğuna
bölünmüştür — uzak geçit crate'i asistan çalıştırıcısına bağlanamaz bile.
Ayrıntılar, protokol metod tablosu ve tasarım kararları için:
[docs/architecture.md](docs/architecture.md) (İngilizce).

## Güven ve yetki modeli

Uzak bir mesaj işletim sistemi komutuna dönüşemez. İzlediği yol:

```text
Telegram güncellemesi → gönderici + sohbet allowlist'i → replay kontrolü →
deterministik intent ayrıştırıcı → dört typed intent'ten biri → store yazımı → audit satırı
```

* Intent tipinin tam olarak dört varyantı vardır (`CREATE_TASK`,
  `CREATE_REMINDER_PROPOSAL`, `QUERY_TASK`, `ADD_NOTE`); execution benzeri
  varyantlar veri modelinde yoktur ve `ops-remote` crate'i ne asistan
  çalıştırıcısına ne de daemon'a bağımlıdır.
* Başka göndericilerin mesajları saklanmaz, ayrıştırılmaz, yanıtlanmaz.
* Mod değişiklikleri ve onaylar yalnızca yerel socket'te vardır; riskli olanlar
  (ACT, FULL) ek olarak yerel onay ya da parola ister.
* Asistanlar minimal ortam, onaylı çalışma dizini, zaman aşımı ve çıktı
  sınırıyla çalışır; `sudo` hiçbir zaman izinli değildir.
* Secret'lar (bot token'ı, WhatsApp anahtarı, Tam Erişim özeti) yalnızca macOS
  Keychain'de yaşar; ayarlar tablosu secret'a benzeyen anahtarları reddeder.

Tehditlerin, önlemlerin ve bunları sabitleyen regresyon testlerinin tam
listesi: [docs/threat-model.md](docs/threat-model.md) (İngilizce).

## Teknoloji yığını

* **Rust** çalışma alanı: `tokio`, `rusqlite` (gömülü SQLite), `git2`,
  `notify`, `reqwest` (rustls), `argon2`, `clap`, `serde`, `chrono`
* **Masaüstü:** Tauri 2, React 18, TypeScript (strict), TanStack Query, Vite
* **Platform:** macOS 12+ — launchd, Keychain (`/usr/bin/security`), FSEvents,
  `osascript` bildirimleri
* **AI sağlayıcıları:** Claude Code CLI ve Codex CLI; isteğe bağlı, kendi
  oturum açmalarıyla kullanılır

## Kurulum

Gereksinimler: macOS 12 ya da üstü, Rust stable (1.97 ile geliştirildi),
Node.js 20+ ve pnpm 10, Xcode Command Line Tools. İsteğe bağlı: asistan ekranı
için `PATH` üzerinde `claude` ve/veya `codex`.

```bash
git clone https://github.com/saidoarici/ops.git
cd ops

# 1. Daemon'ı derle ve başlat (birinci terminal)
cargo run -p ops-daemon              # = personal-opsd run

# 2. İsteğe bağlı: boş veritabanına kurgusal demo çalışma alanı yükle
cargo run -p ops-daemon -- seed-demo

# 3. Masaüstü uygulamasını başlat (ikinci terminal)
cd apps/desktop
pnpm install
pnpm tauri dev
```

Üretim tarzı kurulum:

```bash
cargo build --release -p ops-daemon
./target/release/personal-opsd install-launchd   # login'de başlar, gui/<uid> domain'i

cd apps/desktop && pnpm install && pnpm tauri build
# → target/release/bundle/macos/Personal Ops.app
```

Daemon komutları:

```bash
personal-opsd run                 # ön planda (varsayılan)
personal-opsd seed-demo [--force]
personal-opsd install-launchd | uninstall-launchd | launchd-status
personal-opsd verify-audit        # audit hash zincirini yeniden hesapla
personal-opsd backup              # çevrimiçi SQLite yedeği, son 10 tanesi tutulur
personal-opsctl context           # asistan için JSON bağlam
personal-opsctl project add --name "Ad" --path /yerel/klasor
personal-opsctl task add --title "Başlık" [--project-id <id>]
personal-opsctl task list | task complete --id <id>
```

## Yapılandırma

`.env` dosyası yoktur. Secret içeren her şey uygulama içindeki **Ayarlar**'dan
bir kez girilir ve macOS Keychain'de saklanır; gerisi SQLite `settings`
tablosunda durur.

| Ayar | Nerede | Not |
|---|---|---|
| Telegram bot token'ı, izinli user ID, izinli chat ID | Ayarlar → Telegram | Token → Keychain (`com.personalops.daemon` / `telegram_bot_token`), ID'ler → settings |
| WhatsApp bot adresi, API anahtarı, telefon numarası | Ayarlar → WhatsApp | Anahtar → Keychain; adres loopback değilse `https://` olmalı |
| Tam Erişim parolası | Ayarlar → Tam Erişim | Keychain'de Argon2 özeti olarak saklanır; 10–128 karakter |
| Görünen ad, tema | Ayarlar → Genel | Tema web görünümünün yerel deposunda tutulur |

Ortam değişkenleri (ikisi de isteğe bağlı):

| Değişken | Etkisi |
|---|---|
| `PERSONAL_OPS_DATA_DIR` | Veri dizinini (veritabanı, socket, yedekler) değiştirir. Yolu kısa tut: macOS socket yollarını ~100 karakterle sınırlar. Testlerde ve demo profillerinde kullanılır. |
| `RUST_LOG` | Daemon için `tracing` filtresi; varsayılan `info`. |

Veri ve günlükler:

```text
~/Library/Application Support/PersonalOps/{personalops.db,daemon.sock,Backups/}
~/Library/Logs/PersonalOps/daemon.log
~/Library/LaunchAgents/com.personalops.daemon.plist
```

## Geliştirme

```bash
cargo run -p ops-daemon                      # daemon, debug günlükleriyle (daha fazlası için RUST_LOG=debug)
cd apps/desktop && pnpm tauri dev            # sıcak yüklemeli UI; debug daemon'ı kendisi de başlatabilir

cargo fmt --all                              # rustfmt (ayar: rustfmt.toml)
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && pnpm lint && pnpm typecheck && pnpm format
```

Masaüstü uygulaması daemon'la yalnızca `ops_call(method, params)` üzerinden
konuşur; her metod [docs/architecture.md](docs/architecture.md) içinde
listelidir. Wire string'lerinin doğruluk kaynağı Rust enum'larıdır;
`apps/desktop/src/lib/types.ts` bunları elle yansıtır.

İkonlar `resources/icon-source.png` dosyasından üretilir
(`python3 scripts/make_icon.py`, ardından `pnpm tauri icon`).

## Test

```bash
cargo test --workspace                       # birim + entegrasyon, güvenlik regresyonları dahil
cd apps/desktop && pnpm lint && pnpm typecheck && pnpm format:check && pnpm build
```

Rust test paketi şunları kapsar: store ve durum geçişleri, Bugün motoru,
hatırlatma zamanlaması, rutin zamanlamaları, gerçek git repoları üzerinde
gözlemci (`git2` ile oluşturulur, shell yok), intent ayrıştırıcı, uzak geçit
(enjeksiyon, replay, allowlist, rate limit), asistan başlatma planları ve
sandbox kuralları, Keychain girdi doğrulaması, UDS sunucusu (izinler, istek
sınırı) ve dispatch düzeyindeki yetki kapıları. Testler gerçek Keychain'e ya da
ağa asla dokunmaz. Aynı komutlar CI'da da koşar (`.github/workflows/ci.yml`;
Rust için macOS, UI için Ubuntu runner'ı).

## Proje yapısı

```text
crates/
  ops-core/       modeller, SQLite store + audit zinciri, Bugün motoru, IPC tipleri, yol koruması
  ops-keychain/   macOS Keychain erişimi
  ops-observer/   git2 + FSEvents gözlemcisi → kanıt, tespitler, proje sağlığı
  ops-agent/      mod → allowlist eşlemeli Claude Code / Codex çalıştırıcısı
  ops-remote/     Telegram geçidi, intent ayrıştırıcı, WhatsApp giden adaptörü
  ops-daemon/     personal-opsd (sunucu, dispatch, scheduler, rutinler) ve personal-opsctl
apps/desktop/
  src/            React ekranları, bileşenler, tipli sorgu hook'ları
  src-tauri/      Tauri kabuğu (UDS proxy, tray, global kısayol)
docs/             architecture.md · threat-model.md · data-model.md
resources/        launchd plist şablonu, ikon kaynağı
scripts/          ikon üretici
```

## Mevcut durum

Personal Ops her gün kullandığım, çalışan, tek kullanıcılı bir uygulamadır.
Dağıtım için paketlenmemiştir: build'ler imzasız ve notarize edilmemiştir,
otomatik güncelleme yoktur, arayüz yalnızca Türkçedir. Sıradaki işler:
İngilizce arayüz dili, imzalama/notarization, daha zengin brifler ve kanıtların
anahtar kelimeyle görevlere bağlanması.

## Lisans

[MIT](LICENSE)
