import { EmptyState } from "../components/EmptyState";
import { IconWaiting } from "../components/Icons";
import { daysSince, fmtRelativeDay } from "../lib/format";
import { useCompleteTask, useTasks, useUpdateTask } from "../lib/queries";
import type { Task } from "../lib/types";

/** Bekleyenler: kimden ne bekleniyor, kaç gündür, takip zamanı geldi mi. */
export function WaitingScreen({ onOpenTask }: { onOpenTask: (t: Task) => void }) {
  const tasks = useTasks({ statuses: ["WAITING"] });
  const update = useUpdateTask();
  const complete = useCompleteTask();

  const items = [...(tasks.data ?? [])].sort(
    (a, b) => daysSince(b.waitingSince ?? b.createdAt) - daysSince(a.waitingSince ?? a.createdAt),
  );

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Bekleyenler</h1>
          <div className="page-sub">senin değil, başkasının elindeki işler</div>
        </div>
      </header>

      {items.length === 0 ? (
        <EmptyState
          icon={<IconWaiting size={22} />}
          title="Cevap beklediğin iş yok"
          hint='Bir görevi "Bekliyor" durumuna alınca burada süresiyle izlenir.'
        />
      ) : (
        <div className="list">
          {items.map((t) => {
            const days = daysSince(t.waitingSince ?? t.createdAt);
            const followupDue =
              t.followupAt !== null && new Date(t.followupAt).getTime() <= Date.now();
            return (
              <div key={t.id} className="wait-row" onClick={() => onOpenTask(t)}>
                <div className="wait-main">
                  <span className="task-title">{t.title}</span>
                  <span className="wait-sub">
                    {t.waitingFor && <>{t.waitingFor} · </>}
                    <b className={days >= 7 ? "text-danger" : days >= 4 ? "text-warn" : ""}>
                      {days === 0 ? "bugün soruldu" : `${days} gündür bekliyor`}
                    </b>
                    {t.followupAt && (
                      <>
                        {" · takip: "}
                        <span className={followupDue ? "text-warn" : ""}>
                          {fmtRelativeDay(t.followupAt)}
                        </span>
                      </>
                    )}
                  </span>
                </div>
                <div className="inbox-actions" onClick={(e) => e.stopPropagation()}>
                  {t.projectName && <span className="chip">{t.projectName}</span>}
                  <button
                    className="btn btn-small"
                    title="Takibi bugüne kur"
                    onClick={() =>
                      update.mutate({
                        id: t.id,
                        patch: { followupAt: new Date().toISOString() },
                      })
                    }
                  >
                    Bugün takip et
                  </button>
                  <button
                    className="btn btn-small"
                    title="Cevap geldi, iş bende"
                    onClick={() => update.mutate({ id: t.id, patch: { status: "NEXT" } })}
                  >
                    Devral
                  </button>
                  <button className="btn btn-small" onClick={() => complete.mutate(t.id)}>
                    Bitti
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
