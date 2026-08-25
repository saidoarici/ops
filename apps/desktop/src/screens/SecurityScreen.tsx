import { useState } from "react";
import { IconShield } from "../components/Icons";
import { fmtDayTime, fmtTime } from "../lib/format";
import {
  useAgentSessions,
  useAudit,
  useFullAccessStatus,
  useRemoteMessages,
  useRemoteStatus,
  useVerifyAudit,
} from "../lib/queries";
import type { AuditVerifyReport } from "../lib/types";

function Check({ ok, label, detail }: { ok: boolean | null; label: string; detail?: string }) {
  return (
    <div className="kv">
      <span>
        <b className={ok === null ? "" : ok ? "text-ok" : "text-warn"}>
          {ok === null ? "•" : ok ? "✓" : "!"}
        </b>{" "}
        {label}
      </span>
      {detail && <span className="sec-detail">{detail}</span>}
    </div>
  );
}

const MODE_CAPS: [string, string, string][] = [
  ["Sor (ASK)", "—", "R0 · araç erişimi yok"],
  ["Oku (READ)", "READ_PROJECT_FILES", "R1 · yalnızca proje kökü"],
  ["Düzenle (EDIT)", "WRITE_PROJECT_FILES", "R2 · acceptEdits, kök dışına çıkamaz"],
  ["Uygula (ACT)", "RUN_APPROVED_TEST", "R2 · onaylı komut aileleri; sudo yasak"],
  ["Tam Erişim (FULL)", "FULL_LOCAL_ACCESS", "R4 · yerel parola + 30 dk kilit"],
];

