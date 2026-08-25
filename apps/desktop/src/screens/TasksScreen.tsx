import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { IconTasks } from "../components/Icons";
import { TaskComposer } from "../components/TaskComposer";
import { TaskRow } from "../components/TaskRow";
import { STATUS_LABEL, STATUS_ORDER } from "../lib/format";
import { useTasks } from "../lib/queries";
import type { Task, TaskStatus } from "../lib/types";

type Scope = "open" | "all" | "done";

const EMPTY: Task[] = [];

const OPEN_STATUSES: TaskStatus[] = [
  "IN_PROGRESS",
  "NEXT",
  "PLANNED",
  "INBOX",
  "WAITING",
  "BLOCKED",
  "SOMEDAY",
];

export function TasksScreen({ onOpenTask }: { onOpenTask: (t: Task) => void }) {
  const [scope, setScope] = useState<Scope>("open");
  const [search, setSearch] = useState("");

  const filter = useMemo(() => {
    const statuses =
      scope === "open"
        ? OPEN_STATUSES
        : scope === "done"
          ? (["DONE", "CANCELLED"] as TaskStatus[])
          : undefined;
    return { statuses, search: search.trim() || undefined, limit: 1000 };
  }, [scope, search]);

  const tasks = useTasks(filter);
  const items = tasks.data ?? EMPTY;

  const groups = useMemo(() => {
    const map = new Map<TaskStatus, Task[]>();
    for (const t of items) {
      const list = map.get(t.status) ?? [];
      list.push(t);
      map.set(t.status, list);
    }
    return STATUS_ORDER.filter((s) => map.has(s)).map((s) => ({ status: s, tasks: map.get(s)! }));
  }, [items]);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Görevler</h1>
          <div className="page-sub">{items.length} görev</div>
        </div>
        <div className="page-tools">
          <input
            className="search"
            placeholder="Ara…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <div className="seg">
            {(
              [
                ["open", "Açık"],
                ["done", "Biten"],
                ["all", "Tümü"],
              ] as [Scope, string][]
            ).map(([key, label]) => (
              <button
                key={key}
                className={`seg-item${scope === key ? " active" : ""}`}
                onClick={() => setScope(key)}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      </header>

      <TaskComposer status="NEXT" placeholder="Yeni görev — Enter ile ekle…" />

      {items.length === 0 ? (
        <EmptyState
          icon={<IconTasks size={22} />}
          title={search ? "Eşleşen görev yok" : "Henüz görev yok"}
          hint={search ? "Aramayı sadeleştirmeyi dene." : "Yukarıdan ilk görevini ekle."}
        />
      ) : (
        groups.map((g) => (
          <section key={g.status} className="section">
            <div className="section-head">
              <h2>{STATUS_LABEL[g.status]}</h2>
              <span className="section-hint">{g.tasks.length}</span>
            </div>
            <div className="list">
              {g.tasks.map((t) => (
                <TaskRow key={t.id} task={t} onOpen={onOpenTask} showStatus={false} />
              ))}
            </div>
          </section>
        ))
      )}
    </div>
  );
}
