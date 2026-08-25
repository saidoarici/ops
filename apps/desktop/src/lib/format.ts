// Türkçe tarih/metin biçimlendirme yardımcıları ve etiket sözlükleri.

import type {
  AttentionKind,
  DetectedKind,
  EvidenceType,
  ProjectHealth,
  RepeatRule,
  TaskSource,
  TaskStatus,
} from "./types";

const dateFmt = new Intl.DateTimeFormat("tr-TR", { day: "numeric", month: "short" });
const dateYearFmt = new Intl.DateTimeFormat("tr-TR", {
  day: "numeric",
  month: "short",
  year: "numeric",
});
const timeFmt = new Intl.DateTimeFormat("tr-TR", { hour: "2-digit", minute: "2-digit" });

export function fmtTime(iso: string): string {
  return timeFmt.format(new Date(iso));
}

export function fmtDate(iso: string): string {
  const d = new Date(iso);
  const now = new Date();
  return d.getFullYear() === now.getFullYear() ? dateFmt.format(d) : dateYearFmt.format(d);
}

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** "bugün", "yarın", "dün", "3 gün önce", "5 gün sonra" ya da kısa tarih. */
export function fmtRelativeDay(iso: string): string {
  const days = Math.round((startOfDay(new Date(iso)) - startOfDay(new Date())) / 86_400_000);
  if (days === 0) return "bugün";
  if (days === 1) return "yarın";
  if (days === -1) return "dün";
  if (days < 0 && days >= -7) return `${-days} gün önce`;
  if (days > 0 && days <= 7) return `${days} gün sonra`;
  return fmtDate(iso);
}

export function fmtDayTime(iso: string): string {
  return `${fmtRelativeDay(iso)} ${fmtTime(iso)}`;
}

export function daysSince(iso: string): number {
  return Math.max(0, Math.round((startOfDay(new Date()) - startOfDay(new Date(iso))) / 86_400_000));
}

/** datetime-local input değeri → ISO (UTC). */
export function localInputToIso(v: string): string {
  return new Date(v).toISOString();
}

/** ISO → datetime-local input değeri (lokal saat). */
export function isoToLocalInput(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function greeting(name?: string): string {
  const h = new Date().getHours();
  const word = h < 6 ? "İyi geceler" : h < 12 ? "Günaydın" : h < 18 ? "İyi günler" : "İyi akşamlar";
  return name ? `${word}, ${name}` : word;
}

export function todayLong(): string {
  return new Intl.DateTimeFormat("tr-TR", {
    weekday: "long",
    day: "numeric",
    month: "long",
  }).format(new Date());
}

// ------------------------------------------------------------------ sözlükler

export const STATUS_LABEL: Record<TaskStatus, string> = {
  INBOX: "Gelen",
  PLANNED: "Planlı",
  NEXT: "Sıradaki",
  IN_PROGRESS: "Sürüyor",
  WAITING: "Bekliyor",
  BLOCKED: "Bloklu",
  SOMEDAY: "Bir gün",
  DONE: "Bitti",
  CANCELLED: "İptal",
};

export const STATUS_ORDER: TaskStatus[] = [
  "IN_PROGRESS",
  "NEXT",
  "PLANNED",
  "INBOX",
  "WAITING",
  "BLOCKED",
  "SOMEDAY",
  "DONE",
  "CANCELLED",
];

export const HEALTH_LABEL: Record<ProjectHealth, string> = {
  ACTIVE: "Aktif",
  QUIET: "Sessiz",
  STALE: "Durgun",
  BLOCKED: "Bloklu",
  WAITING: "Beklemede",
  AT_RISK: "Riskli",
  COMPLETED: "Tamamlandı",
};

export const SOURCE_LABEL: Record<TaskSource, string> = {
  LOCAL_UI: "Uygulama",
  QUICK_CAPTURE: "Hızlı not",
  TELEGRAM: "Telegram",
  WHATSAPP: "WhatsApp",
  AGENT_CHAT: "Asistan",
  AI_DETECTED: "AI tespiti",
};

export const ATTENTION_LABEL: Record<AttentionKind, string> = {
  OVERDUE: "Gecikti",
  WAITING_LONG: "Uzun bekleme",
  BLOCKED: "Bloklu",
  STALE: "Durgun",
};

export const REPEAT_LABEL: Record<RepeatRule, string> = {
  NONE: "Tek sefer",
  DAILY: "Her gün",
  WEEKDAYS: "Hafta içi",
  WEEKLY: "Her hafta",
  MONTHLY: "Her ay",
};

export const DETECTED_LABEL: Record<DetectedKind, string> = {
  UNCOMMITTED_CHANGES: "Commit'lenmemiş iş",
  UNPUSHED_COMMITS: "Push bekliyor",
  STALE_TASK: "Muhtemelen yarım",
};

export const EVIDENCE_LABEL: Record<EvidenceType, string> = {
  GIT_COMMIT: "Commit",
  FILE_CHANGE: "Dosya değişikliği",
  AI_SESSION: "AI oturumu",
  ROUTINE_RESULT: "Rutin sonucu",
};

export const AUDIT_ACTION_LABEL: Record<string, string> = {
  TASK_CREATE: "Görev oluşturuldu",
  TASK_UPDATE: "Görev güncellendi",
  TASK_ARCHIVE: "Görev arşivlendi",
  PROJECT_CREATE: "Proje oluşturuldu",
  PROJECT_UPDATE: "Proje güncellendi",
  REMINDER_CREATE: "Hatırlatma kuruldu",
  REMINDER_UPDATE: "Hatırlatma güncellendi",
  REMINDER_FIRE: "Hatırlatma tetiklendi",
  REMINDER_MISSED: "Hatırlatma kaçırıldı",
  SEND_NOTIFICATION: "Bildirim gönderildi",
  SETTINGS_SET: "Ayar değişti",
  BACKUP_CREATE: "Yedek alındı",
  OBSERVER_OBSERVATION: "Gözlem kaydedildi",
  OBSERVER_SCAN: "Manuel tarama",
  PROJECT_HEALTH_UPDATE: "Proje sağlığı değişti",
  DETECTED_WORK_CREATE: "Yarım iş tespiti",
  DETECTED_WORK_REOPEN: "Tespit yeniden açıldı",
  DETECTED_WORK_DISMISS: "Tespit yoksayıldı",
  DETECTED_WORK_CONVERT: "Tespit göreve çevrildi",
  AGENT_SESSION_START: "Asistan oturumu başladı",
  AGENT_SESSION_END: "Asistan oturumu bitti",
  FULL_ACCESS_PASSWORD_SET: "Tam Erişim parolası ayarlandı",
  FULL_ACCESS_SESSION_LOCK: "Tam Erişim oturumu kilitlendi",
  ROUTINE_UPDATE: "Rutin güncellendi",
  ROUTINE_RUN: "Rutin çalıştı",
  REMOTE_MESSAGE_PROCESSED: "Uzak mesaj işlendi",
  REMOTE_MESSAGE_REJECTED: "Uzak mesaj reddedildi",
};

export function fmtMinutes(m: number): string {
  if (m < 60) return `${m} dk`;
  const h = Math.floor(m / 60);
  const r = m % 60;
  return r === 0 ? `${h} sa` : `${h} sa ${r} dk`;
}

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}
