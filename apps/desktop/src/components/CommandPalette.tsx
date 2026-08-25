import { useEffect, useRef, useState } from "react";
import type { Route } from "../lib/navigation";
import type { Task } from "../lib/types";
import { STATUS_LABEL } from "../lib/format";
import { useCreateTask, useProjects, useRunBackup, useRunScan, useTasks } from "../lib/queries";
import { IconArrowRight, IconPlus, IconSparkle } from "./Icons";

export type PaletteMode = "command" | "capture";

interface Item {
  id: string;
  label: string;
  hint?: string;
  kind: "nav" | "action" | "task" | "project" | "create";
  run: () => void;
}

export function CommandPalette({
  mode,
  onClose,
  navigate,
  onOpenTask,
}: {
  mode: PaletteMode;
  onClose: () => void;
  navigate: (r: Route) => void;
  onOpenTask: (t: Task) => void;
}) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const createTask = useCreateTask();
  const backup = useRunBackup();
  const scan = useRunScan();
  const projects = useProjects();
  const tasks = useTasks(
    { search: query.trim() || undefined, limit: 8 },
    mode === "command" && query.trim().length > 1,
  );

  useEffect(() => {
    inputRef.current?.focus();
  }, []);
  useEffect(() => setIndex(0), [query]);

  const q = query.trim().toLowerCase();

  // Liste küçüktür (en fazla ~20 öğe); her render'da yeniden kurmak ucuzdur ve
  // bayat closure riskini ortadan kaldırır.
  const items: Item[] = (() => {
    const done = () => onClose();
    if (mode === "capture") {
      const title = query.trim();
      return title === ""
        ? []
        : [
            {
              id: "capture",
              label: `Görev oluştur: "${title}"`,
              hint: "Gelen kutusuna",
              kind: "create",
              run: () => {
                createTask.mutate({ title, status: "INBOX", source: "QUICK_CAPTURE" });
                done();
              },
            },
          ];
    }

    const nav: Item[] = (
      [
        ["Bugün", { name: "today" }],
        ["Gelen Kutusu", { name: "inbox" }],
        ["Görevler", { name: "tasks" }],
        ["Bekleyenler", { name: "waiting" }],
        ["Projeler", { name: "projects" }],
        ["Asistan", { name: "assistant" }],
        ["Rutinler", { name: "routines" }],
        ["Hatırlatmalar", { name: "reminders" }],
        ["Aktivite", { name: "activity" }],
        ["Güvenlik", { name: "security" }],
        ["Ayarlar", { name: "settings" }],
      ] as [string, Route][]
    ).map(([label, route]) => ({
      id: `nav-${label}`,
      label,
      hint: "Git",
      kind: "nav" as const,
      run: () => {
        navigate(route);
        done();
      },
    }));

    const actions: Item[] = [
      {
        id: "act-backup",
        label: "Yedek al",
        hint: "SQLite yedeği",
        kind: "action",
        run: () => {
          backup.mutate(undefined);
          done();
        },
      },
      {
        id: "act-scan",
        label: "Projeleri şimdi tara",
        hint: "Observer",
        kind: "action",
        run: () => {
          scan.mutate(undefined);
          done();
        },
      },
    ];

    const projectItems: Item[] = (projects.data ?? []).map((p) => ({
      id: `proj-${p.id}`,
      label: p.name,
      hint: "Proje",
      kind: "project" as const,
      run: () => {
        navigate({ name: "project", id: p.id });
        done();
      },
    }));

    const taskItems: Item[] = (q.length > 1 ? (tasks.data ?? []) : []).map((t) => ({
      id: `task-${t.id}`,
      label: t.title,
      hint: STATUS_LABEL[t.status],
      kind: "task" as const,
      run: () => {
        onOpenTask(t);
        done();
      },
    }));

    const filtered = [...nav, ...actions, ...projectItems].filter(
      (i) => q === "" || i.label.toLowerCase().includes(q),
    );

    const createItem: Item[] =
      query.trim().length > 2
        ? [
            {
              id: "create-task",
              label: `Yeni görev: "${query.trim()}"`,
              hint: "⏎ oluştur",
              kind: "create",
              run: () => {
                createTask.mutate({
                  title: query.trim(),
                  status: "INBOX",
                  source: "QUICK_CAPTURE",
                });
                done();
              },
            },
          ]
        : [];

    return [...filtered.slice(0, 8), ...taskItems, ...createItem];
  })();

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setIndex((i) => Math.min(i + 1, items.length - 1));
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setIndex((i) => Math.max(i - 1, 0));
    }
    if (e.key === "Enter" && items[index]) {
      e.preventDefault();
      items[index].run();
    }
  };

  return (
    <div
      className="overlay palette-overlay"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="palette">
        <div className="palette-input-row">
          {mode === "capture" ? <IconPlus size={15} /> : <IconSparkle size={15} />}
          <input
            ref={inputRef}
            className="palette-input"
            placeholder={
              mode === "capture"
                ? "Aklındakini yaz, Enter'la görev olsun…"
                : "Komut, görev ya da proje ara…"
            }
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKey}
          />
          <span className="palette-esc">esc</span>
        </div>
        {items.length > 0 && (
          <div className="palette-list">
            {items.map((item, i) => (
              <button
                key={item.id}
                className={`palette-item${i === index ? " active" : ""}`}
                onMouseEnter={() => setIndex(i)}
                onClick={item.run}
              >
                <span className="palette-label">{item.label}</span>
                {item.hint && <span className="palette-hint">{item.hint}</span>}
                {i === index && <IconArrowRight size={12} />}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
