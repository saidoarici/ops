// Rust modellerinin elle senkron TS aynası.
// Tek doğruluk kaynağı: crates/ops-core/src/models/ (bkz. docs/data-model.md).

export type TaskStatus =
  | "INBOX"
  | "PLANNED"
  | "NEXT"
  | "IN_PROGRESS"
  | "WAITING"
  | "BLOCKED"
  | "SOMEDAY"
  | "DONE"
  | "CANCELLED";

export type TaskSource =
  "LOCAL_UI" | "QUICK_CAPTURE" | "TELEGRAM" | "WHATSAPP" | "AGENT_CHAT" | "AI_DETECTED";

export type EnergyLevel = "LOW" | "MEDIUM" | "HIGH";
export type ProjectState = "ACTIVE" | "PAUSED" | "ARCHIVED" | "COMPLETED";
export type ProjectHealth =
  "ACTIVE" | "QUIET" | "STALE" | "BLOCKED" | "WAITING" | "AT_RISK" | "COMPLETED";
export type ReminderStatus = "SCHEDULED" | "FIRED" | "DISMISSED" | "MISSED";
export type RepeatRule = "NONE" | "DAILY" | "WEEKDAYS" | "WEEKLY" | "MONTHLY";
export type NotificationChannel = "MACOS" | "TELEGRAM" | "WHATSAPP";
export type AuditResultKind = "OK" | "DENIED" | "ERROR";
export type RiskLevel = "R0" | "R1" | "R2" | "R3" | "R4";

export interface Task {
  id: string;
  title: string;
  description: string;
  projectId: string | null;
  status: TaskStatus;
  priority: number;
  importance: number;
  urgency: number;
  dueAt: string | null;
  scheduledAt: string | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  parentTaskId: string | null;
  tags: string[];
  source: TaskSource;
  waitingFor: string | null;
  waitingSince: string | null;
  followupAt: string | null;
  blockedBy: string | null;
  estimatedMinutes: number | null;
  energyLevel: EnergyLevel | null;
  archived: boolean;
  projectName?: string | null;
}

export interface TaskPatch {
  title?: string;
  description?: string;
  status?: TaskStatus;
  priority?: number;
  importance?: number;
  urgency?: number;
  tags?: string[];
  projectId?: string | null;
  dueAt?: string | null;
  scheduledAt?: string | null;
  parentTaskId?: string | null;
  waitingFor?: string | null;
  followupAt?: string | null;
  blockedBy?: string | null;
  estimatedMinutes?: number | null;
  energyLevel?: EnergyLevel | null;
}

export interface TaskCreate {
  title: string;
  description?: string;
  projectId?: string;
  status?: TaskStatus;
  priority?: number;
  importance?: number;
  urgency?: number;
  dueAt?: string;
  scheduledAt?: string;
  tags?: string[];
  source?: TaskSource;
  waitingFor?: string;
  followupAt?: string;
  estimatedMinutes?: number;
}

export interface TaskFilter {
  statuses?: TaskStatus[];
  projectId?: string;
  includeArchived?: boolean;
  search?: string;
  limit?: number;
}

export interface ProjectCreate {
  name: string;
  description?: string;
  priority?: number;
  localPaths?: string[];
}

export interface Project {
  id: string;
  name: string;
  description: string;
  state: ProjectState;
  health: ProjectHealth;
  priority: number;
  localPaths: string[];
  gitRepositories: string[];
  keywords: string[];
  relatedContacts: string[];
  lastActivityAt: string | null;
  staleThresholdDays: number;
  createdAt: string;
  updatedAt: string;
}

export interface ProjectWithStats extends Project {
  openTasks: number;
  waitingTasks: number;
  inboxTasks: number;
  lastTaskActivity: string | null;
}

export interface ProjectPatch {
  name?: string;
  description?: string;
  state?: ProjectState;
  health?: ProjectHealth;
  priority?: number;
  localPaths?: string[];
  gitRepositories?: string[];
  keywords?: string[];
  staleThresholdDays?: number;
}