export function SecurityScreen() {
  const remote = useRemoteStatus();
  const sessions = useAgentSessions();
  const audit = useAudit(300);
  const verify = useVerifyAudit();
  const fullAccess = useFullAccessStatus();
  const remoteMessages = useRemoteMessages();
  const [report, setReport] = useState<AuditVerifyReport | null>(null);

  const tg = remote.data?.telegram;
  const denied = (audit.data ?? []).filter((e) => e.result !== "OK").slice(0, 10);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Güvenlik Merkezi</h1>
          <div className="page-sub">AI'ya güvenme — permission sistemine güven</div>
        </div>
        <button
          className="btn"
          disabled={verify.isPending}
          onClick={() => verify.mutate(undefined, { onSuccess: setReport })}
        >
          <IconShield size={13} /> Audit zincirini doğrula
        </button>
      </header>

      {report && (
        <div className={`banner ${report.ok ? "banner-ok" : "banner-err"}`}>
          {report.ok ? "✓ " : "✗ "}
          {report.message}
        </div>
      )}

      <section className="section">
        <div className="section-head">
          <h2>Güvenlik Sağlığı</h2>
        </div>
        <div className="settings-card">
          <Check
            ok
            label="Public inbound port yok"
            detail="yalnızca UDS (0600) + outbound long-poll"
          />
          <Check
            ok
            label="Remote execution yolu yok"
            detail="RemoteIntent şemasında komut tipi tanımsız; derleme düzeyinde ayrık"
          />
          <Check
            ok
            label="Riskli onay yalnızca lokal UI"
            detail="uzaktan onay/mod değişikliği API'si mevcut değil"
          />
          <Check
            ok={tg ? !tg.configured || (tg.allowedUserSet && tg.allowedChatSet) : null}
            label="Telegram tek kullanıcıya kısıtlı"
            detail={
              tg?.configured
                ? tg.allowedUserSet
                  ? "allowlist aktif"
                  : "allowlist eksik!"
                : "bağlı değil"
            }
          />
          <Check
            ok={tg?.configured || fullAccess.data?.configured ? true : null}
            label="Secret'lar Keychain'de"
            detail={
              fullAccess.data?.configured
                ? "Tam Erişim Argon2 hash'i Keychain'de; düz parola DB'de yok"
                : tg?.configured
                  ? "bot token Keychain'de; DB'de secret alanı yok"
                  : "kayıtlı secret yok"
            }
          />
          <Check
            ok
            label="sudo / root yetkisi verilmez"
            detail="FULL yalnızca mevcut macOS kullanıcı hesabı kapsamındadır"
          />
        </div>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Bağlı Kanallar</h2>
        </div>
        <div className="settings-card">
          <div className="kv">
            <span>Telegram</span>
            <b className={tg?.polling ? "text-ok" : ""}>
              {tg?.polling
                ? `dinleniyor${tg.botName ? ` (@${tg.botName})` : ""}`
                : tg?.configured
                  ? "yapılandırıldı"
                  : "bağlı değil"}
            </b>
          </div>
          <div className="kv">
            <span>WhatsApp</span>
            <b>
              {remote.data?.whatsapp.configured ? "yapılandırıldı" : "bağlı değil (adapter hazır)"}
            </b>
          </div>
        </div>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Agent Yetkileri</h2>
          <span className="section-hint">mod → capability eşlemesi (allowlist)</span>
        </div>
        <div className="settings-card">
          {MODE_CAPS.map(([mode, cap, note]) => (
            <div key={mode} className="kv">
              <span>{mode}</span>
              <span className="sec-detail">
                {cap !== "—" && <code>{cap}</code>} {note}
              </span>
            </div>
          ))}
        </div>
      </section>

      {(remoteMessages.data ?? []).length > 0 && (
        <section className="section">
          <div className="section-head">
            <h2>Uzak Mesajlar</h2>
            <span className="section-hint">son 20 — yetkisiz göndericilerin içeriği saklanmaz</span>
          </div>
          <div className="audit-list">
            {(remoteMessages.data ?? []).map((m) => (
              <div key={m.id} className="audit-row">
                <span className="tl-time">{fmtTime(m.receivedAt)}</span>
                <span
                  className={`audit-result ${
                    m.authenticationState === "AUTHENTICATED" ? "ok" : "err"
                  }`}
                />
                <span className="audit-action ev-summary">
                  {m.authenticationState === "AUTHENTICATED"
                    ? m.rawText || "(boş)"
                    : `reddedilen gönderici: ${m.senderId}`}
                </span>
                {m.parsedIntent && <span className="chip chip-quiet">{m.parsedIntent.type}</span>}
                <span className="chip">{m.channel}</span>
              </div>
            ))}
          </div>
        </section>
      )}

      {(sessions.data ?? []).length > 0 && (
        <section className="section">
          <div className="section-head">
            <h2>Son Agent Oturumları</h2>
          </div>
          <div className="audit-list">
            {(sessions.data ?? []).slice(0, 6).map((s) => (
              <div key={s.id} className="audit-row">
                <span className="tl-time">{fmtDayTime(s.startedAt)}</span>
                <span className="audit-action ev-summary">{s.title ?? "Sohbet"}</span>
                <span className="chip">{s.provider === "CLAUDE" ? "Claude" : "Codex"}</span>
                <span className="chip chip-quiet">{s.mode}</span>
                <span className={`chip ${s.status === "FAILED" ? "chip-danger" : "chip-quiet"}`}>
                  {s.status}
                </span>
              </div>
            ))}
          </div>
        </section>
      )}

      <section className="section">
        <div className="section-head">
          <h2>Reddedilen / Hatalı İşlemler</h2>
          <span className="section-hint">audit'ten OK olmayan son kayıtlar</span>
        </div>
        {denied.length === 0 ? (
          <div className="settings-card">
            <div className="kv">
              <span className="text-ok">✓ Reddedilmiş ya da hatalı işlem yok</span>
            </div>
          </div>
        ) : (
          <div className="audit-list">
            {denied.map((e) => (
              <div key={e.id} className="audit-row">
                <span className="tl-time">{fmtTime(e.timestamp)}</span>
                <span className="audit-result err" />
                <span className="audit-action">{e.action}</span>
                {e.target && <code className="audit-target">{e.target}</code>}
                <span className="chip chip-danger">{e.result}</span>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
