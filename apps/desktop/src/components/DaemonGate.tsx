import { useState, type ReactNode } from "react";
import { startDaemonDev } from "../lib/ipc";
import { useDaemon } from "../lib/queries";
import { IconShield } from "./Icons";

/**
 * Daemon bağlantı kapısı: servis yoksa uygulama çökmez, durum ekranı gösterir.
 * Bağlantı gelince çocuklar render edilir.
 */
export function DaemonGate({ children }: { children: ReactNode }) {
  const daemon = useDaemon();
  const [starting, setStarting] = useState(false);
  const [startMsg, setStartMsg] = useState<string | null>(null);

  if (daemon.isLoading && !daemon.data) {
    return <div className="gate gate-quiet">Servise bağlanılıyor…</div>;
  }

  if (!daemon.data?.connected) {
    return (
      <div className="gate">
        <div className="gate-card">
          <IconShield size={26} />
          <h2>Arka plan servisi bağlı değil</h2>
          <p>
            Görevler, hatırlatmalar ve gözlemci <code>personal-opsd</code> servisinde çalışır.
            Servis şu an ulaşılamıyor.
          </p>
          <p className="gate-path">{daemon.data?.socketPath}</p>
          <div className="gate-actions">
            <button
              className="btn btn-primary"
              disabled={starting}
              onClick={async () => {
                setStarting(true);
                setStartMsg(null);
                try {
                  const ok = await startDaemonDev();
                  setStartMsg(
                    ok
                      ? "Servis başlatıldı, bağlanılıyor…"
                      : "Derlenmiş personal-opsd bulunamadı. Terminalden: cargo run -p ops-daemon",
                  );
                } catch (e) {
                  setStartMsg(String(e instanceof Error ? e.message : e));
                } finally {
                  setStarting(false);
                  setTimeout(() => void daemon.refetch(), 800);
                }
              }}
            >
              Servisi başlat
            </button>
            <button className="btn" onClick={() => void daemon.refetch()}>
              Yeniden dene
            </button>
          </div>
          {startMsg && <p className="gate-msg">{startMsg}</p>}
          <p className="gate-hint">
            Kalıcı kurulum için: <code>personal-opsd install-launchd</code>
          </p>
        </div>
      </div>
    );
  }

  return <>{children}</>;
}
