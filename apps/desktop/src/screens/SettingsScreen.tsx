import { useEffect, useState } from "react";
import type { ThemePref } from "../lib/theme";
import { fmtBytes, fmtDayTime } from "../lib/format";
import {
  useBackups,
  useConfigureFullAccess,
  useDaemon,
  useFullAccessStatus,
  useObserverStatus,
  useRemoteStatus,
  useRunBackup,
  useRunScan,
  useSetSetting,
  useSettings,
  useTelegramConfigure,
  useTelegramDisable,
  useTelegramTest,
  useWhatsappConfigure,
  useWhatsappDisable,
  useWhatsappTest,
} from "../lib/queries";

export function SettingsScreen({
  theme,
  setTheme,
}: {
  theme: ThemePref;
  setTheme: (t: ThemePref) => void;
}) {
  const settings = useSettings();
  const setSetting = useSetSetting();
  const daemon = useDaemon();
  const backups = useBackups();
  const runBackup = useRunBackup();
  const fullStatus = useFullAccessStatus();
  const configureFull = useConfigureFullAccess();
  const [fullCurrent, setFullCurrent] = useState("");
  const [fullNew, setFullNew] = useState("");
  const [fullConfirm, setFullConfirm] = useState("");
  const [fullMsg, setFullMsg] = useState<string | null>(null);

  const observer = useObserverStatus();
  const runScan = useRunScan();
  const remote = useRemoteStatus();
  const tgConfigure = useTelegramConfigure();
  const tgDisable = useTelegramDisable();
  const tgTest = useTelegramTest();
  const [tgToken, setTgToken] = useState("");
  const [tgUser, setTgUser] = useState("");
  const [tgChat, setTgChat] = useState("");
  const [tgMsg, setTgMsg] = useState<string | null>(null);
  const waConfigure = useWhatsappConfigure();
  const waDisable = useWhatsappDisable();
  const waTest = useWhatsappTest();
  const [waBase, setWaBase] = useState("");
  const [waKey, setWaKey] = useState("");
  const [waPhone, setWaPhone] = useState("");
  const [waMsg, setWaMsg] = useState<string | null>(null);

  const saved = settings.data?.display_name ?? "";
  const [name, setName] = useState(saved);
  useEffect(() => setName(saved), [saved]);

  const health = daemon.data?.health;

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Ayarlar</h1>
        </div>
      </header>

      <section className="section">
        <div className="section-head">
          <h2>Genel</h2>
        </div>
        <div className="settings-card">
          <label className="field">
            <span>Ad (selamlama için)</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              onBlur={() => {
                const n = name.trim();
                if (n && n !== saved) setSetting.mutate({ key: "display_name", value: n });
              }}
            />
          </label>
          <label className="field">
            <span>Görünüm</span>
            <div className="seg">
              {(
                [
                  ["system", "Sistem"],
                  ["light", "Açık"],
                  ["dark", "Koyu"],
                ] as [ThemePref, string][]
              ).map(([key, label]) => (
                <button
                  key={key}
                  className={`seg-item${theme === key ? " active" : ""}`}
                  onClick={() => setTheme(key)}
                >
                  {label}
                </button>
              ))}
            </div>
          </label>
        </div>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Tam Erişim</h2>
          <span className="section-hint">yalnızca yerel parola onayı; remote kanallara kapalı</span>
        </div>
        <div className="settings-card full-access-card">
          <div className="kv">
            <span>Durum</span>
            <b className={fullStatus.data?.configured ? "text-ok" : "text-warn"}>
              {fullStatus.data?.configured ? "Parola ayarlı" : "Parola ayarlanmamış"}
            </b>
          </div>
          {fullStatus.data?.configured && (
            <label className="field field-wide">
              <span>Mevcut parola</span>
              <input
                type="password"
                value={fullCurrent}
                onChange={(e) => setFullCurrent(e.target.value)}
                autoComplete="off"
              />
            </label>
          )}
          <div className="field-pair full-password-row">
            <label className="field field-wide">
              <span>Yeni parola</span>
              <input
                type="password"
                value={fullNew}
                onChange={(e) => setFullNew(e.target.value)}
                placeholder="En az 10 karakter"
                autoComplete="new-password"
              />
            </label>
            <label className="field field-wide">
              <span>Yeni parola tekrar</span>
              <input
                type="password"
                value={fullConfirm}
                onChange={(e) => setFullConfirm(e.target.value)}
                autoComplete="new-password"
              />
            </label>
          </div>
          <div className="kv">
            <p className="settings-hint">
              Düz metin parola saklanmaz; Argon2 türevi macOS Keychain'de tutulur. Tam Erişim daemon
              yeniden başladığında veya {fullStatus.data?.unlockMinutes ?? 30}
              dakika hareketsizlikte otomatik kilitlenir.
            </p>
            <button
              className="btn btn-primary"
              disabled={configureFull.isPending || fullNew.length < 10 || fullNew !== fullConfirm}
              onClick={() => {
                if (fullNew !== fullConfirm) {
                  setFullMsg("✗ Yeni parolalar eşleşmiyor.");
                  return;
                }
                configureFull.mutate(
                  {
                    newPassword: fullNew,
                    currentPassword: fullCurrent || undefined,
                  },
                  {
                    onSuccess: () => {
                      setFullCurrent("");
                      setFullNew("");
                      setFullConfirm("");
                      setFullMsg("✓ Tam Erişim parolası Keychain'e kaydedildi.");
                    },
                    onError: (e) => setFullMsg(`✗ ${e.message}`),
                  },
                );
              }}
            >
              {configureFull.isPending
                ? "Kaydediliyor…"
                : fullStatus.data?.configured
                  ? "Parolayı değiştir"
                  : "Parolayı oluştur"}
            </button>
          </div>
          {fullMsg && <p className="settings-hint">{fullMsg}</p>}
        </div>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Arka Plan Servisi</h2>
        </div>
        <div className="settings-card">
          <div className="kv">
            <span>Durum</span>
            <b className={daemon.data?.connected ? "text-ok" : "text-danger"}>
              {daemon.data?.connected ? "Bağlı" : "Bağlı değil"}
            </b>
          </div>
          {health && (
            <>
              <div className="kv">
                <span>Sürüm</span>
                <b>v{health.version}</b>
              </div>
              <div className="kv">
                <span>Çalışma süresi</span>
                <b>
                  {health.uptimeSecs < 3600
                    ? `${Math.max(1, Math.floor(health.uptimeSecs / 60))} dk`
                    : `${(health.uptimeSecs / 3600).toFixed(1)} sa`}
                </b>
              </div>
              <div className="kv">
                <span>Veri dizini</span>
                <code>{health.dataDir}</code>
              </div>
              <div className="kv">
                <span>Socket</span>
                <code>{health.socketPath}</code>
              </div>
            </>
          )}
          <p className="settings-hint">
            Login'de otomatik başlatmak için terminalden: <code>personal-opsd install-launchd</code>
          </p>
        </div>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Gözlemci</h2>
          <span className="section-hint">yalnızca onaylı proje klasörleri; salt-okunur</span>
        </div>
        <div className="settings-card">
          <div className="kv">
            <span>Durum</span>
            <b className={observer.data?.running ? "text-ok" : "text-danger"}>
              {observer.data?.running ? "Çalışıyor" : "Kapalı"}
            </b>
          </div>
          <div className="kv">
            <span>İzlenen klasör</span>
            <b>{observer.data?.watchedPaths.length ?? 0}</b>
          </div>
          {observer.data?.watchedPaths.map((p) => (
            <div key={p} className="kv kv-quiet">
              <code>{p}</code>
            </div>
          ))}
          {observer.data?.lastScanAt && (
            <div className="kv">
              <span>Son tarama</span>
              <b>{fmtDayTime(observer.data.lastScanAt)}</b>
            </div>
          )}
          {observer.data?.lastSummary && observer.data.lastSummary.errors.length > 0 && (
            <div className="kv kv-quiet">
              <span className="text-warn">
                {observer.data.lastSummary.errors.length} tarama uyarısı:{" "}
                {observer.data.lastSummary.errors[0]}
              </span>
            </div>
          )}
          <div className="kv">
            <span>Manuel tarama</span>
            <button
              className="btn"
              disabled={runScan.isPending}
              onClick={() => runScan.mutate(undefined)}
            >
              {runScan.isPending ? "Taranıyor…" : "Şimdi tara"}
            </button>
          </div>
        </div>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Mesajlaşma — Telegram</h2>
          <span className="section-hint">
            uzak mesajlar yalnızca gelen kutusuna veri ekler; asla komut çalıştıramaz
          </span>
        </div>
        <div className="settings-card">
          <div className="kv">
            <span>Durum</span>
            <b className={remote.data?.telegram.polling ? "text-ok" : ""}>
              {remote.data?.telegram.polling
                ? `Dinleniyor${remote.data.telegram.botName ? ` (@${remote.data.telegram.botName})` : ""}`
                : remote.data?.telegram.configured
                  ? "Yapılandırıldı, bekliyor"
                  : "Bağlı değil"}
            </b>
          </div>
          {remote.data?.telegram.lastError && (
            <div className="kv kv-quiet">
              <span className="text-danger">{remote.data.telegram.lastError}</span>
            </div>
          )}
          {!remote.data?.telegram.configured || !remote.data.telegram.enabled ? (
            <>
              <label className="field field-wide">
                <span>Bot token (yalnızca macOS Keychain'e yazılır)</span>
                <input
                  type="password"
                  value={tgToken}
                  onChange={(e) => setTgToken(e.target.value)}
                  placeholder="123456789:AAH…"
                  autoComplete="off"
                />
              </label>
              <div className="field-pair">
                <label className="field">
                  <span>İzinli Telegram user ID</span>
                  <input
                    value={tgUser}
                    onChange={(e) => setTgUser(e.target.value)}
                    placeholder="ör. 12345678"
                  />
                </label>
                <label className="field">
                  <span>İzinli chat ID</span>
                  <input
                    value={tgChat}
                    onChange={(e) => setTgChat(e.target.value)}
                    placeholder="ör. 12345678"
                  />
                </label>
              </div>
              <div className="kv">
                <span />
                <button
                  className="btn btn-primary"
                  disabled={
                    tgConfigure.isPending || !tgToken.trim() || !tgUser.trim() || !tgChat.trim()
                  }
                  onClick={() =>
                    tgConfigure.mutate(
                      {
                        token: tgToken.trim(),
                        allowedUserId: tgUser.trim(),
                        allowedChatId: tgChat.trim(),
                      },
                      {
                        onSuccess: (r) => {
                          setTgMsg(`✓ Bağlandı: @${r.botName}`);
                          setTgToken("");
                        },
                        onError: (e) => setTgMsg(`✗ ${e.message}`),
                      },
                    )
                  }
                >
                  {tgConfigure.isPending ? "Doğrulanıyor…" : "Bağla ve doğrula"}
                </button>
              </div>
            </>
          ) : (
            <div className="kv">
              <span>Bağlantı</span>
              <span className="field-pair">
                <button
                  className="btn btn-small"
                  disabled={tgTest.isPending}
                  onClick={() =>
                    tgTest.mutate(undefined, {
                      onSuccess: (r) => setTgMsg(`✓ Bot erişilebilir: @${r.botName}`),
                      onError: (e) => setTgMsg(`✗ ${e.message}`),
                    })
                  }
                >
                  Test et
                </button>
                <button
                  className="btn btn-small btn-quiet"
                  onClick={() =>
                    tgDisable.mutate(undefined, {
                      onSuccess: () => setTgMsg("Telegram devre dışı; token Keychain'den silindi."),
                    })
                  }
                >
                  Devre dışı bırak
                </button>
              </span>
            </div>
          )}
          {tgMsg && <p className="settings-hint">{tgMsg}</p>}
          <p className="settings-hint">
            Bot'u BotFather'dan oluştur; yalnızca senin user ID'nden gelen mesajlar işlenir. Uzaktan
            onay/mod değişikliği tasarım gereği imkânsızdır (bkz. THREAT_MODEL).
          </p>
        </div>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Mesajlaşma — WhatsApp</h2>
          <span className="section-hint">
            yalnızca giden bildirim; gelen yön tasarım gereği kapalı (inbound port yok)
          </span>
        </div>
        <div className="settings-card">
          <div className="kv">
            <span>Durum</span>
            <b className={remote.data?.whatsapp.configured ? "text-ok" : ""}>
              {remote.data?.whatsapp.configured
                ? `Yapılandırıldı${remote.data.whatsapp.phoneNumber ? ` (${remote.data.whatsapp.phoneNumber})` : ""}`
                : "Bağlı değil"}
            </b>
          </div>
          {remote.data?.whatsapp.configured && remote.data.whatsapp.baseUrl && (
            <div className="kv kv-quiet">
              <code>{remote.data.whatsapp.baseUrl}</code>
            </div>
          )}
          {!remote.data?.whatsapp.configured ? (
            <>
              <label className="field field-wide">
                <span>Bot API adresi</span>
                <input
                  value={waBase}
                  onChange={(e) => setWaBase(e.target.value)}
                  placeholder="https://bot.example.com"
                  autoComplete="off"
                />
              </label>
              <div className="field-pair">
                <label className="field">
                  <span>API anahtarı (yalnızca Keychain'e yazılır)</span>
                  <input
                    type="password"
                    value={waKey}
                    onChange={(e) => setWaKey(e.target.value)}
                    autoComplete="off"
                  />
                </label>
                <label className="field">
                  <span>Bildirim numarası (ülke kodlu, +'sız)</span>
                  <input
                    value={waPhone}
                    onChange={(e) => setWaPhone(e.target.value)}
                    placeholder="ör. 90555…"
                  />
                </label>
              </div>
              <div className="kv">
                <span />
                <button
                  className="btn btn-primary"
                  disabled={
                    waConfigure.isPending || !waBase.trim() || !waKey.trim() || !waPhone.trim()
                  }
                  onClick={() =>
                    waConfigure.mutate(
                      {
                        baseUrl: waBase.trim(),
                        apiKey: waKey.trim(),
                        phoneNumber: waPhone.trim(),
                      },
                      {
                        onSuccess: (r) => {
                          setWaMsg(`✓ ${r.status}`);
                          setWaKey("");
                        },
                        onError: (e) => setWaMsg(`✗ ${e.message}`),
                      },
                    )
                  }
                >
                  {waConfigure.isPending ? "Kaydediliyor…" : "Bağla"}
                </button>
              </div>
            </>
          ) : (
            <div className="kv">
              <span>Bağlantı</span>
              <span className="field-pair">
                <button
                  className="btn btn-small"
                  disabled={waTest.isPending}
                  onClick={() =>
                    waTest.mutate(undefined, {
                      onSuccess: (r) => setWaMsg(`✓ ${r.status}`),
                      onError: (e) => setWaMsg(`✗ ${e.message}`),
                    })
                  }
                >
                  Test et
                </button>
                <button
                  className="btn btn-small btn-quiet"
                  onClick={() =>
                    waDisable.mutate(undefined, {
                      onSuccess: () =>
                        setWaMsg("WhatsApp devre dışı; API anahtarı Keychain'den silindi."),
                    })
                  }
                >
                  Devre dışı bırak
                </button>
              </span>
            </div>
          )}
          {waMsg && <p className="settings-hint">{waMsg}</p>}
          <p className="settings-hint">
            Kendi self-hosted WhatsApp botuna bağlanır (send-to-user API'si). Hatırlatma ve rutin
            brifleri bu numaraya iletilir; WhatsApp'tan gelen mesajlar işlenmez.
          </p>
        </div>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Veri</h2>
          <span className="section-hint">yedekler secret içermez; son 10 tanesi tutulur</span>
        </div>
        <div className="settings-card">
          <div className="kv">
            <span>Yerel yedek</span>
            <button
              className="btn"
              disabled={runBackup.isPending}
              onClick={() => runBackup.mutate(undefined)}
            >
              {runBackup.isPending ? "Alınıyor…" : "Şimdi yedek al"}
            </button>
          </div>
          {(backups.data ?? []).map((b) => (
            <div key={b.fileName} className="kv kv-quiet">
              <code>{b.fileName}</code>
              <span>
                {fmtBytes(b.sizeBytes)}
                {b.createdAt && ` · ${fmtDayTime(b.createdAt)}`}
              </span>
            </div>
          ))}
        </div>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Hakkında</h2>
        </div>
        <div className="settings-card">
          <p className="settings-hint">
            Personal Ops — local-first kişisel operasyon yöneticisi. Veriler bu Mac'te kalır; remote
            kanallar hiçbir koşulda komut çalıştıramaz. Ayrıntı: <code>docs/THREAT_MODEL.md</code>
          </p>
        </div>
      </section>
    </div>
  );
}