export interface ReminderCreate {
  title: string;
  remindAt: string;
  notes?: string;
  taskId?: string;
  repeatRule?: RepeatRule;
  channels?: NotificationChannel[];
}

export interface Reminder {
  id: string;
  taskId: string | null;
  title: string;
  notes: string;
  remindAt: string;
  repeatRule: RepeatRule;
  channels: NotificationChannel[];
  status: ReminderStatus;
  firedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface AuditEvent {
  id: string;
  seq: number;
  timestamp: string;
  actor: string;
  origin: string;
  action: string;
  target: string | null;
  riskLevel: RiskLevel;
  capability: string | null;
  result: AuditResultKind;
  metadata: Record<string, unknown>;
  previousHash: string;
  hash: string;
}

export interface AuditVerifyReport {
  ok: boolean;
  checked: number;
  brokenAtSeq: number | null;
  message: string;
}

export type AttentionKind = "OVERDUE" | "WAITING_LONG" | "BLOCKED" | "STALE";
export type TimelineKind = "REMINDER" | "DUE" | "SCHEDULED";

export type EvidenceType = "GIT_COMMIT" | "FILE_CHANGE" | "AI_SESSION" | "ROUTINE_RESULT";

export interface Evidence {
  id: string;
  taskId: string | null;
  projectId: string | null;
  type: EvidenceType;
  source: string;
  timestamp: string;
  summary: string;
  confidence: number | null;
  sourceReference: string | null;
  contentHash: string | null;
  createdAt: string;
  projectName?: string | null;
}

export type DetectedKind = "UNCOMMITTED_CHANGES" | "UNPUSHED_COMMITS" | "STALE_TASK";
export type DetectedStatus = "OPEN" | "DISMISSED" | "CONVERTED" | "RESOLVED";

export interface DetectedWork {
  id: string;
  projectId: string | null;
  taskId: string | null;
  kind: DetectedKind;
  title: string;
  detail: string;
  evidenceIds: string[];
  confidence: number;
  status: DetectedStatus;
  suggestedTaskTitle: string | null;
  dedupeKey: string;
  firstDetectedAt: string;
  lastSeenAt: string;
  resolvedAt: string | null;
  createdAt: string;
  projectName?: string | null;
}

export interface RepoState {
  projectId: string;
  repoPath: string;
  branch: string | null;
  headCommit: string | null;
  dirtyFiles: number;
  dirtySince: string | null;
  ahead: number;
  lastCommitAt: string | null;
  lastScanAt: string;
}

export interface ScanSummary {
  projects: number;
  repos: number;
  evidenceAdded: number;
  detectedOpen: number;
  errors: string[];
  finishedAt: string;
}

export interface ObserverStatus {
  running: boolean;
  watchedPaths: string[];
  lastScanAt: string | null;
  lastSummary?: ScanSummary | null;
}

export interface ProjectOverview {
  project: Project;
  repoStates: RepoState[];
  evidence: Evidence[];
  detected: DetectedWork[];
}

export type AgentProviderKind = "CLAUDE" | "CODEX";
export type AgentMode = "ASK" | "READ" | "EDIT" | "ACT" | "FULL";
export type AgentSessionStatus = "RUNNING" | "COMPLETED" | "FAILED" | "CANCELLED";
export type AgentMessageRole = "USER" | "ASSISTANT" | "TOOL" | "SYSTEM" | "ERROR";

export interface AgentSession {
  id: string;
  provider: AgentProviderKind;
  projectId: string | null;
  startedAt: string;
  endedAt: string | null;
  mode: AgentMode;
  workingDirectory: string | null;
  status: AgentSessionStatus;
  summary: string | null;
  evidenceIds: string[];
  createdAt: string;
  providerSessionId: string | null;
  lastActivityAt: string | null;
  title: string | null;
  projectName?: string | null;
}

export interface AgentMessage {
  id: string;
  sessionId: string;
  seq: number;
  role: AgentMessageRole;
  content: string;
  createdAt: string;
}

export interface ProviderInfo {
  installed: boolean;
  path: string | null;
  version: string | null;
}

export interface AgentDetectReport {
  claude: ProviderInfo;
  codex: ProviderInfo;
  checkedAt: string;
}

export interface AgentChatRequest {
  sessionId?: string;
  provider?: AgentProviderKind;
  projectId?: string;
  mode?: AgentMode;
  prompt: string;
  confirmAct?: boolean;
  fullAccessPassword?: string;
}

export interface FullAccessStatus {
  configured: boolean;
  unlockMinutes: number;
}

export interface TelegramStatus {
  configured: boolean;
  enabled: boolean;
  polling: boolean;
  botName: string | null;
  allowedUserSet: boolean;
  allowedChatSet: boolean;
  lastPollAt: string | null;
  lastError: string | null;
}

export interface WhatsAppStatus {
  configured: boolean;
  baseUrl: string | null;
  /** Maskelenmiş numara (yalnızca son 4 hane). */
  phoneNumber: string | null;
}

export interface RemoteStatus {
  telegram: TelegramStatus;
  whatsapp: WhatsAppStatus;
}

export type RoutineAction = "MORNING_BRIEF" | "EVENING_REVIEW" | "WEEKLY_REVIEW";

export interface RoutinePatch {
  enabled?: boolean;
  schedule?: string;
}

export interface Routine {
  id: string;
  name: string;
  enabled: boolean;
  /** "HH:MM" (her gün) ya da "MON HH:MM" (haftalık); makine yerel saati. */
  schedule: string;
  actionType: RoutineAction;
  lastRunAt: string | null;
  nextRunAt: string | null;
  lastResult: { summary?: string; channels?: string[] } | null;
  createdAt: string;
  updatedAt: string;
}

export type RemoteChannel = "TELEGRAM" | "WHATSAPP";
export type RemoteAuthState = "AUTHENTICATED" | "REJECTED_SENDER";
export type RemoteReplayState = "NEW" | "REPLAYED";
export type RemoteProcessingStatus = "PENDING" | "PROCESSED" | "REJECTED";
export type RemoteIntentKind =
  "CREATE_TASK" | "CREATE_REMINDER_PROPOSAL" | "QUERY_TASK" | "ADD_NOTE";

export interface RemoteMessage {
  id: string;
  channel: RemoteChannel;
  externalMessageId: string;
  senderId: string;
  receivedAt: string;
  /** Reddedilen göndericilerde boş bırakılır. */
  rawText: string;
  authenticationState: RemoteAuthState;
  replayState: RemoteReplayState;
  parsedIntent: { type: RemoteIntentKind } | null;
  resultingInboxItemId: string | null;
  processingStatus: RemoteProcessingStatus;
  createdAt: string;
}

export interface TodayView {
  generatedAt: string;
  dayStart: string;
  dayEnd: string;
  focus: { task: Task; whyNow: string; score: number }[];
  needsAttention: { task: Task; kind: AttentionKind; detail: string }[];
  detected: DetectedWork[];
  timeline: {
    at: string;
    kind: TimelineKind;
    title: string;
    taskId?: string;
    reminderId?: string;
    status?: string;
  }[];
  stats: {
    openTasks: number;
    inbox: number;
    waiting: number;
    dueToday: number;
    overdue: number;
    doneToday: number;
  };
}

export interface DaemonHealth {
  ok: boolean;
  version: string;
  uptimeSecs: number;
  dataDir: string;
  socketPath: string;
  time: string;
}

export interface DaemonStatus {
  connected: boolean;
  health?: DaemonHealth;
  error?: string;
  socketPath: string;
}

export interface BackupInfo {
  fileName: string;
  path: string;
  sizeBytes: number;
  createdAt: string | null;
}

/** `settings` tablosu; anahtar allowlist'i daemon tarafındadır. */
export interface Settings {
  display_name?: string;
  telegram_enabled?: boolean;
  telegram_allowed_user_id?: string;
  telegram_allowed_chat_id?: string;
  telegram_last_update_id?: number;
  whatsapp_config?: unknown;
}
