import type { DetectedWork } from "../lib/types";
import { DETECTED_LABEL, fmtRelativeDay } from "../lib/format";
import { useConvertDetected, useDismissDetected } from "../lib/queries";
import { IconSparkle } from "./Icons";

/** Observer tespiti: sistem görev YARATMAZ — kullanıcı dönüştürür ya da yoksayar. */
export function DetectedCard({ item }: { item: DetectedWork }) {
  const convert = useConvertDetected();
  const dismiss = useDismissDetected();

  return (
    <div className="detected-card">
      <span className="detected-icon">
        <IconSparkle size={13} />
      </span>
      <div className="detected-main">
        <div className="detected-title-line">
          <span className="detected-kind">{DETECTED_LABEL[item.kind]}</span>
          <span className="detected-title">{item.title}</span>
        </div>
        <div className="detected-detail">
          {item.detail}
          {item.projectName && <> · {item.projectName}</>}
          <span className="detected-when"> · {fmtRelativeDay(item.firstDetectedAt)}</span>
        </div>
      </div>
      <div className="detected-actions">
        <span className="chip chip-quiet" title="Tespit güveni">
          %{Math.round(item.confidence * 100)}
        </span>
        {item.kind !== "STALE_TASK" && (
          <button
            className="btn btn-small"
            disabled={convert.isPending}
            onClick={() => convert.mutate(item.id)}
          >
            Görev oluştur
          </button>
        )}
        <button
          className="btn btn-small btn-quiet"
          disabled={dismiss.isPending}
          onClick={() => dismiss.mutate(item.id)}
        >
          Yoksay
        </button>
      </div>
    </div>
  );
}
