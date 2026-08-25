import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { Task, TaskPatch, TaskStatus } from "../lib/types";
import {
  STATUS_LABEL,
  STATUS_ORDER,
  SOURCE_LABEL,
  fmtDayTime,
  isoToLocalInput,
  localInputToIso,
} from "../lib/format";
import { ops } from "../lib/ipc";
import { useArchiveTask, useProjects, useUpdateTask } from "../lib/queries";
import { IconArchive, IconX } from "./Icons";

/** Sağdan açılan görev detayı; alanlar değiştikçe anında kaydedilir. */
export function TaskDetailPanel({ task: initial, onClose }: { task: Task; onClose: () => void }) {
  const update = useUpdateTask();
  const archive = useArchiveTask();
  const projects = useProjects();

  // Panel açıkken görevin canlı hali izlenir; mutasyon sonrası alanlar tazelenir.
  const live = useQuery<Task>({
    queryKey: ["task", initial.id],
    queryFn: () => ops<Task>("task.get", { id: initial.id }),
    initialData: initial,
    refetchInterval: 5000,
  });
  const task = live.data ?? initial;

  const [title, setTitle] = useState(initial.title);
  const [description, setDescription] = useState(initial.description);
  const [waitingFor, setWaitingFor] = useState(initial.waitingFor ?? "");

  const apply = (patch: TaskPatch) => update.mutate({ id: task.id, patch });

  const num = (v: string) => Number.parseInt(v, 10);

  return (
    <aside className="panel" onClick={(e) => e.stopPropagation()}>
      <div className="panel-head">
        <span className="panel-source">{SOURCE_LABEL[task.source]}</span>
        <div className="panel-actions">
          <button
            className="icon-btn"
            title="Arşivle"
            onClick={() => {
              archive.mutate(task.id);
              onClose();
            }}
          >
            <IconArchive size={14} />
          </button>
          <button className="icon-btn" onClick={onClose} aria-label="Kapat">
            <IconX size={14} />
          </button>
        </div>
      </div>

      <input
        className="panel-title"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onBlur={() => {
          const t = title.trim();
          if (t && t !== task.title) apply({ title: t });
        }}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />

      <textarea
        className="panel-desc"
        placeholder="Açıklama…"
        value={description}
        rows={3}
        onChange={(e) => setDescription(e.target.value)}
        onBlur={() => {
          if (description !== task.description) apply({ description });
        }}
      />

      <div className="field-grid">
        <label className="field">
          <span>Durum</span>
          <select
            value={task.status}
            onChange={(e) => apply({ status: e.target.value as TaskStatus })}
          >
            {STATUS_ORDER.map((s) => (
              <option key={s} value={s}>
                {STATUS_LABEL[s]}
              </option>
            ))}
          </select>
        </label>

        <label className="field">
          <span>Proje</span>
          <select
            value={task.projectId ?? ""}
            onChange={(e) => apply({ projectId: e.target.value === "" ? null : e.target.value })}
          >
            <option value="">—</option>
            {(projects.data ?? []).map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>

        <label className="field">
          <span>Öncelik</span>
          <select value={task.priority} onChange={(e) => apply({ priority: num(e.target.value) })}>
            {[1, 2, 3, 4, 5].map((n) => (
              <option key={n} value={n}>
                P{n}
              </option>
            ))}
          </select>
        </label>

        <label className="field">
          <span>Önem / Aciliyet</span>
          <div className="field-pair">
            <select
              value={task.importance}
              onChange={(e) => apply({ importance: num(e.target.value) })}
              title="Önem"
            >
              {[1, 2, 3, 4, 5].map((n) => (
                <option key={n} value={n}>
                  Ö{n}
                </option>
              ))}
            </select>
            <select
              value={task.urgency}
              onChange={(e) => apply({ urgency: num(e.target.value) })}
              title="Aciliyet"
            >
              {[1, 2, 3, 4, 5].map((n) => (
                <option key={n} value={n}>
                  A{n}
                </option>
              ))}
            </select>
          </div>
        </label>

        <label className="field">
          <span>Son tarih</span>
          <div className="field-pair">
            <input
              type="datetime-local"
              value={task.dueAt ? isoToLocalInput(task.dueAt) : ""}
              onChange={(e) =>
                apply({ dueAt: e.target.value ? localInputToIso(e.target.value) : null })
              }
            />
          </div>
        </label>

        <label className="field">
          <span>Planlanan</span>
          <input
            type="datetime-local"
            value={task.scheduledAt ? isoToLocalInput(task.scheduledAt) : ""}
            onChange={(e) =>
              apply({ scheduledAt: e.target.value ? localInputToIso(e.target.value) : null })
            }
          />
        </label>

        <label className="field">
          <span>Tahmini süre (dk)</span>
          <input
            type="number"
            min={0}
            step={5}
            value={task.estimatedMinutes ?? ""}
            onChange={(e) =>
              apply({
                estimatedMinutes: e.target.value === "" ? null : num(e.target.value),
              })
            }
          />
        </label>

        {task.status === "WAITING" && (
          <>
            <label className="field field-wide">
              <span>Kimden / ne bekleniyor</span>
              <input
                value={waitingFor}
                placeholder="ör. Hukuk ekibi — sözleşme taslağı"
                onChange={(e) => setWaitingFor(e.target.value)}
                onBlur={() => {
                  const w = waitingFor.trim();
                  if (w !== (task.waitingFor ?? "")) apply({ waitingFor: w === "" ? null : w });
                }}
              />
            </label>
            <label className="field">
              <span>Takip tarihi</span>
              <input
                type="datetime-local"
                value={task.followupAt ? isoToLocalInput(task.followupAt) : ""}
                onChange={(e) =>
                  apply({ followupAt: e.target.value ? localInputToIso(e.target.value) : null })
                }
              />
            </label>
          </>
        )}
      </div>

      <div className="panel-foot">
        <span>Oluşturuldu: {fmtDayTime(task.createdAt)}</span>
        <span>Güncellendi: {fmtDayTime(task.updatedAt)}</span>
        {task.completedAt && <span>Tamamlandı: {fmtDayTime(task.completedAt)}</span>}
      </div>
    </aside>
  );
}
