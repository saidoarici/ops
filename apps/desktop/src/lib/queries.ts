// React Query katmanı. Daemon'dan UI'a event akışı yoktur; listeler kısa
// aralıkla poll edilir, mutasyonlar tüm sorguları tazeler (docs/architecture.md).

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { daemonStatus, ops } from "./ipc";
import type {
  AgentChatRequest,
  AgentDetectReport,
  AgentMessage,
  AgentSession,
  AuditEvent,
  AuditVerifyReport,
  BackupInfo,
  DaemonStatus,
  DetectedWork,
  Evidence,
  FullAccessStatus,
  ObserverStatus,
  Project,
  ProjectCreate,
  ProjectOverview,
  ProjectPatch,
  ProjectWithStats,
  Reminder,
  ReminderCreate,
  RemoteMessage,
  RemoteStatus,
  Routine,
  RoutinePatch,
  ScanSummary,
  Settings,
  Task,
  TaskCreate,
  TaskFilter,
  TaskPatch,
  TodayView,
} from "./types";

const POLL_MS = 5000;

export function useDaemon() {
  return useQuery<DaemonStatus>({
    queryKey: ["daemon"],
    queryFn: daemonStatus,
    refetchInterval: (q) => (q.state.data?.connected ? 15000 : 2500),
    retry: false,
  });
}

function localOffsetMinutes(): number {
  return -new Date().getTimezoneOffset();
}

export function useToday(enabled = true) {
  return useQuery<TodayView>({
    queryKey: ["today"],
    queryFn: () => ops<TodayView>("today.view", { utcOffsetMinutes: localOffsetMinutes() }),
    refetchInterval: POLL_MS,
    enabled,
  });
}

export function useTasks(filter: TaskFilter, enabled = true) {
  return useQuery<Task[]>({
    queryKey: ["tasks", filter],
    queryFn: () => ops<Task[]>("task.list", filter),
    refetchInterval: POLL_MS,
    enabled,
  });
}

export function useProjects(includeArchived = false) {
  return useQuery<ProjectWithStats[]>({
    queryKey: ["projects", includeArchived],
    queryFn: () => ops<ProjectWithStats[]>("project.list", { includeArchived }),
    refetchInterval: POLL_MS,
  });
}

export function useReminders() {
  return useQuery<Reminder[]>({
    queryKey: ["reminders"],
    queryFn: () => ops<Reminder[]>("reminder.list", { limit: 300 }),
    refetchInterval: POLL_MS,
  });
}

export function useAudit(limit = 200) {
  return useQuery<AuditEvent[]>({
    queryKey: ["audit", limit],
    queryFn: () => ops<AuditEvent[]>("audit.list", { limit }),
    refetchInterval: POLL_MS,
  });
}

export function useEvidence(projectId?: string, limit = 100) {
  return useQuery<Evidence[]>({
    queryKey: ["evidence", projectId ?? "all", limit],
    queryFn: () => ops<Evidence[]>("evidence.list", { projectId, limit }),
    refetchInterval: POLL_MS,
  });
}

export function useObserverStatus() {
  return useQuery<ObserverStatus>({
    queryKey: ["observer"],
    queryFn: () => ops<ObserverStatus>("observer.status"),
    refetchInterval: 10000,
  });
}

export function useProjectOverview(id: string) {
  return useQuery<ProjectOverview>({
    queryKey: ["project-overview", id],
    queryFn: () => ops<ProjectOverview>("project.overview", { id }),
    refetchInterval: POLL_MS,
  });
}

export function useAgentDetect() {
  return useQuery<AgentDetectReport>({
    queryKey: ["agent-detect"],
    queryFn: () => ops<AgentDetectReport>("agent.detect"),
    staleTime: 60_000,
  });
}

export function useAgentSessions() {
  return useQuery<AgentSession[]>({
    queryKey: ["agent-sessions"],
    queryFn: () => ops<AgentSession[]>("agent.sessions", { limit: 30 }),
    refetchInterval: POLL_MS,
  });
}

export function useAgentSession(id: string | null) {
  return useQuery<AgentSession>({
    queryKey: ["agent-session", id],
    queryFn: () => ops<AgentSession>("agent.session", { id }),
    enabled: id !== null,
    refetchInterval: (q) => (q.state.data?.status === "RUNNING" ? 1200 : 6000),
  });
}

export function useAgentMessages(sessionId: string | null, running: boolean) {
  return useQuery<AgentMessage[]>({
    queryKey: ["agent-messages", sessionId],
    queryFn: () => ops<AgentMessage[]>("agent.messages", { sessionId }),
    enabled: sessionId !== null,
    refetchInterval: running ? 1200 : 6000,
  });
}

export function useAgentChat() {
  return useOpsMutation((req: AgentChatRequest) => ops<AgentSession>("agent.chat", req));
}

export function useAgentCancel() {
  return useOpsMutation((id: string) => ops<{ cancelled: boolean }>("agent.cancel", { id }));
}

export function useFullAccessStatus() {
  return useQuery<FullAccessStatus>({
    queryKey: ["full-access-status"],
    queryFn: () => ops<FullAccessStatus>("agent.fullAccess.status"),
    staleTime: 30_000,
  });
}

