import { useState } from "react";
import type { Route } from "../lib/navigation";
import { HealthBadge } from "../components/Badges";
import { DetectedCard } from "../components/DetectedCard";
import { EmptyState } from "../components/EmptyState";
import { IconProjects } from "../components/Icons";
import { TaskComposer } from "../components/TaskComposer";
import { TaskRow } from "../components/TaskRow";
import { EVIDENCE_LABEL, fmtDayTime, fmtRelativeDay } from "../lib/format";
import { useProjectOverview, useTasks, useUpdateProject } from "../lib/queries";
import type { Project, RepoState, Task } from "../lib/types";

function RepoCard({ repo }: { repo: RepoState }) {
  return (
    <div className="repo-card">
      <div className="repo-head">
        <code className="repo-path">{repo.repoPath}</code>
        {repo.branch && <span className="chip">{repo.branch}</span>}
      </div>
      <div className="repo-facts">
        <span className={repo.dirtyFiles > 0 ? "text-warn" : "text-ok"}>
          {repo.dirtyFiles > 0
            ? `${repo.dirtyFiles} dosya commit bekliyor${
                repo.dirtySince ? ` (${fmtRelativeDay(repo.dirtySince)}'den beri)` : ""
              }`
            : "çalışma kopyası temiz"}
        </span>
        {repo.ahead > 0 && <span className="text-warn">{repo.ahead} commit push'lanmamış</span>}
        <span className="repo-quiet">
          {repo.lastCommitAt
            ? `son commit ${fmtRelativeDay(repo.lastCommitAt)}`
            : "henüz commit yok"}
          {" · "}tarama {fmtDayTime(repo.lastScanAt)}
        </span>
      </div>
    </div>
  );
}

export function ProjectScreen({
  id,
  onOpenTask,
  navigate,
}: {
  id: string;
  onOpenTask: (t: Task) => void;
  navigate: (r: Route) => void;
}) {
  const overview = useProjectOverview(id);
  const tasks = useTasks({ projectId: id, limit: 500 });
  const update = useUpdateProject();
  const [newPath, setNewPath] = useState("");

  const p = overview.data?.project;
  const items = tasks.data ?? [];
  const open = items.filter((t) => t.status !== "DONE" && t.status !== "CANCELLED");
  const done = items.filter((t) => t.status === "DONE" || t.status === "CANCELLED");

  if (!p) {
    return <div className="page" />;
  }
  const detected = overview.data?.detected ?? [];
  const evidence = overview.data?.evidence ?? [];
  const repoStates = overview.data?.repoStates ?? [];

  const addPath = () => {
    const path = newPath.trim();
    if (!path) return;
    update.mutate(
      { id, patch: { localPaths: [...p.localPaths, path] } },
      { onSuccess: () => setNewPath("") },
    );
  };

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <button className="crumb" onClick={() => navigate({ name: "projects" })}>
            Projeler
          </button>
          <h1>{p.name}</h1>
          <div className="page-sub project-head-sub">
            <HealthBadge health={p.health} />
            <span>P{p.priority}</span>
            {p.description && <span>{p.description}</span>}
          </div>
        </div>
        <select
          className="select-plain"
          value={p.state}
          onChange={(e) =>
            update.mutate({ id, patch: { state: e.target.value as Project["state"] } })
          }
          title="Proje durumu"
        >
          <option value="ACTIVE">Aktif</option>
          <option value="PAUSED">Duraklatıldı</option>
          <option value="COMPLETED">Tamamlandı</option>
          <option value="ARCHIVED">Arşiv</option>
        </select>
      </header>

      {repoStates.length > 0 && (
        <section className="section">
          <div className="section-head">
            <h2>Git Durumu</h2>
          </div>
          <div className="focus-list">
            {repoStates.map((r) => (
              <RepoCard key={r.repoPath} repo={r} />
            ))}
          </div>
        </section>
      )}

      {detected.length > 0 && (
        <section className="section">
          <div className="section-head">
            <h2>Tespitler</h2>
            <span className="section-hint">yarım kalmış görünen işler</span>
          </div>
          <div className="focus-list">
            {detected.map((d) => (
              <DetectedCard key={d.id} item={d} />
            ))}
          </div>
        </section>
      )}

      <TaskComposer status="NEXT" projectId={id} placeholder={`${p.name} için görev ekle…`} />

      <section className="section">
        <div className="section-head">
          <h2>Açık İşler</h2>
          <span className="section-hint">{open.length}</span>
        </div>
        {open.length === 0 ? (
          <EmptyState
            icon={<IconProjects size={22} />}
            title="Bu projede açık iş yok"
            hint="Yukarıdan görev ekleyebilirsin."
          />
        ) : (
          <div className="list">
            {open.map((t) => (
              <TaskRow key={t.id} task={t} onOpen={onOpenTask} showProject={false} />
            ))}
          </div>
        )}
      </section>

      {evidence.length > 0 && (
        <section className="section">
          <div className="section-head">
            <h2>Son Aktivite</h2>
            <span className="section-hint">observer gözlemleri</span>
          </div>
          <div className="audit-list">
            {evidence.slice(0, 12).map((e) => (
              <div key={e.id} className="audit-row">
                <span className="tl-time">{fmtDayTime(e.timestamp)}</span>
                <span className="audit-action ev-summary">{e.summary}</span>
                <span className="chip chip-quiet">{EVIDENCE_LABEL[e.type]}</span>
              </div>
            ))}
          </div>
        </section>
      )}

      {done.length > 0 && (
        <section className="section">
          <div className="section-head">
            <h2>Bitenler</h2>
            <span className="section-hint">{done.length}</span>
          </div>
          <div className="list">
            {done.map((t) => (
              <TaskRow key={t.id} task={t} onOpen={onOpenTask} showProject={false} />
            ))}
          </div>
        </section>
      )}

      <section className="section">
        <div className="section-head">
          <h2>Proje Klasörleri</h2>
          <span className="section-hint">gözlemci yalnızca bu onaylı yollara bakar</span>
        </div>
        {p.localPaths.length > 0 && (
          <div className="path-list">
            {p.localPaths.map((path) => (
              <div key={path} className="path-row">
                <code>{path}</code>
                <button
                  className="btn btn-small btn-quiet"
                  onClick={() =>
                    update.mutate({
                      id,
                      patch: { localPaths: p.localPaths.filter((x) => x !== path) },
                    })
                  }
                >
                  Kaldır
                </button>
              </div>
            ))}
          </div>
        )}
        <div className="composer">
          <input
            className="composer-input"
            placeholder="/Users/sen/Projects/…"
            value={newPath}
            onChange={(e) => setNewPath(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addPath()}
          />
          <button className="btn btn-small" onClick={addPath} disabled={!newPath.trim()}>
            Ekle
          </button>
        </div>
      </section>
    </div>
  );
}
