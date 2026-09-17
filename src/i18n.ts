import type { AppConfig } from "./types";

export type LanguagePreference = AppConfig["language"];

const en = {
  liveSnapshot: "LIVE LOCAL SNAPSHOT", codexUsage: "Codex Usage", refresh: "Refresh",
  connecting: "Connecting to Codex", unavailable: "Codex unavailable", waitingSnapshot: "Waiting for the first rate-limit snapshot.",
  fiveHour: "5 Hour", weekly: "Weekly", accountPlan: "Codex plan", planUnavailable: "Not provided", nextTrigger: "Next trigger", lastUpdated: "Last updated", none: "None",
  resetCredits: "Quota reset cards", resetCardsAvailable: "{count} available", resetCreditsUnavailable: "Not provided for this account",
  creditGrantedAt: "Granted", creditExpiresAt: "Expires", noExpiry: "No expiry", creditAvailable: "Available", creditDetailsUnavailable: "Card time details are unavailable.",
  automation: "AUTOMATION", sessionScheduler: "Session Scheduler", scheduler: "Scheduler",
  schedulerLead: "Each enabled time starts one tiny, ephemeral Codex session per local day. Missed times are skipped.",
  noTriggers: "No triggers yet", addFirst: "Add the first daily session below.", neverRun: "Never run", addTrigger: "Add trigger",
  statusSuccess: "Success", statusFailed: "Failed", statusRunning: "Running", statusTimedOut: "Timed out",
  morning: "Morning", codexSession: "Codex session", label: "Label", delete: "Delete",
  schedulerPrivacy: "Runs in QuotaMate's local runtime directory with read-only sandboxing. No retries and no saved Codex conversation.",
  preferences: "PREFERENCES", settings: "Settings", general: "General", launchStartup: "Launch at startup",
  disabledDefault: "Disabled by default", startMinimized: "Start minimized", minimizeTray: "Minimize to tray", runBackground: "Run in background",
  usage: "Usage", refreshInterval: "Refresh interval", notificationsImmediate: "Notifications still update immediately",
  seconds30: "30 seconds", minute1: "1 minute", minutes2: "2 minutes", minutes5: "5 minutes",
  showFiveHour: "Show 5-hour window", showWeekly: "Show weekly window", showCountdown: "Show reset countdown",
  display: "Display", mutuallyExclusive: "Compact Widget and Desktop Pet are mutually exclusive; only one can be active.",
  enableCompact: "Enable Compact Mode", compactContent: "Compact content", compactContentDesc: "Collapsed width follows the selected content.", compactContentDescMac: "The selected values appear directly in the macOS menu bar.",
  onlyFiveHour: "5h only", onlyWeekly: "Weekly only", showAll: "Both", lockCompact: "Lock compact position",
  enablePet: "Enable Desktop Pet", petAlwaysTop: "Pet always on top", builtInPets: "Built-in pets",
  petStateDesc: "Expression and energy effects follow the 5-hour quota, falling back to weekly when unavailable.", customPet: "Custom pet image", currentUsing: "Currently in use",
  savedSelect: "Saved; choose again to use", formats: "Transparent PNG, WebP, and GIF supported", chooseImage: "Choose image…",
  customPetHistory: "Custom image history", customPetHistoryDesc: "Select a previously imported image or remove it.", importedEarlier: "Earlier import", removeImage: "Remove image", deleteImageConfirm: "Remove this image from history and delete its local copy?",
  petScale: "Pet scale", opacity: "Opacity", language: "Language", languageDesc: "System follows your operating system language.",
  systemDefault: "System default", chinese: "简体中文", english: "English",
  about: "About", aboutLead: "A private, local Codex usage monitor and session scheduler.", appVersion: "App version",
  codexCli: "Codex CLI", appServer: "App Server", notDetected: "Not detected", starting: "starting", openLogs: "Open log folder",
  aboutPrivacy: "QuotaMate communicates only with the official Codex CLI on this computer. It never reads, stores, or uploads authentication tokens.",
  connected: "connected", reconnecting: "reconnecting",
  compactDrag: "Drag widget", collapse: "Collapse", unlock: "Unlock position", lock: "Lock position", close: "Close", loading: "Connecting to QuotaMate…",
  remaining: "left", resetUnavailable: "Reset time unavailable", resetsDays: "Resets in {days}d {hours}h", resetsHours: "Resets in {hours}h {minutes}m",
  quotaUnavailable: "Unavailable", quotaMissing: "This quota window was not returned by Codex.",
  energyFull: "Fully charged", energyGood: "Doing well", energyLow: "Low energy", energyEmpty: "Needs charging",
  petCat: "Violet fox", petDog: "Dog", petRocket: "Rocket", petCar: "Car", petRobot: "Robot", petTiga: "Ultraman Tiga", customPetAlt: "Custom desktop pet",
} as const;

type MessageKey = keyof typeof en;

