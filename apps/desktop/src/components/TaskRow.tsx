import type { Task } from "../lib/types";
import { fmtRelativeDay, fmtMinutes } from "../lib/format";
import { useCompleteTask, useUpdateTask } from "../lib/queries";
import { StatusBadge, PriorityFlag } from "./Badges";

export function TaskRow({
  task,
  onOpen,
  showProject = true,
  showStatus = true,
}: {
  task: Task;
  onOpen: (task: Task) => void;
  showProject?: boolean;
  showStatus?: boolean;
}) {
  const complete = useCompleteTask();
  const update = useUpdateTask();
  const done = task.status === "DONE";
  const overdue = !done && task.dueAt !== null && new Date(task.dueAt).getTime() < Date.now();

  const toggle = () => {
    if (done) {
      update.mutate({ id: task.id, patch: { status: "NEXT" } });
    } else {
      complete.mutate(task.id);
    }
  };

  return (
    <div className={`task-row${done ? " is-done" : ""}`} onClick={() => onOpen(task)}>
      <button
        className={`check${done ? " checked" : ""}`}
        onClick={(e) => {
          e.stopPropagation();
          toggle();
        }}
        aria-label={done ? "Yeniden aç" : "Tamamla"}
      >
        {done && (
          <svg
            width="9"
            height="9"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="3.4"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M20 6L9 17l-5-5" />
          </svg>
        )}
      </button>
      <div className="task-main">
        <div className="task-title-line">
          <span className="task-title">{task.title}</span>
          <PriorityFlag value={task.priority} />
        </div>
        {(task.waitingFor || task.description) && (
          <div className="task-sub">{task.waitingFor ?? task.description}</div>
        )}
      </div>
      <div className="task-meta">
        {task.estimatedMinutes != null && (
          <span className="chip chip-quiet">{fmtMinutes(task.estimatedMinutes)}</span>
        )}
        {showProject && task.projectName && <span className="chip">{task.projectName}</span>}
        {task.dueAt && (
          <span className={`chip ${overdue ? "chip-danger" : "chip-due"}`}>
            {fmtRelativeDay(task.dueAt)}
          </span>
        )}
        {showStatus && <StatusBadge status={task.status} />}
      </div>
    </div>
  );
}
