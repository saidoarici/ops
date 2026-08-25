import type { ProjectHealth, TaskStatus } from "../lib/types";
import { HEALTH_LABEL, STATUS_LABEL } from "../lib/format";

const STATUS_CLASS: Record<TaskStatus, string> = {
  INBOX: "st-inbox",
  PLANNED: "st-planned",
  NEXT: "st-next",
  IN_PROGRESS: "st-progress",
  WAITING: "st-waiting",
  BLOCKED: "st-blocked",
  SOMEDAY: "st-someday",
  DONE: "st-done",
  CANCELLED: "st-cancelled",
};

export function StatusBadge({ status }: { status: TaskStatus }) {
  return (
    <span className={`badge ${STATUS_CLASS[status]}`}>
      <span className="badge-dot" />
      {STATUS_LABEL[status]}
    </span>
  );
}

const HEALTH_CLASS: Record<ProjectHealth, string> = {
  ACTIVE: "st-progress",
  QUIET: "st-someday",
  STALE: "st-waiting",
  BLOCKED: "st-blocked",
  WAITING: "st-waiting",
  AT_RISK: "st-blocked",
  COMPLETED: "st-done",
};

export function HealthBadge({ health }: { health: ProjectHealth }) {
  return (
    <span className={`badge ${HEALTH_CLASS[health]}`}>
      <span className="badge-dot" />
      {HEALTH_LABEL[health]}
    </span>
  );
}

/** P1..P5 öncelik işareti (5 en yüksek). */
export function PriorityFlag({ value }: { value: number }) {
  if (value <= 3) return null;
  return <span className={`prio ${value === 5 ? "prio-high" : "prio-med"}`}>P{value}</span>;
}
