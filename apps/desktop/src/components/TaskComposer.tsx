import { useState } from "react";
import { useCreateTask } from "../lib/queries";
import type { TaskStatus } from "../lib/types";
import { IconPlus } from "./Icons";

/** Things tarzı hızlı ekleme: yaz, Enter'a bas, görev oluşsun. */
export function TaskComposer({
  placeholder = "Yeni görev ekle…",
  status,
  projectId,
}: {
  placeholder?: string;
  status?: TaskStatus;
  projectId?: string;
}) {
  const [title, setTitle] = useState("");
  const create = useCreateTask();

  const submit = () => {
    const t = title.trim();
    if (!t || create.isPending) return;
    create.mutate({ title: t, status, projectId }, { onSuccess: () => setTitle("") });
  };

  return (
    <div className="composer">
      <IconPlus size={14} className="composer-icon" />
      <input
        className="composer-input"
        value={title}
        placeholder={placeholder}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") submit();
          if (e.key === "Escape") setTitle("");
        }}
      />
      {create.isError && <span className="composer-err">{create.error.message}</span>}
    </div>
  );
}
