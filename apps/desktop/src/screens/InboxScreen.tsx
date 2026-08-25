import { EmptyState } from "../components/EmptyState";
import { IconInbox } from "../components/Icons";
import { TaskComposer } from "../components/TaskComposer";
import { SOURCE_LABEL, fmtRelativeDay } from "../lib/format";
import { useTasks, useUpdateTask, useArchiveTask } from "../lib/queries";
import type { Task } from "../lib/types";

/** Inbox: her yerden düşenler; hızlı triage aksiyonlarıyla. */
export function InboxScreen({ onOpenTask }: { onOpenTask: (t: Task) => void }) {
  const tasks = useTasks({ statuses: ["INBOX"] });
  const update = useUpdateTask();
  const archive = useArchiveTask();

  const items = tasks.data ?? [];

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Gelen Kutusu</h1>
          <div className="page-sub">telefondan, asistandan ve hızlı nottan düşenler</div>
        </div>
      </header>

      <TaskComposer status="INBOX" placeholder="Aklındakini bırak — sonra düzenlersin…" />

      {items.length === 0 ? (
        <EmptyState
          icon={<IconInbox size={22} />}
          title="Gelen kutusu boş"
          hint="Güzel. Yeni gelenler burada birikir; buradan projeye ya da plana taşırsın."
        />
      ) : (
        <div className="list">
          {items.map((t) => (
            <div key={t.id} className="inbox-row" onClick={() => onOpenTask(t)}>
              <div className="inbox-main">
                <span className="task-title">{t.title}</span>
                <span className="inbox-meta">
                  {SOURCE_LABEL[t.source]} · {fmtRelativeDay(t.createdAt)}
                </span>
              </div>
              <div className="inbox-actions" onClick={(e) => e.stopPropagation()}>
                <button
                  className="btn btn-small"
                  title="Sıradaki işlere al"
                  onClick={() => update.mutate({ id: t.id, patch: { status: "NEXT" } })}
                >
                  Sıradaki
                </button>
                <button
                  className="btn btn-small"
                  title="Bugüne planla"
                  onClick={() =>
                    update.mutate({
                      id: t.id,
                      patch: { status: "PLANNED", scheduledAt: new Date().toISOString() },
                    })
                  }
                >
                  Bugüne
                </button>
                <button
                  className="btn btn-small"
                  title="Bir gün / belki"
                  onClick={() => update.mutate({ id: t.id, patch: { status: "SOMEDAY" } })}
                >
                  Bir gün
                </button>
                <button
                  className="btn btn-small btn-quiet"
                  title="Arşivle"
                  onClick={() => archive.mutate(t.id)}
                >
                  Arşivle
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
