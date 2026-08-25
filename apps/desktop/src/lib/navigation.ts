export type Route =
  | { name: "today" }
  | { name: "inbox" }
  | { name: "tasks" }
  | { name: "waiting" }
  | { name: "projects" }
  | { name: "project"; id: string }
  | { name: "assistant" }
  | { name: "routines" }
  | { name: "reminders" }
  | { name: "activity" }
  | { name: "security" }
  | { name: "settings" };

/** URL hash ↔ rota: "#tasks", "#project/<id>". Yeniden yüklemede ekran korunur. */
export function routeFromHash(hash: string): Route | null {
  const [name, id] = hash.replace(/^#\/?/, "").split("/");
  switch (name) {
    case "today":
    case "inbox":
    case "tasks":
    case "waiting":
    case "projects":
    case "assistant":
    case "routines":
    case "reminders":
    case "activity":
    case "security":
    case "settings":
      return { name };
    case "project":
      return id ? { name: "project", id } : null;
    default:
      return null;
  }
}

export function hashFromRoute(route: Route): string {
  return route.name === "project" ? `#project/${route.id}` : `#${route.name}`;
}
