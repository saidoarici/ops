import type { Route } from "../lib/navigation";
import { DetectedCard } from "../components/DetectedCard";
import { EmptyState } from "../components/EmptyState";
import {
  IconArrowRight,
  IconBell,
  IconCalendar,
  IconSparkle,
  IconToday,
} from "../components/Icons";
import { StatusBadge } from "../components/Badges";
import { ATTENTION_LABEL, fmtMinutes, fmtTime, greeting, todayLong } from "../lib/format";
import { useCompleteTask, useSettings, useToday } from "../lib/queries";
import type { Task, TodayView } from "../lib/types";

/** Deterministik brief cümlesi — kısa, gerçek, motivasyon metni yok. */
function briefText(v: TodayView): string {
  const parts: string[] = [];
  if (v.stats.overdue > 0) parts.push(`${v.stats.overdue} geciken iş var`);
  if (v.stats.dueToday > 0) parts.push(`bugün ${v.stats.dueToday} son tarih`);
  if (v.stats.waiting > 0) parts.push(`${v.stats.waiting} iş cevap bekliyor`);
  if (v.stats.inbox > 0) parts.push(`gelen kutusunda ${v.stats.inbox} öğe`);
  if (parts.length === 0) {
    return v.stats.openTasks === 0
      ? "Açık görev yok. Sakin bir gün."
      : "Acil bir şey yok; odak listen hazır.";
  }
  const s = parts.join(", ");
  return s.charAt(0).toUpperCase() + s.slice(1) + ".";
}

export function TodayScreen({
  onOpenTask,
  navigate,
}: {
  onOpenTask: (t: Task) => void;
  navigate: (r: Route) => void;
}) {
  const today = useToday();
  const settings = useSettings();
  const complete = useCompleteTask();

  const view = today.data;
  const name = settings.data?.display_name || undefined;

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>{greeting(name)}</h1>
          <div className="page-sub">{todayLong()}</div>
        </div>
        {view && view.focus.length > 0 && (
          <button
            className="btn btn-primary"
            title={view.focus[0].whyNow}
            onClick={() => onOpenTask(view.focus[0].task)}
          >
            <IconSparkle size={13} /> Şimdi ne yapayım?
          </button>
        )}
      </header>

      {view && (
        <>
          <p className="brief">{briefText(view)}</p>

          <section className="section">
            <div className="section-head">
              <h2>Odak</h2>
              <span className="section-hint">bugünün en önemli en fazla 3 işi</span>
            </div>
            {view.focus.length === 0 ? (
              <EmptyState
                icon={<IconToday size={22} />}
                title="Odak listesi boş"
                hint="Sıradaki veya planlı görev ekledikçe burada belirir."
              />
            ) : (
              <div className="focus-list">
                {view.focus.map((f, i) => (
                  <div key={f.task.id} className="focus-card" onClick={() => onOpenTask(f.task)}>
                    <span className="focus-rank">{i + 1}</span>
                    <div className="focus-main">
                      <div className="focus-title">{f.task.title}</div>
                      <div className="focus-why">
                        <IconSparkle size={12} />
                        {f.whyNow}
                      </div>
                    </div>
                    <div className="focus-meta">
                      {f.task.projectName && <span className="chip">{f.task.projectName}</span>}
                      {f.task.estimatedMinutes != null && (
                        <span className="chip chip-quiet">
                          {fmtMinutes(f.task.estimatedMinutes)}
                        </span>
                      )}
                      <StatusBadge status={f.task.status} />
                      <button
                        className="btn btn-small"
                        onClick={(e) => {
                          e.stopPropagation();
                          complete.mutate(f.task.id);
                        }}
                      >
                        Tamamla
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </section>

          {view.needsAttention.length > 0 && (
            <section className="section">
              <div className="section-head">
                <h2>Dikkat Gerekiyor</h2>
                <span className="section-hint">geciken · uzun bekleyen · durgun</span>
              </div>
              <div className="list">
                {view.needsAttention.map((a) => (
                  <div key={a.task.id} className="attn-row" onClick={() => onOpenTask(a.task)}>
                    <span className={`attn-kind attn-${a.kind.toLowerCase()}`}>
                      {ATTENTION_LABEL[a.kind]}
                    </span>
                    <div className="attn-main">
                      <span className="attn-title">{a.task.title}</span>
                      <span className="attn-detail">{a.detail}</span>
                    </div>
                    {a.task.projectName && <span className="chip">{a.task.projectName}</span>}
                    <IconArrowRight size={13} className="attn-go" />
                  </div>
                ))}
              </div>
            </section>
          )}

          {view.detected.length > 0 && (
            <section className="section">
              <div className="section-head">
                <h2>Tespit Edilenler</h2>
                <span className="section-hint">
                  observer'ın git bulguları — istersen göreve çevir
                </span>
              </div>
              <div className="focus-list">
                {view.detected.map((d) => (
                  <DetectedCard key={d.id} item={d} />
                ))}
              </div>
            </section>
          )}

          <section className="section">
            <div className="section-head">
              <h2>Bugünün Akışı</h2>
              <span className="section-hint">hatırlatmalar ve son tarihler</span>
            </div>
            {view.timeline.length === 0 ? (
              <EmptyState
                icon={<IconCalendar size={22} />}
                title="Bugün için zamanlanmış bir şey yok"
                hint="Hatırlatmalar ekranından yenisini kurabilirsin."
              />
            ) : (
              <div className="timeline">
                {view.timeline.map((item, i) => (
                  <div key={`${item.reminderId ?? item.taskId ?? i}-${item.at}`} className="tl-row">
                    <span className="tl-time">{fmtTime(item.at)}</span>
                    <span className={`tl-icon tl-${item.kind.toLowerCase()}`}>
                      {item.kind === "REMINDER" ? (
                        <IconBell size={12} />
                      ) : (
                        <IconCalendar size={12} />
                      )}
                    </span>
                    <span className="tl-title">{item.title}</span>
                    <span className="tl-kind">
                      {item.kind === "REMINDER"
                        ? item.status === "FIRED"
                          ? "hatırlatıldı"
                          : "hatırlatma"
                        : item.kind === "DUE"
                          ? "son tarih"
                          : "planlandı"}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>

          <footer className="today-stats">
            <button className="stat" onClick={() => navigate({ name: "tasks" })}>
              <b>{view.stats.openTasks}</b> açık görev
            </button>
            <button className="stat" onClick={() => navigate({ name: "inbox" })}>
              <b>{view.stats.inbox}</b> gelen
            </button>
            <button className="stat" onClick={() => navigate({ name: "waiting" })}>
              <b>{view.stats.waiting}</b> bekleyen
            </button>
            <span className="stat">
              <b>{view.stats.doneToday}</b> bugün bitti
            </span>
          </footer>
        </>
      )}
    </div>
  );
}
