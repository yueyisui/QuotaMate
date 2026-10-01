import type { CodexUsage, UsageWindow } from "../types";

// Select by the actual snapshot, never by a plan name or an assumed entitlement.
export function quotaWindows(usage: CodexUsage, showFiveHour = true, showWeekly = true): UsageWindow[] {
  if (!showFiveHour && !showWeekly) return [];
  const all = [usage.fiveHour, usage.weekly, ...(usage.otherWindows ?? [])]
    .filter((window): window is UsageWindow => window !== null && window !== undefined);
  const selected = all.filter((window) => window.windowMinutes === 300
    ? showFiveHour : window.windowMinutes === 10080 ? showWeekly : true);
  return selected.length ? selected : all;
}

export function quotaShortLabel(window: UsageWindow): string {
  if (window.windowMinutes === 10080) return "W";
  return window.windowMinutes % 60 === 0 ? `${window.windowMinutes / 60}h` : `${window.windowMinutes}m`;
}
