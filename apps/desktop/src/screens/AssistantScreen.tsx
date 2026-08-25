import { useEffect, useMemo, useRef, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { IconSparkle, IconX } from "../components/Icons";
import { Markdown } from "../components/Markdown";
import { fmtRelativeDay, fmtTime } from "../lib/format";
import {
  useAgentCancel,
  useAgentChat,
  useAgentDetect,
  useAgentMessages,
  useAgentSession,
  useAgentSessions,
  useFullAccessStatus,
  useLockFullAccess,
  useProjects,
} from "../lib/queries";
import type { AgentMode, AgentProviderKind, AgentSession } from "../lib/types";

const MODE_LABEL: Record<AgentMode, string> = {
  ASK: "Sor",
  READ: "Oku",
  EDIT: "Düzenle",
  ACT: "Uygula",
  FULL: "Tam Erişim",
};

const MODE_HINT: Record<AgentMode, string> = {
  ASK: "Sadece konuşur; hiçbir araca erişemez.",
  READ: "Proje dosyalarını okuyabilir, değiştiremez.",
  EDIT: "Proje kökünde dosya düzenleyebilir (onaylı kök dışına çıkamaz).",
  ACT: "Düzenleme + önceden onaylı komut aileleri (git, test…). sudo her zaman yasak.",
  FULL: "Parola onayıyla bu Mac kullanıcısının erişebildiği dosya ve komutlara ulaşır. 30 dakika hareketsizlikte kilitlenir.",
};

const STATUS_LABEL: Record<AgentSession["status"], string> = {
  RUNNING: "çalışıyor",
  COMPLETED: "tamamlandı",
  FAILED: "hata",
  CANCELLED: "iptal",
};

export function AssistantScreen() {
  const detect = useAgentDetect();
  const sessions = useAgentSessions();
  const projects = useProjects();
  const chat = useAgentChat();
  const cancel = useAgentCancel();
  const fullStatus = useFullAccessStatus();
  const lockFull = useLockFullAccess();

  const [activeId, setActiveId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [provider, setProvider] = useState<AgentProviderKind>("CLAUDE");
  const [mode, setMode] = useState<AgentMode>("ASK");
  const [projectId, setProjectId] = useState<string>("");
  const [actApproved, setActApproved] = useState(false);
  const [fullPassword, setFullPassword] = useState("");

  const session = useAgentSession(activeId);
  const running = session.data?.status === "RUNNING";
  const messages = useAgentMessages(activeId, running);
  const bottomRef = useRef<HTMLDivElement>(null);

  const claudeOk = detect.data?.claude.installed ?? false;
  const codexOk = detect.data?.codex.installed ?? false;
  const anyProvider = claudeOk || codexOk;

  useEffect(() => {
    if (detect.data && !claudeOk && codexOk) setProvider("CODEX");
  }, [detect.data, claudeOk, codexOk]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.data?.length, running]);

  const canSend = useMemo(() => {
    if (chat.isPending || running) return false;
    if (draft.trim() === "") return false;
    if (activeId) return true;
    if (!anyProvider) return false;
    if (!["ASK", "FULL"].includes(mode) && projectId === "") return false;
    if (mode === "ACT" && !actApproved) return false;
    if (mode === "FULL" && (!fullStatus.data?.configured || fullPassword === "")) return false;
    return true;
  }, [
    chat.isPending,
    running,
    draft,
    activeId,
    anyProvider,
    mode,
    projectId,
    actApproved,
    fullStatus.data?.configured,
    fullPassword,
  ]);

  const send = () => {
    if (!canSend) return;
    const prompt = draft.trim();
    chat.mutate(
      activeId
        ? {
            sessionId: activeId,
            prompt,
            fullAccessPassword:
              session.data?.mode === "FULL" && fullPassword ? fullPassword : undefined,
          }
        : {
            provider,
            mode,
            projectId: projectId || undefined,
            prompt,
            confirmAct: mode === "ACT" ? actApproved : undefined,
            fullAccessPassword: mode === "FULL" ? fullPassword : undefined,
          },
      {
        onSuccess: (s) => {
          setDraft("");
          setFullPassword("");
          setActiveId(s.id);
        },
      },
    );
  };

  const newChat = () => {
    setActiveId(null);
    setDraft("");
    setActApproved(false);
    setFullPassword("");
  };

  return (
    <div className="assistant">
      <aside className="chat-list">
        <button className="btn btn-primary chat-new" onClick={newChat}>
          <IconSparkle size={13} /> Yeni sohbet
        </button>
        <div className="chat-list-scroll">
          {(sessions.data ?? []).map((s) => (
            <button
              key={s.id}
              className={`chat-item${s.id === activeId ? " active" : ""}`}
              onClick={() => setActiveId(s.id)}
            >
              <span className="chat-item-title">{s.title ?? "Sohbet"}</span>
              <span className="chat-item-sub">
                {s.provider === "CLAUDE" ? "Claude" : "Codex"} · {MODE_LABEL[s.mode]}
                {s.projectName && ` · ${s.projectName}`}
                {" · "}
                {fmtRelativeDay(s.lastActivityAt ?? s.createdAt)}
                {s.status === "RUNNING" && <span className="chat-running"> ●</span>}
              </span>
            </button>
          ))}
        </div>
      </aside>

      <div className="chat-main">
        {activeId === null ? (
          <div className="chat-setup">
            <h1>Asistan</h1>
            <p className="page-sub">
              Kurulu resmi CLI'larla lokal sohbet — API anahtarı gerekmez, credential'lara
              dokunulmaz.
            </p>

            {!anyProvider && !detect.isLoading && (
              <div className="banner banner-err">
                Claude Code veya Codex CLI bulunamadı. Kurulum sonrası Ayarlar'dan tara.
              </div>
            )}

            <div className="chat-setup-row">
              <label className="field">
                <span>Sağlayıcı</span>
                <div className="seg">
                  <button
                    className={`seg-item${provider === "CLAUDE" ? " active" : ""}`}
                    disabled={!claudeOk}
                    onClick={() => setProvider("CLAUDE")}
                    title={detect.data?.claude.version ?? "kurulu değil"}
                  >
                    Claude{!claudeOk && " (yok)"}
                  </button>
                  <button
                    className={`seg-item${provider === "CODEX" ? " active" : ""}`}
                    disabled={!codexOk}
                    onClick={() => setProvider("CODEX")}
                    title={detect.data?.codex.version ?? "kurulu değil"}
                  >
                    Codex{!codexOk && " (yok)"}
                  </button>
                </div>
              </label>

              <label className="field">
                <span>Proje</span>
                <select value={projectId} onChange={(e) => setProjectId(e.target.value)}>
                  <option value="">Genel (yalnızca Sor)</option>
                  {(projects.data ?? []).map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>

              <label className="field">
                <span>Mod</span>
                <div className="seg">
                  {(Object.keys(MODE_LABEL) as AgentMode[]).map((m) => (
                    <button
                      key={m}
                      className={`seg-item${mode === m ? " active" : ""}`}
                      disabled={!["ASK", "FULL"].includes(m) && projectId === ""}
                      onClick={() => setMode(m)}
                    >
                      {MODE_LABEL[m]}
                    </button>
                  ))}
                </div>
              </label>
            </div>

            <p className="mode-hint">{MODE_HINT[mode]}</p>

            {mode === "ACT" && (
              <label className="act-confirm">
                <input
                  type="checkbox"
                  checked={actApproved}
                  onChange={(e) => setActApproved(e.target.checked)}
                />
                <span>
                  <b>Uygula modunu onaylıyorum.</b> Asistan bu projede dosya değiştirebilir ve
                  onaylı komutları (git, test…) çalıştırabilir. Bu onay yalnızca bu oturum için
                  geçerlidir.
                </span>
              </label>
            )}

            {mode === "FULL" && (
              <div className="full-confirm">
                <b>Tam Erişim — yalnızca yerel oturum</b>
                {fullStatus.data?.configured ? (
                  <>
                    <p>
                      Parola provider'a gönderilmez. Yetki daemon yeniden başladığında veya{" "}
                      {fullStatus.data.unlockMinutes} dakika hareketsizlikte kapanır.
                    </p>
                    <input
                      type="password"
                      value={fullPassword}
                      onChange={(e) => setFullPassword(e.target.value)}
                      placeholder="Tam Erişim parolası"
                      autoComplete="off"
                    />
                  </>
                ) : (
                  <p>Önce Ayarlar → Tam Erişim bölümünden yerel parolanı oluştur.</p>
                )}
              </div>
            )}
          </div>
        ) : (
          <div className="chat-head">
            <div className="chat-head-main">
              <span className="chat-head-title">{session.data?.title ?? "Sohbet"}</span>
              <span className="chat-head-sub">
                {session.data?.provider === "CLAUDE" ? "Claude Code" : "Codex"} ·{" "}
                {session.data && MODE_LABEL[session.data.mode]}
                {session.data?.projectName && ` · ${session.data.projectName}`} ·{" "}
                {session.data && STATUS_LABEL[session.data.status]}
              </span>
            </div>
            <div className="chat-head-actions">
              {session.data?.mode === "FULL" && (
                <button
                  className="btn btn-small btn-quiet"
                  onClick={() =>
                    activeId && lockFull.mutate(activeId, { onSuccess: () => setFullPassword("") })
                  }
                >
                  Kilitle
                </button>
              )}
              {running && (
                <button
                  className="btn btn-small"
                  onClick={() => activeId && cancel.mutate(activeId)}
                >
                  <IconX size={12} /> Durdur
                </button>
              )}
            </div>
          </div>
        )}

        {activeId !== null && (
          <div className="chat-scroll">
            {(messages.data ?? []).length === 0 && !running ? (
              <EmptyState title="Mesaj yok" hint="Alttan ilk mesajı gönder." />
            ) : (
              (messages.data ?? []).map((m) => {
                switch (m.role) {
                  case "USER":
                    return (
                      <div key={m.id} className="msg msg-user">
                        <div className="msg-bubble">{m.content}</div>
                      </div>
                    );
                  case "ASSISTANT":
                    return (
                      <div key={m.id} className="msg msg-assistant">
                        <Markdown text={m.content} />
                        <span className="msg-time">{fmtTime(m.createdAt)}</span>
                      </div>
                    );
                  case "TOOL":
                    return (
                      <div key={m.id} className="msg msg-tool">
                        <code>{m.content}</code>
                      </div>
                    );
                  case "ERROR":
                    return (
                      <div key={m.id} className="msg msg-error">
                        {m.content}
                      </div>
                    );
                  default:
                    return (
                      <div key={m.id} className="msg msg-system">
                        {m.content}
                      </div>
                    );
                }
              })
            )}
            {running && (
              <div className="msg msg-working">
                <span className="working-dot" />
                çalışıyor…
              </div>
            )}
            <div ref={bottomRef} />
          </div>
        )}

        {activeId !== null && session.data?.mode === "FULL" && (
          <div className="chat-full-unlock">
            <span>Kilit açmak gerekirse:</span>
            <input
              type="password"
              value={fullPassword}
              onChange={(e) => setFullPassword(e.target.value)}
              placeholder="Tam Erişim parolası (isteğe bağlı)"
              autoComplete="off"
              disabled={running || chat.isPending}
            />
          </div>
        )}
        <div className="chat-composer">
          <textarea
            value={draft}
            placeholder={
              activeId
                ? "Devam et… (Enter gönderir, ⇧Enter yeni satır, /görev <başlık> görev oluşturur)"
                : "Ne yapalım? (Enter gönderir)"
            }
            rows={draft.split("\n").length > 3 ? 5 : 2}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
            disabled={running || chat.isPending}
          />
          <button className="btn btn-primary" onClick={send} disabled={!canSend}>
            Gönder
          </button>
        </div>
        {chat.isError && <p className="form-err chat-err">{chat.error.message}</p>}
      </div>
    </div>
  );
}
