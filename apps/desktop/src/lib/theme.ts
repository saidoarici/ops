// Tema tercihi tarayıcı yerel deposunda tutulur; daemon ayarı değildir.

export type ThemePref = "system" | "light" | "dark";

const STORAGE_KEY = "theme";

export function readStoredTheme(): ThemePref {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    return saved === "light" || saved === "dark" ? saved : "system";
  } catch {
    return "system";
  }
}

export function storeTheme(pref: ThemePref) {
  try {
    localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    /* saklanamazsa oturum boyunca bellekte kalır */
  }
}

export function applyTheme(pref: ThemePref) {
  const root = document.documentElement;
  if (pref === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", pref);
  }
}
