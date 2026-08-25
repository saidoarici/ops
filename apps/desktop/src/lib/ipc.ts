// Daemon'a tek geçit: Tauri `ops_call` komutu → UDS → personal-opsd.
// İş mantığı daemon'dadır; burada yalnızca tipli sarmalayıcılar var.

import { invoke } from "@tauri-apps/api/core";
import type { DaemonStatus } from "./types";

export class OpsError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.name = "OpsError";
  }
}

function toOpsError(e: unknown): OpsError {
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    const p = e as { code: string; message: string };
    return new OpsError(p.code, p.message);
  }
  return new OpsError("INTERNAL", String(e));
}

export async function ops<T>(method: string, params?: unknown): Promise<T> {
  try {
    return await invoke<T>("ops_call", { method, params: params ?? null });
  } catch (e) {
    throw toOpsError(e);
  }
}

export async function daemonStatus(): Promise<DaemonStatus> {
  return invoke<DaemonStatus>("daemon_status");
}

export async function startDaemonDev(): Promise<boolean> {
  try {
    return await invoke<boolean>("start_daemon_dev");
  } catch (e) {
    throw toOpsError(e);
  }
}
