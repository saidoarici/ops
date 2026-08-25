import { useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { IconBell } from "../components/Icons";
import { REPEAT_LABEL, fmtDayTime, isoToLocalInput, localInputToIso } from "../lib/format";
import { useCreateReminder, useDismissReminder, useReminders } from "../lib/queries";
import type { Reminder, RepeatRule } from "../lib/types";

function defaultRemindAt(): string {
  const d = new Date(Date.now() + 60 * 60 * 1000);
  d.setMinutes(0, 0, 0);
  return isoToLocalInput(d.toISOString());
}

export function RemindersScreen() {
  const reminders = useReminders();
  const create = useCreateReminder();
  const dismiss = useDismissReminder();

  const [title, setTitle] = useState("");
  const [when, setWhen] = useState(defaultRemindAt);
  const [repeat, setRepeat] = useState<RepeatRule>("NONE");

  const items = reminders.data ?? [];
  const upcoming = items.filter((r) => r.status === "SCHEDULED");
  const past = items
    .filter((r) => r.status !== "SCHEDULED")
    .sort((a, b) => (b.firedAt ?? b.remindAt).localeCompare(a.firedAt ?? a.remindAt))
    .slice(0, 20);

  const submit = () => {
    const t = title.trim();
    if (!t || !when || create.isPending) return;
    create.mutate(
      { title: t, remindAt: localInputToIso(when), repeatRule: repeat },
      {
        onSuccess: () => {
          setTitle("");
          setWhen(defaultRemindAt());
          setRepeat("NONE");
        },
      },
    );
  };

  const statusText = (r: Reminder) =>
    r.status === "FIRED"
      ? "hatırlatıldı"
      : r.status === "MISSED"
        ? "kaçırıldı"
        : r.status === "DISMISSED"
          ? "kapatıldı"
          : "iptal";

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Hatırlatmalar</h1>
          <div className="page-sub">uygulama kapalıyken de servis tetikler</div>
        </div>
      </header>

      <div className="reminder-form">
        <input
          className="composer-input"
          placeholder="Neyi hatırlatayım?"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
        />
        <input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} />
        <select value={repeat} onChange={(e) => setRepeat(e.target.value as RepeatRule)}>
          {(Object.keys(REPEAT_LABEL) as RepeatRule[]).map((r) => (
            <option key={r} value={r}>
              {REPEAT_LABEL[r]}
            </option>
          ))}
        </select>
        <button className="btn btn-primary" onClick={submit} disabled={!title.trim()}>
          Kur
        </button>
      </div>
      {create.isError && <p className="form-err">{create.error.message}</p>}

      <section className="section">
        <div className="section-head">
          <h2>Yaklaşanlar</h2>
          <span className="section-hint">{upcoming.length}</span>
        </div>
        {upcoming.length === 0 ? (
          <EmptyState
            icon={<IconBell size={22} />}
            title="Kurulu hatırlatma yok"
            hint="Yukarıdan ilk hatırlatmayı kur; macOS bildirimi olarak gelir."
          />
        ) : (
          <div className="list">
            {upcoming.map((r) => (
              <div key={r.id} className="rem-row">
                <IconBell size={13} className="rem-icon" />
                <div className="rem-main">
                  <span className="task-title">{r.title}</span>
                  {r.notes && <span className="task-sub">{r.notes}</span>}
                </div>
                <span className="chip chip-due">{fmtDayTime(r.remindAt)}</span>
                {r.repeatRule !== "NONE" && (
                  <span className="chip chip-quiet">{REPEAT_LABEL[r.repeatRule]}</span>
                )}
                <button className="btn btn-small btn-quiet" onClick={() => dismiss.mutate(r.id)}>
                  İptal
                </button>
              </div>
            ))}
          </div>
        )}
      </section>

      {past.length > 0 && (
        <section className="section">
          <div className="section-head">
            <h2>Geçmiş</h2>
          </div>
          <div className="list">
            {past.map((r) => (
              <div key={r.id} className="rem-row is-past">
                <IconBell size={13} className="rem-icon" />
                <div className="rem-main">
                  <span className="task-title">{r.title}</span>
                </div>
                <span className="chip chip-quiet">{statusText(r)}</span>
                <span className="rem-time">{fmtDayTime(r.firedAt ?? r.remindAt)}</span>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