export function useConfigureFullAccess() {
  return useOpsMutation(
    ({ newPassword, currentPassword }: { newPassword: string; currentPassword?: string }) =>
      ops<FullAccessStatus>("agent.fullAccess.configure", {
        newPassword,
        currentPassword: currentPassword || undefined,
      }),
  );
}

export function useLockFullAccess() {
  return useOpsMutation((id: string) => ops<{ locked: boolean }>("agent.fullAccess.lock", { id }));
}

export function useSettings() {
  return useQuery<Settings>({
    queryKey: ["settings"],
    queryFn: () => ops<Settings>("settings.get"),
    staleTime: 60_000,
  });
}

export function useBackups() {
  return useQuery<BackupInfo[]>({
    queryKey: ["backups"],
    queryFn: () => ops<BackupInfo[]>("data.backups"),
  });
}

// ------------------------------------------------------------------ mutations

function useOpsMutation<TArgs, TOut = unknown>(
  fn: (args: TArgs) => Promise<TOut>,
): UseMutationResult<TOut, Error, TArgs> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: () => {
      // Tek kullanıcılı lokal uygulama: topyekûn tazeleme ucuz ve doğru.
      void qc.invalidateQueries();
    },
  });
}

export function useCreateTask() {
  return useOpsMutation((input: TaskCreate) => ops<Task>("task.create", input));
}

export function useUpdateTask() {
  return useOpsMutation(({ id, patch }: { id: string; patch: TaskPatch }) =>
    ops<Task>("task.update", { id, patch }),
  );
}

export function useCompleteTask() {
  return useOpsMutation((id: string) => ops<Task>("task.complete", { id }));
}

export function useArchiveTask() {
  return useOpsMutation((id: string) => ops<Task>("task.archive", { id }));
}

export function useCreateProject() {
  return useOpsMutation((input: ProjectCreate) => ops<Project>("project.create", input));
}

export function useUpdateProject() {
  return useOpsMutation(({ id, patch }: { id: string; patch: ProjectPatch }) =>
    ops("project.update", { id, patch }),
  );
}

export function useCreateReminder() {
  return useOpsMutation((input: ReminderCreate) => ops<Reminder>("reminder.create", input));
}

export function useDismissReminder() {
  return useOpsMutation((id: string) => ops<Reminder>("reminder.dismiss", { id }));
}

export function useSetSetting() {
  return useOpsMutation(({ key, value }: { key: string; value: unknown }) =>
    ops<Settings>("settings.set", { key, value }),
  );
}

export function useRunBackup() {
  return useOpsMutation(() => ops<BackupInfo>("data.backup"));
}

export function useVerifyAudit() {
  return useOpsMutation(() => ops<AuditVerifyReport>("audit.verify"));
}

export function useDismissDetected() {
  return useOpsMutation((id: string) => ops<DetectedWork>("detected.dismiss", { id }));
}

export function useConvertDetected() {
  return useOpsMutation((id: string) => ops<Task>("detected.convert", { id }));
}

export function useRunScan() {
  return useOpsMutation(() => ops<ScanSummary>("observer.scan"));
}

export function useRoutines() {
  return useQuery<Routine[]>({
    queryKey: ["routines"],
    queryFn: () => ops<Routine[]>("routine.list"),
    refetchInterval: 15000,
  });
}

export function useUpdateRoutine() {
  return useOpsMutation(({ id, patch }: { id: string; patch: RoutinePatch }) =>
    ops<Routine>("routine.update", { id, patch }),
  );
}

export function useRunRoutine() {
  return useOpsMutation((id: string) => ops<{ text: string }>("routine.run", { id }));
}

export function useRemoteMessages() {
  return useQuery<RemoteMessage[]>({
    queryKey: ["remote-messages"],
    queryFn: () => ops<RemoteMessage[]>("remote.messages", { limit: 20 }),
    refetchInterval: 10000,
  });
}

export function useRemoteStatus() {
  return useQuery<RemoteStatus>({
    queryKey: ["remote-status"],
    queryFn: () => ops<RemoteStatus>("remote.status"),
    refetchInterval: 10000,
  });
}

export function useTelegramConfigure() {
  return useOpsMutation((p: { token: string; allowedUserId: string; allowedChatId: string }) =>
    ops<{ botName: string }>("remote.telegram.configure", p),
  );
}

export function useTelegramDisable() {
  return useOpsMutation(() => ops<{ ok: boolean }>("remote.telegram.disable"));
}

export function useTelegramTest() {
  return useOpsMutation(() => ops<{ botName: string }>("remote.telegram.test"));
}

export function useWhatsappConfigure() {
  return useOpsMutation((p: { baseUrl: string; apiKey: string; phoneNumber: string }) =>
    ops<{ status: string }>("remote.whatsapp.configure", p),
  );
}

export function useWhatsappDisable() {
  return useOpsMutation(() => ops<{ ok: boolean }>("remote.whatsapp.disable"));
}

export function useWhatsappTest() {
  return useOpsMutation(() => ops<{ status: string }>("remote.whatsapp.test"));
}
