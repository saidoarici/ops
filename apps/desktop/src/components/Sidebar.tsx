import type { ComponentType } from "react";
import type { Route } from "../lib/navigation";
import { useDaemon, useToday } from "../lib/queries";
import {
  IconActivity,
  IconBell,
  IconInbox,
  IconProjects,
  IconRepeat,
  IconSettings,
  IconShield,
  IconSparkle,
  IconTasks,
  IconToday,
  IconWaiting,
} from "./Icons";

interface Item {
  route: Route["name"];
  label: string;
  icon: ComponentType<{ size?: number }>;
  count?: number;
}

export function Sidebar({ route, navigate }: { route: Route; navigate: (r: Route) => void }) {
  const today = useToday();
  const daemon = useDaemon();
  const stats = today.data?.stats;

  const main: Item[] = [
    { route: "today", label: "Bugün", icon: IconToday },
    { route: "inbox", label: "Gelen Kutusu", icon: IconInbox, count: stats?.inbox },
    { route: "tasks", label: "Görevler", icon: IconTasks },
    { route: "waiting", label: "Bekleyenler", icon: IconWaiting, count: stats?.waiting },
    { route: "projects", label: "Projeler", icon: IconProjects },
  ];
  const secondary: Item[] = [
    { route: "assistant", label: "Asistan", icon: IconSparkle },
    { route: "routines", label: "Rutinler", icon: IconRepeat },
    { route: "reminders", label: "Hatırlatmalar", icon: IconBell },
    { route: "activity", label: "Aktivite", icon: IconActivity },
    { route: "security", label: "Güvenlik", icon: IconShield },
    { route: "settings", label: "Ayarlar", icon: IconSettings },
  ];

  const renderItem = (item: Item) => {
    const active =
      route.name === item.route || (item.route === "projects" && route.name === "project");
    const Icon = item.icon;
    return (
      <button
        key={item.route}
        className={`nav-item${active ? " active" : ""}`}
        onClick={() => navigate({ name: item.route } as Route)}
      >
        <Icon size={15} />
        <span className="nav-label">{item.label}</span>
        {item.count != null && item.count > 0 && <span className="nav-count">{item.count}</span>}
      </button>
    );
  };

  const connected = daemon.data?.connected ?? false;

  return (
    <nav className="sidebar">
      <div className="sidebar-drag" data-tauri-drag-region />
      <div className="nav-group">{main.map(renderItem)}</div>
      <div className="nav-sep" />
      <div className="nav-group">{secondary.map(renderItem)}</div>
      <div className="sidebar-spacer" />
      <div className="sidebar-status">
        <span className={`status-dot ${connected ? "on" : "off"}`} />
        <div className="status-text">
          <span>{connected ? "Servis aktif" : "Servis bağlı değil"}</span>
          {connected && daemon.data?.health && (
            <span className="status-sub">v{daemon.data.health.version}</span>
          )}
        </div>
      </div>
    </nav>
  );
}
