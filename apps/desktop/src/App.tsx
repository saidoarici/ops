import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CommandPalette, type PaletteMode } from "./components/CommandPalette";
import { DaemonGate } from "./components/DaemonGate";
import { Dialog } from "./components/Dialog";
import { Sidebar } from "./components/Sidebar";
import { TaskDetailPanel } from "./components/TaskDetailPanel";
import { hashFromRoute, routeFromHash, type Route } from "./lib/navigation";
import { applyTheme, readStoredTheme, storeTheme, type ThemePref } from "./lib/theme";
import type { Task } from "./lib/types";
import { TodayScreen } from "./screens/TodayScreen";
import { InboxScreen } from "./screens/InboxScreen";
import { TasksScreen } from "./screens/TasksScreen";
import { WaitingScreen } from "./screens/WaitingScreen";
import { ProjectsScreen } from "./screens/ProjectsScreen";
import { ProjectScreen } from "./screens/ProjectScreen";
import { AssistantScreen } from "./screens/AssistantScreen";
import { RoutinesScreen } from "./screens/RoutinesScreen";
import { RemindersScreen } from "./screens/RemindersScreen";
import { ActivityScreen } from "./screens/ActivityScreen";
import { SecurityScreen } from "./screens/SecurityScreen";
import { SettingsScreen } from "./screens/SettingsScreen";

const ONBOARDED_KEY = "onboarded";

function readOnboarded(): boolean {
  try {
    return localStorage.getItem(ONBOARDED_KEY) === "1";
  } catch {
    return false;
  }
}

function markOnboarded() {
  try {
    localStorage.setItem(ONBOARDED_KEY, "1");
  } catch {
    /* saklanamazsa bir sonraki açılışta tekrar gösterilir */
  }
}

export default function App() {
  const [route, setRoute] = useState<Route>(
    () => routeFromHash(window.location.hash) ?? { name: "today" },
  );
  const [openTask, setOpenTask] = useState<Task | null>(null);
  const [palette, setPalette] = useState<PaletteMode | null>(null);
  const [showOnboarding, setShowOnboarding] = useState(() => !readOnboarded());
  const [theme, setTheme] = useState<ThemePref>(readStoredTheme);

  useEffect(() => {
    applyTheme(theme);
    storeTheme(theme);
  }, [theme]);

  // ⌘K komut paleti · ⌘N hızlı görev
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette((p) => (p === "command" ? null : "command"));
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        setPalette("capture");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // ⌥Space global kısayolu / tray "Hızlı Görev" → capture paleti
  useEffect(() => {
    const un = listen("quick-capture", () => setPalette("capture"));
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const navigate = useCallback((r: Route) => {
    setRoute(r);
    setOpenTask(null);
  }, []);

  useEffect(() => {
    window.history.replaceState(null, "", hashFromRoute(route));
  }, [route]);

  const openTaskPanel = useCallback((t: Task) => setOpenTask(t), []);
  const closeTaskPanel = useCallback(() => setOpenTask(null), []);
  const finishOnboarding = () => {
    markOnboarded();
    setShowOnboarding(false);
  };

  return (
    <DaemonGate>
      <div className="app">
        <Sidebar route={route} navigate={navigate} />
        <main className="content" onClick={closeTaskPanel}>
          <div className="content-drag" data-tauri-drag-region />
          {route.name === "today" && <TodayScreen onOpenTask={openTaskPanel} navigate={navigate} />}
          {route.name === "inbox" && <InboxScreen onOpenTask={openTaskPanel} />}
          {route.name === "tasks" && <TasksScreen onOpenTask={openTaskPanel} />}
          {route.name === "waiting" && <WaitingScreen onOpenTask={openTaskPanel} />}
          {route.name === "projects" && <ProjectsScreen navigate={navigate} />}
          {route.name === "project" && (
            <ProjectScreen id={route.id} onOpenTask={openTaskPanel} navigate={navigate} />
          )}
          {route.name === "assistant" && <AssistantScreen />}
          {route.name === "routines" && <RoutinesScreen />}
          {route.name === "reminders" && <RemindersScreen />}
          {route.name === "activity" && <ActivityScreen />}
          {route.name === "security" && <SecurityScreen />}
          {route.name === "settings" && <SettingsScreen theme={theme} setTheme={setTheme} />}
        </main>
        {openTask && <TaskDetailPanel key={openTask.id} task={openTask} onClose={closeTaskPanel} />}
        {palette && (
          <CommandPalette
            mode={palette}
            onClose={() => setPalette(null)}
            navigate={navigate}
            onOpenTask={openTaskPanel}
          />
        )}
        {showOnboarding && (
          <Dialog
            title="Personal Ops'a hoş geldin"
            onClose={finishOnboarding}
            footer={
              <button className="btn btn-primary" onClick={finishOnboarding}>
                Başla
              </button>
            }
          >
            <p className="ob-line">
              <b>1 · İşlerini o takip etsin.</b> Projelerine yerel klasör bağla; commit'ler ve dosya
              hareketleri kanıt olarak birikir, yarım kalan işler kendiliğinden yüzeye çıkar.
            </p>
            <p className="ob-line">
              <b>2 · Her şey bu Mac'te kalır.</b> Veriler lokal SQLite'ta; Telegram'dan gelen
              mesajlar yalnızca gelen kutusuna metin ekleyebilir — asla komut çalıştıramaz.
            </p>
            <p className="ob-line">
              <b>3 · Kısayollar:</b> <code>⌘K</code> komut paleti, <code>⌘N</code> ya da{" "}
              <code>⌥Space</code> hızlı görev, menü çubuğundaki simge her yerden erişim.
            </p>
          </Dialog>
        )}
      </div>
    </DaemonGate>
  );
}
