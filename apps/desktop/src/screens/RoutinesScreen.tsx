import { useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { IconRepeat } from "../components/Icons";
import { fmtDayTime } from "../lib/format";
import { useRoutines, useRunRoutine, useUpdateRoutine } from "../lib/queries";
import type { Routine } from "../lib/types";

const ACTION_HINT: Record<Routine["actionType"], string> = {
  MORNING_BRIEF: "Bugünün odağı, bekleyenler ve dünden beri olan gözlemler.",
  EVENING_REVIEW: "Bugün bitenler, sürenler ve yarım görünen işler.",
  WEEKLY_REVIEW: "Proje sağlıkları ve haftanın özeti.",
};

function RoutineRow({ routine }: { routine: Routine }) {
  const update = useUpdateRoutine();
  const run = useRunRoutine();
  const [schedule, setSchedule] = useState(routine.schedule);
  const [preview, setPreview] = useState<string | null>(null);

  return (
    <div className="routine-card">
      <div className="routine-head">
        <label className="routine-toggle">
          <input
            type="checkbox"
            checked={routine.enabled}
            onChange={(e) =>
              update.mutate({ id: routine.id, patch: { enabled: e.target.checked } })
            }
          />
          <span className="routine-name">{routine.name}</span>
        </label>
        <div className="routine-tools">
          <input
            className="routine-schedule"
            value={schedule}
            onChange={(e) => setSchedule(e.target.value)}
            onBlur={() => {
              const s = schedule.trim();
              if (s && s !== routine.schedule) {
                update.mutate(
                  { id: routine.id, patch: { schedule: s } },
                  { onError: () => setSchedule(routine.schedule) },
                );
              }
            }}
            title='"HH:MM" ya da "MON HH:MM"'
          />
          <button
            className="btn btn-small"
            disabled={run.isPending}
            onClick={() => run.mutate(routine.id, { onSuccess: (r) => setPreview(r.text) })}
          >
            {run.isPending ? "Çalışıyor…" : "Şimdi çalıştır"}
          </button>
        </div>
      </div>
      <div className="routine-sub">
        {ACTION_HINT[routine.actionType]}
        {routine.lastRunAt && <> · son koşu {fmtDayTime(routine.lastRunAt)}</>}
        {routine.enabled && routine.nextRunAt && <> · sıradaki {fmtDayTime(routine.nextRunAt)}</>}
        {routine.lastResult?.channels && routine.lastResult.channels.length > 0 && (
          <> · kanal: {routine.lastResult.channels.join(", ")}</>
        )}
      </div>
      {update.isError && <p className="form-err">{update.error.message}</p>}
      {preview && <pre className="routine-preview">{preview}</pre>}
    </div>
  );
}

export function RoutinesScreen() {
  const routines = useRoutines();
  const items = routines.data ?? [];

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Rutinler</h1>
          <div className="page-sub">
            uygulama kapalıyken de servis çalıştırır; içerik deterministiktir, yalnızca bildirim
            gönderir
          </div>
        </div>
      </header>

      {items.length === 0 ? (
        <EmptyState
          icon={<IconRepeat size={22} />}
          title="Rutin bulunamadı"
          hint="Servis ilk açılışta yerleşik rutinleri oluşturur."
        />
      ) : (
        <div className="focus-list">
          {items.map((r) => (
            <RoutineRow key={r.id} routine={r} />
          ))}
        </div>
      )}
    </div>
  );
}