const zh: Record<MessageKey, string> = {
  liveSnapshot: "本机实时数据", codexUsage: "Codex 额度", refresh: "刷新",
  connecting: "正在连接 Codex", unavailable: "Codex 暂不可用", waitingSnapshot: "正在等待首次额度数据。",
  fiveHour: "5 小时", weekly: "一周", accountPlan: "Codex 等级", planUnavailable: "暂未返回", nextTrigger: "下次触发", lastUpdated: "更新时间", none: "无",
  resetCredits: "额度重置卡", resetCardsAvailable: "剩余 {count} 张", resetCreditsUnavailable: "此账号暂未返回重置卡信息",
  creditGrantedAt: "获得时间", creditExpiresAt: "到期时间", noExpiry: "无到期时间", creditAvailable: "可用", creditDetailsUnavailable: "暂未返回重置卡的时间明细。",
  automation: "自动化", sessionScheduler: "会话计划", scheduler: "计划任务",
  schedulerLead: "每个启用的时间每天启动一次轻量临时 Codex 会话；错过的时间不会补执行。",
  noTriggers: "还没有计划", addFirst: "在下方添加第一个每日触发时间。", neverRun: "从未运行", addTrigger: "添加计划",
  statusSuccess: "成功", statusFailed: "失败", statusRunning: "运行中", statusTimedOut: "已超时",
  morning: "早晨", codexSession: "Codex 会话", label: "名称", delete: "删除",
  schedulerPrivacy: "任务在 QuotaMate 本地运行目录中以只读沙箱执行，不重试，也不保存 Codex 对话。",
  preferences: "偏好设置", settings: "设置", general: "常规", launchStartup: "开机时启动",
  disabledDefault: "默认关闭", startMinimized: "启动时最小化", minimizeTray: "最小化到托盘", runBackground: "允许后台运行",
  usage: "额度", refreshInterval: "刷新间隔", notificationsImmediate: "额度通知仍会即时更新",
  seconds30: "30 秒", minute1: "1 分钟", minutes2: "2 分钟", minutes5: "5 分钟",
  showFiveHour: "显示 5 小时额度", showWeekly: "显示一周额度", showCountdown: "显示重置倒计时",
  display: "桌面显示", mutuallyExclusive: "简洁模式和桌面宠物互斥，同时只能启用一种。",
  enableCompact: "启用简洁模式", compactContent: "简洁模式内容", compactContentDesc: "收起后的宽度会跟随所选内容。", compactContentDescMac: "所选额度会直接显示在 macOS 菜单栏。",
  onlyFiveHour: "仅 5h", onlyWeekly: "仅一周", showAll: "全部", lockCompact: "锁定简洁模式位置",
  enablePet: "启用桌面宠物", petAlwaysTop: "宠物保持置顶", builtInPets: "内置宠物",
  petStateDesc: "表情和能量效果以 5 小时额度为准；无 5 小时数据时使用一周额度。", customPet: "自定义宠物图片", currentUsing: "当前正在使用",
  savedSelect: "已保存，可再次选择使用", formats: "支持透明 PNG、WebP 和 GIF", chooseImage: "选择图片…",
  customPetHistory: "自定义图片历史", customPetHistoryDesc: "可重新选择以前导入的图片，或删除本地副本。", importedEarlier: "较早导入", removeImage: "删除图片", deleteImageConfirm: "确定从历史记录中移除并删除这张本地图片吗？",
  petScale: "宠物大小", opacity: "透明度", language: "语言", languageDesc: "跟随系统时使用操作系统显示语言。",
  systemDefault: "跟随系统", chinese: "简体中文", english: "English",
  about: "关于", aboutLead: "私密、本地运行的 Codex 额度监控与会话计划工具。", appVersion: "应用版本",
  codexCli: "Codex CLI", appServer: "应用服务", notDetected: "未检测到", starting: "正在启动", openLogs: "打开日志文件夹",
  aboutPrivacy: "QuotaMate 只与本机官方 Codex CLI 通信，不会读取、保存或上传身份验证令牌。",
  connected: "已连接", reconnecting: "正在重连",
  compactDrag: "拖动简洁条", collapse: "收起", unlock: "解锁位置", lock: "锁定位置", close: "关闭", loading: "正在连接 QuotaMate…",
  remaining: "剩余", resetUnavailable: "暂无重置时间", resetsDays: "{days} 天 {hours} 小时后重置", resetsHours: "{hours} 小时 {minutes} 分钟后重置",
  quotaUnavailable: "不可用", quotaMissing: "Codex 没有返回这项额度数据。",
  energyFull: "能量充足", energyGood: "状态良好", energyLow: "能量偏低", energyEmpty: "需要充能",
  petCat: "紫色小狐", petDog: "小狗", petRocket: "火箭", petCar: "汽车", petRobot: "机器人", petTiga: "迪迦奥特曼", customPetAlt: "自定义桌面宠物",
};

export function resolvedLanguage(preference: LanguagePreference | null | undefined): "zh-CN" | "en" {
  if (preference === "zh-CN" || preference === "en") return preference;
  return navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}

export function translate(preference: LanguagePreference | null | undefined, key: MessageKey, values?: Record<string, string | number>) {
  let message = (resolvedLanguage(preference) === "zh-CN" ? zh : en)[key];
  for (const [name, value] of Object.entries(values ?? {})) message = message.split(`{${name}}`).join(String(value));
  return message;
}

export function translator(preference: LanguagePreference | null | undefined) {
  return (key: MessageKey, values?: Record<string, string | number>) => translate(preference, key, values);
}

export function localeName(preference: LanguagePreference | null | undefined) {
  return resolvedLanguage(preference) === "zh-CN" ? "zh-CN" : "en-US";
}

export function petName(preference: LanguagePreference | null | undefined, id: string) {
  const key = ({ cat: "petCat", dog: "petDog", rocket: "petRocket", car: "petCar", robot: "petRobot", tiga: "petTiga" } as const)[id as "cat"];
  return key ? translate(preference, key) : id;
}
