import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { IconActivity, IconShield } from "../components/Icons";
import { AUDIT_ACTION_LABEL, EVIDENCE_LABEL, fmtTime } from "../lib/format";
import { useAudit, useEvidence, useVerifyAudit } from "../lib/queries";
import type { AuditEvent, AuditVerifyReport, Evidence } from "../lib/types";

const dayFmt = new Intl.DateTimeFormat("tr-TR", {
  weekday: "long",
  day: "numeric",
  month: "long",
});

function actorLabel(e: AuditEvent): string {
  const actor =
    e.actor === "USER"
      ? "Sen"
      : e.actor === "SCHEDULER"
        ? "Zamanlayıcı"
        : e.actor === "DAEMON"
          ? "Servis"
          : e.actor;
  const origin =
    e.origin === "LOCAL_UI" ? "" : e.origin === "CLI" ? " (terminal)" : ` (${e.origin})`;
  return actor + origin;
}

function groupByDay<T>(items: T[], at: (t: T) => string): [string, T[]][] {
  const map = new Map<string, T[]>();
  for (const item of items) {
    const key = dayFmt.format(new Date(at(item)));
    const list = map.get(key) ?? [];
    list.push(item);
    map.set(key, list);
  }
  return [...map.entries()];
}

function evidenceIcon(e: Evidence): string {
  switch (e.type) {
    case "GIT_COMMIT":
      return "⌥";
    case "FILE_CHANGE":
      return "±";
    case "AI_SESSION":
      return "✦";
    default:
      return "·";
  }
}

export function ActivityScreen() {
  const [tab, setTab] = useState<"work" | "system">("work");
  const evidence = useEvidence(undefined, 200);
  const audit = useAudit(300);
  const verify = useVerifyAudit();
  const [report, setReport] = useState<AuditVerifyReport | null>(null);

  const evidenceByDay = useMemo(
    () => groupByDay(evidence.data ?? [], (e) => e.timestamp),
    [evidence.data],
  );
  const auditByDay = useMemo(() => groupByDay(audit.data ?? [], (e) => e.timestamp), [audit.data]);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Aktivite</h1>
          <div className="page-sub">
            {tab === "work"
              ? "çalışma zaman çizgisi — commit'ler, dosya hareketleri, oturumlar"
              : "tamper-evident işlem günlüğü (hash-chain)"}
          </div>
        </div>
        <div className="page-tools">
          <div className="seg">
            <button
              className={`seg-item${tab === "work" ? " active" : ""}`}
              onClick={() => setTab("work")}
            >
              Çalışma
            </button>
            <button
              className={`seg-item${tab === "system" ? " active" : ""}`}
              onClick={() => setTab("system")}
            >
              Sistem
            </button>
          </div>
          {tab === "system" && (
            <button
              className="btn"
              disabled={verify.isPending}
              onClick={() => verify.mutate(undefined, { onSuccess: setReport })}
            >
              <IconShield size={13} /> Zinciri doğrula
            </button>
          )}
        </div>
      </header>

      {tab === "system" && report && (
        <div className={`banner ${report.ok ? "banner-ok" : "banner-err"}`}>
          {report.ok ? "✓ " : "✗ "}
          {report.message}
        </div>
      )}

      {tab === "work" ? (
        evidenceByDay.length === 0 ? (
          <EmptyState
            icon={<IconActivity size={22} />}
            title="Henüz gözlem yok"
            hint="Projelerine yerel klasör bağladığında commit'ler ve dosya hareketleri burada birikir."
          />
        ) : (
          evidenceByDay.map(([day, list]) => (
            <section key={day} className="section">
              <div className="section-head">
                <h2>{day}</h2>
                <span className="section-hint">{list.length} gözlem</span>
              </div>
              <div className="audit-list">
                {list.map((e) => (
                  <div key={e.id} className="audit-row">
                    <span className="tl-time">{fmtTime(e.timestamp)}</span>
                    <span className="ev-glyph">{evidenceIcon(e)}</span>
                    <span className="audit-action ev-summary">{e.summary}</span>
                    {e.projectName && <span className="chip">{e.projectName}</span>}
                    <span className="chip chip-quiet">{EVIDENCE_LABEL[e.type]}</span>
                  </div>
                ))}
              </div>
            </section>
          ))
        )
      ) : auditByDay.length === 0 ? (
        <EmptyState
          icon={<IconActivity size={22} />}
          title="Henüz kayıt yok"
          hint="Görev, hatırlatma ve servis işlemleri burada zaman çizgisi olarak birikir."
        />
      ) : (
        auditByDay.map(([day, list]) => (
          <section key={day} className="section">
            <div className="section-head">
              <h2>{day}</h2>
              <span className="section-hint">{list.length} işlem</span>
            </div>
            <div className="audit-list">
              {list.map((e) => (
                <div key={e.id} className="audit-row">
                  <span className="tl-time">{fmtTime(e.timestamp)}</span>
                  <span className={`audit-result ${e.result === "OK" ? "ok" : "err"}`} />
                  <span className="audit-action">{AUDIT_ACTION_LABEL[e.action] ?? e.action}</span>
                  {e.target && <code className="audit-target">{e.target}</code>}
                  <span className="audit-actor">{actorLabel(e)}</span>
                  <span className="chip chip-quiet">{e.riskLevel}</span>
                </div>
              ))}
            </div>
          </section>
        ))
      )}
    </div>
  );
}
