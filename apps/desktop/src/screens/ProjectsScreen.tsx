import { useState } from "react";
import type { Route } from "../lib/navigation";
import { HealthBadge } from "../components/Badges";
import { Dialog } from "../components/Dialog";
import { EmptyState } from "../components/EmptyState";
import { IconPlus, IconProjects } from "../components/Icons";
import { fmtRelativeDay } from "../lib/format";
import { useCreateProject, useProjects } from "../lib/queries";

export function ProjectsScreen({ navigate }: { navigate: (r: Route) => void }) {
  const projects = useProjects();
  const create = useCreateProject();
  const [showNew, setShowNew] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [priority, setPriority] = useState(3);

  const items = projects.data ?? [];

  const submit = () => {
    const n = name.trim();
    if (!n || create.isPending) return;
    create.mutate(
      { name: n, description: description.trim() || undefined, priority },
      {
        onSuccess: () => {
          setShowNew(false);
          setName("");
          setDescription("");
          setPriority(3);
        },
      },
    );
  };

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Projeler</h1>
          <div className="page-sub">{items.length} aktif proje</div>
        </div>
        <button className="btn btn-primary" onClick={() => setShowNew(true)}>
          <IconPlus size={13} /> Yeni Proje
        </button>
      </header>

      {items.length === 0 ? (
        <EmptyState
          icon={<IconProjects size={22} />}
          title="Henüz proje yok"
          hint="Projeler; görevleri, klasörleri ve ileride git gözlemini bir araya toplar."
        />
      ) : (
        <div className="project-grid">
          {items.map((p) => (
            <div
              key={p.id}
              className="project-card"
              onClick={() => navigate({ name: "project", id: p.id })}
            >
              <div className="project-card-head">
                <span className="project-name">{p.name}</span>
                <HealthBadge health={p.health} />
              </div>
              {p.description && <div className="project-desc">{p.description}</div>}
              <div className="project-stats">
                <span>
                  <b>{p.openTasks}</b> açık
                </span>
                {p.waitingTasks > 0 && (
                  <span>
                    <b>{p.waitingTasks}</b> bekleyen
                  </span>
                )}
                {p.inboxTasks > 0 && (
                  <span>
                    <b>{p.inboxTasks}</b> gelen
                  </span>
                )}
                <span className="project-activity">
                  {p.lastTaskActivity
                    ? `son aktivite ${fmtRelativeDay(p.lastTaskActivity)}`
                    : "aktivite yok"}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}

      {showNew && (
        <Dialog
          title="Yeni Proje"
          onClose={() => setShowNew(false)}
          footer={
            <>
              <button className="btn" onClick={() => setShowNew(false)}>
                Vazgeç
              </button>
              <button className="btn btn-primary" disabled={!name.trim()} onClick={submit}>
                Oluştur
              </button>
            </>
          }
        >
          <label className="field field-wide">
            <span>Ad</span>
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && submit()}
              placeholder="ör. Atlas CRM"
            />
          </label>
          <label className="field field-wide">
            <span>Açıklama</span>
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="kısa tanım (opsiyonel)"
            />
          </label>
          <label className="field">
            <span>Öncelik</span>
            <select value={priority} onChange={(e) => setPriority(Number(e.target.value))}>
              {[1, 2, 3, 4, 5].map((n) => (
                <option key={n} value={n}>
                  P{n}
                </option>
              ))}
            </select>
          </label>
          {create.isError && <p className="form-err">{create.error.message}</p>}
        </Dialog>
      )}
    </div>
  );
}
