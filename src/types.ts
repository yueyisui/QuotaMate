export interface UsageWindow {
  usedPercent: number;
  remainingPercent: number;
  resetAt: number | null;
  windowMinutes: number;
}

export interface CodexUsage {
  fiveHour: UsageWindow | null;
  weekly: UsageWindow | null;
  planType: string | null;
  rateLimitResetCredits: RateLimitResetCredits | null;
  lastUpdated: number | null;
  status: "connecting" | "available" | "unavailable";
  error: string | null;
}

export interface RateLimitResetCredit {
  id: string;
  resetType: string;
  status: string;
  grantedAt: number | null;
  expiresAt: number | null;
  title: string | null;
  description: string | null;
}

export interface RateLimitResetCredits {
  availableCount: number;
  credits: RateLimitResetCredit[] | null;
}

export interface PetImageRecord {
  id: string;
  path: string;
  importedAt: number;
}

export interface PetImageHistoryEntry {
  id: string;
  importedAt: number;
  dataUrl: string;
  active: boolean;
}

export interface WindowPosition {
  x: number;
  y: number;
}

export interface Trigger {
  id: string;
  enabled: boolean;
  time: string;
  label: string;
  lastRun: number | null;
  lastRunDate: string | null;
  lastStatus: string | null;
  lastResult: string | null;
}

export interface AppConfig {
  configVersion: number;
  language: "system" | "zh-CN" | "en";
  launchAtStartup: boolean;
  startMinimized: boolean;
  minimizeToTray: boolean;
  runInBackground: boolean;
  compactWidgetEnabled: boolean;
  compactWidgetLocked: boolean;
  compactShowFiveHour: boolean;
  compactShowWeekly: boolean;
  compactPosition: WindowPosition | null;
  petEnabled: boolean;
  petImage: string | null;
  petImageHistory: PetImageRecord[];
  petPreset: "cat" | "dog" | "rocket" | "car" | "robot" | "custom";
  petScale: number;
  petPosition: WindowPosition | null;
  petAlwaysOnTop: boolean;
  opacity: number;
  refreshInterval: number;
  showFiveHour: boolean;
  showWeekly: boolean;
  showResetCountdown: boolean;
  schedulerEnabled: boolean;
  triggers: Trigger[];
}

export interface RuntimeStatus {
  cliDetected: boolean;
  cliVersion: string | null;
  appServerStatus: string;
  lastError: string | null;
}
