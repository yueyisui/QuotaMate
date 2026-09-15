import type { UsageWindow } from "../types";
import { localeName, translate, type LanguagePreference } from "../i18n";

interface Props {
  title: string;
  shortTitle: string;
  window: UsageWindow | null;
  showCountdown?: boolean;
  compact?: boolean;
  language?: LanguagePreference;
}

export function formatCountdown(resetAt: number | null, language?: LanguagePreference) {
  if (!resetAt) return translate(language, "resetUnavailable");
  const seconds = Math.max(0, resetAt - Math.floor(Date.now() / 1000));
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  return days > 0
    ? translate(language, "resetsDays", { days, hours })
    : translate(language, "resetsHours", { hours, minutes });
}

export function formatResetTime(resetAt: number | null, language?: LanguagePreference) {
  if (!resetAt) return "—";
  return new Intl.DateTimeFormat(localeName(language), {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(resetAt * 1000));
}

export function UsageCard({ title, shortTitle, window, showCountdown = true, compact, language }: Props) {
  if (!window) {
    return (
      <section className={`usage-card ${compact ? "usage-card--compact" : ""}`}>
        <div className="usage-card__heading">
          <span>{compact ? shortTitle : title}</span>
          <strong>{translate(language, "quotaUnavailable")}</strong>
        </div>
        {!compact && <p className="muted">{translate(language, "quotaMissing")}</p>}
      </section>
    );
  }

  return (
    <section className={`usage-card ${compact ? "usage-card--compact" : ""}`}>
      <div className="usage-card__heading">
        <span>{compact ? shortTitle : title}</span>
        <strong>{Math.round(window.remainingPercent)}% {translate(language, "remaining")}</strong>
      </div>
      <div className="progress" aria-label={`${title} ${window.remainingPercent}% remaining`}>
        <span style={{ width: `${window.remainingPercent}%` }} />
      </div>
      {!compact && showCountdown && (
        <div className="usage-card__meta">
          <span>{formatCountdown(window.resetAt, language)}</span>
          <span>{formatResetTime(window.resetAt, language)}</span>
        </div>
      )}
    </section>
  );
}
