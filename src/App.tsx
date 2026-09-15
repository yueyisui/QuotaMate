import { useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";
import { UsageCard } from "./components/UsageCard";
import { Toggle } from "./components/Toggle";
import { energyState, PetAvatar, PET_PRESETS, usageEnergy } from "./components/PetAvatar";
import type { BuiltInPet } from "./components/PetAvatar";
import { useAppData } from "./hooks/useAppData";
import { localeName, petName, translator, type LanguagePreference } from "./i18n";
import { backend } from "./services/backend";
import type { AppConfig, CodexUsage, PetImageHistoryEntry, Trigger } from "./types";

type Page = "usage" | "scheduler" | "settings" | "about";
const IS_MACOS = navigator.userAgent.includes("Mac OS X");

type PressGesture = {
  pointerId: number;
  startX: number;
  startY: number;
  dragging: boolean;
  draggable: boolean;
  timer: number | null;
};

function useClickOrDrag(enabled: boolean, draggable: boolean, onClick: () => void) {
  const gesture = useRef<PressGesture | null>(null);
  const beginDrag = (current: PressGesture | null) => {
    if (!current || current.dragging || !current.draggable) return;
    current.dragging = true;
    if (current.timer !== null) window.clearTimeout(current.timer);
    current.timer = null;
    void getCurrentWindow().startDragging();
  };
  const cancel = () => {
    if (gesture.current?.timer !== null && gesture.current?.timer !== undefined) {
      window.clearTimeout(gesture.current.timer);
    }
    gesture.current = null;
  };
  return {
    onPointerDown: (event: ReactPointerEvent<HTMLElement>) => {
      if (!enabled || event.button !== 0 || (event.target as Element).closest("[data-no-window-gesture]")) return;
      cancel();
      const current: PressGesture = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        dragging: false,
        draggable,
        timer: null,
      };
      if (draggable) current.timer = window.setTimeout(() => beginDrag(current), 180);
      gesture.current = current;
      event.preventDefault();
    },
    onPointerMove: (event: ReactPointerEvent<HTMLElement>) => {
      const current = gesture.current;
      if (!current || current.pointerId !== event.pointerId || !current.draggable) return;
      if (Math.hypot(event.clientX - current.startX, event.clientY - current.startY) >= 4) beginDrag(current);
    },
    onPointerUp: (event: ReactPointerEvent<HTMLElement>) => {
      const current = gesture.current;
      if (!current || current.pointerId !== event.pointerId) return;
      const wasDragging = current.dragging;
      cancel();
      if (!wasDragging) onClick();
    },
    onPointerCancel: cancel,
  };
}

function createId() {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`;
}

function nextTrigger(config: AppConfig | null, none = "None") {
  const enabled = config?.schedulerEnabled
    ? config.triggers.filter((trigger) => trigger.enabled)
    : [];
  if (!enabled.length) return none;
  const current = new Date();
  const nowMinutes = current.getHours() * 60 + current.getMinutes();
  return [...enabled]
    .sort((a, b) => {
      const aMinutes = Number(a.time.slice(0, 2)) * 60 + Number(a.time.slice(3));
      const bMinutes = Number(b.time.slice(0, 2)) * 60 + Number(b.time.slice(3));
      return (aMinutes - nowMinutes + 1440) % 1440 - (bMinutes - nowMinutes + 1440) % 1440;
    })[0].time;
}

function formatLastUpdated(timestamp: number | null | undefined, language?: LanguagePreference) {
  return timestamp
    ? new Date(timestamp * 1000).toLocaleTimeString(localeName(language), { hour: "2-digit", minute: "2-digit", second: "2-digit" })
    : "—";
}

function formatDateTime(timestamp: number | null | undefined, language?: LanguagePreference) {
  return timestamp
    ? new Date(timestamp * 1000).toLocaleString(localeName(language), { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })
    : "—";
}

function formatPlan(planType: string | null | undefined, unavailable: string) {
  if (!planType) return unavailable;
  const label = planType
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
    .join(" ");
  return label ? `ChatGPT ${label}` : unavailable;
}

function ResetCreditsCard({ credits, language, compact = false }: {
  credits: CodexUsage["rateLimitResetCredits"];
  language?: LanguagePreference;
  compact?: boolean;
}) {
  const t = translator(language);
  const details = credits?.credits ?? null;
  return (
    <section className={`reset-credits-card ${compact ? "reset-credits-card--compact" : ""}`}>
      <div className="reset-credits-card__heading">
        <div><span className="reset-card-icon">↻</span><strong>{t("resetCredits")}</strong></div>
        <strong>{credits ? t("resetCardsAvailable", { count: credits.availableCount }) : "—"}</strong>
      </div>
      {!credits ? <p>{t("resetCreditsUnavailable")}</p> : details === null ? <p>{t("creditDetailsUnavailable")}</p> : details.length > 0 ? (
        <div className="reset-credit-list">
          {details.map((item) => (
            <div className="reset-credit-row" key={item.id}>
              <span><strong>{item.title || t("resetCredits")}</strong><small>{item.status === "available" ? t("creditAvailable") : item.status}</small></span>
              <span><small>{t("creditGrantedAt")}: {formatDateTime(item.grantedAt, language)}</small><small>{t("creditExpiresAt")}: {item.expiresAt ? formatDateTime(item.expiresAt, language) : t("noExpiry")}</small></span>
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}

function UsageContent() {
  const { usage, config, error, refresh } = useAppData();
  const t = translator(config?.language);
  if (!usage || !config) return <Loading />;
  return (
    <div className="page-content">
      <div className="page-title-row">
        <div><p className="eyebrow">{t("liveSnapshot")}</p><div className="usage-title"><h1>{t("codexUsage")}</h1><span className="plan-badge">{formatPlan(usage.planType, t("planUnavailable"))}</span></div></div>
        <button className="button button--primary" onClick={refresh}>↻ {t("refresh")}</button>
      </div>
      {usage.status !== "available" && (
        <div className={`status-banner status-banner--${usage.status}`}>
          <span className="status-dot" />
          <div><strong>{usage.status === "connecting" ? t("connecting") : t("unavailable")}</strong><small>{usage.error ?? t("waitingSnapshot")}</small></div>
        </div>
      )}
      {error && <div className="status-banner status-banner--unavailable">{error}</div>}
      <div className="usage-stack">
        {config.showFiveHour && <UsageCard title={t("fiveHour")} shortTitle="5h" window={usage.fiveHour} showCountdown={config.showResetCountdown} language={config.language} />}
        {config.showWeekly && <UsageCard title={t("weekly")} shortTitle="W" window={usage.weekly} showCountdown={config.showResetCountdown} language={config.language} />}
      </div>
      <ResetCreditsCard credits={usage.rateLimitResetCredits} language={config.language} />
      <div className="facts-grid">
        <div><span>{t("nextTrigger")}</span><strong>{nextTrigger(config, t("none"))}</strong></div>
        <div><span>{t("lastUpdated")}</span><strong>{formatLastUpdated(usage.lastUpdated, config.language)}</strong></div>
      </div>
    </div>
  );
}

function SchedulerPage() {
  const { config, saveConfig, error } = useAppData();
  const [time, setTime] = useState("05:00");
  const [label, setLabel] = useState("");
  const t = translator(config?.language);
  if (!config) return <Loading />;
  const updateTriggers = (triggers: Trigger[]) => void saveConfig({ ...config, triggers });
  const addTrigger = () => updateTriggers([...config.triggers, {
    id: createId(), enabled: true, time, label: label.trim() || t("codexSession"),
    lastRun: null, lastRunDate: null, lastStatus: null, lastResult: null,
  }]);
  const updateTrigger = (id: string, patch: Partial<Trigger>) =>
    updateTriggers(config.triggers.map((item) => item.id === id ? { ...item, ...patch } : item));
  const statusLabel = (status: string | null) => {
    if (!status) return t("neverRun");
    if (status === "success") return t("statusSuccess");
    if (status === "running") return t("statusRunning");
    if (status === "timed_out") return t("statusTimedOut");
    return t("statusFailed");
  };

  return (
    <div className="page-content">
      <div className="page-title-row">
        <div><p className="eyebrow">{t("automation")}</p><h1>{t("sessionScheduler")}</h1></div>
        <Toggle label={t("scheduler")} checked={config.schedulerEnabled} onChange={(schedulerEnabled) => void saveConfig({ ...config, schedulerEnabled })} />
      </div>
      <p className="page-lead">{t("schedulerLead")}</p>
      {error && <div className="status-banner status-banner--unavailable">{error}</div>}
      <section className="panel trigger-list">
        {config.triggers.length === 0 && <div className="empty-state"><span>◷</span><strong>{t("noTriggers")}</strong><p>{t("addFirst")}</p></div>}
        {config.triggers.map((trigger) => (
          <div className="trigger-row" key={trigger.id}>
            <input className="switch" type="checkbox" checked={trigger.enabled} onChange={(event) => updateTrigger(trigger.id, { enabled: event.target.checked })} />
            <input type="time" value={trigger.time} onChange={(event) => updateTrigger(trigger.id, { time: event.target.value })} />
            <input value={trigger.label} maxLength={60} onChange={(event) => updateTrigger(trigger.id, { label: event.target.value })} />
            <div className="trigger-result"><span className={`result-pill result-pill--${trigger.lastStatus ?? "never"}`}>{statusLabel(trigger.lastStatus)}</span>{trigger.lastResult && <small title={trigger.lastResult}>{trigger.lastResult}</small>}</div>
            <button className="icon-button danger" aria-label={`${t("delete")} ${trigger.label}`} onClick={() => updateTriggers(config.triggers.filter((item) => item.id !== trigger.id))}>×</button>
          </div>
        ))}
      </section>
      <section className="panel add-trigger">
        <h2>{t("addTrigger")}</h2>
        <input type="time" value={time} onChange={(event) => setTime(event.target.value)} />
        <input value={label} maxLength={60} placeholder={t("label")} onChange={(event) => setLabel(event.target.value)} />
        <button className="button button--primary" onClick={addTrigger}>＋ {t("addTrigger")}</button>
      </section>
      <p className="privacy-note">{t("schedulerPrivacy")}</p>
    </div>
  );
}

function SettingsPage() {
  const { config, saveConfig, setConfig, error, setError } = useAppData();
  const [petHistory, setPetHistory] = useState<PetImageHistoryEntry[]>([]);
  const t = translator(config?.language);
  useEffect(() => {
    void backend.getPetImageHistory().then(setPetHistory).catch((reason) => setError(String(reason)));
  }, [config?.petImage, setError]);
  if (!config) return <Loading />;
  const patch = (change: Partial<AppConfig>) => void saveConfig({ ...config, ...change });
  const choosePet = async () => {
    try {
      const source = await open({ multiple: false, filters: [{ name: t("customPet"), extensions: ["png", "webp", "gif"] }] });
      if (source) setConfig(await backend.importPetImage(source));
    } catch (reason) { setError(String(reason)); }
  };
  const selectPetHistory = async (id: string) => {
    try { setConfig(await backend.selectPetImage(id)); }
    catch (reason) { setError(String(reason)); }
  };
  const deletePetHistory = async (id: string) => {
    if (!window.confirm(t("deleteImageConfirm"))) return;
    try {
      setConfig(await backend.deletePetImage(id));
      setPetHistory(await backend.getPetImageHistory());
    } catch (reason) { setError(String(reason)); }
  };
  return (
    <div className="page-content settings-page">
      <div><p className="eyebrow">{t("preferences")}</p><h1>{t("settings")}</h1></div>
      {error && <div className="status-banner status-banner--unavailable">{error}</div>}
      <SettingsSection title={t("general")}>
        <label className="setting-row"><span><strong>{t("language")}</strong><small>{t("languageDesc")}</small></span><select value={config.language} onChange={(event) => patch({ language: event.target.value as AppConfig["language"] })}><option value="system">{t("systemDefault")}</option><option value="zh-CN">{t("chinese")}</option><option value="en">{t("english")}</option></select></label>
        <Toggle label={t("launchStartup")} description={t("disabledDefault")} checked={config.launchAtStartup} onChange={(launchAtStartup) => patch({ launchAtStartup })} />
        <Toggle label={t("startMinimized")} checked={config.startMinimized} onChange={(startMinimized) => patch({ startMinimized })} />
        <Toggle label={t("minimizeTray")} checked={config.minimizeToTray} onChange={(minimizeToTray) => patch({ minimizeToTray })} />
        <Toggle label={t("runBackground")} checked={config.runInBackground} onChange={(runInBackground) => patch({ runInBackground })} />
      </SettingsSection>
      <SettingsSection title={t("usage")}>
        <label className="setting-row"><span><strong>{t("refreshInterval")}</strong><small>{t("notificationsImmediate")}</small></span>
          <select value={config.refreshInterval} onChange={(event) => patch({ refreshInterval: Number(event.target.value) })}>
            <option value={30}>{t("seconds30")}</option><option value={60}>{t("minute1")}</option><option value={120}>{t("minutes2")}</option><option value={300}>{t("minutes5")}</option>
          </select>
        </label>
        <Toggle label={t("showFiveHour")} checked={config.showFiveHour} onChange={(showFiveHour) => patch({ showFiveHour })} />
        <Toggle label={t("showWeekly")} checked={config.showWeekly} onChange={(showWeekly) => patch({ showWeekly })} />
        <Toggle label={t("showCountdown")} checked={config.showResetCountdown} onChange={(showResetCountdown) => patch({ showResetCountdown })} />
      </SettingsSection>
      <SettingsSection title={t("display")}>
        <p className="section-note">{t("mutuallyExclusive")}</p>
        <Toggle label={t("enableCompact")} checked={config.compactWidgetEnabled} onChange={(compactWidgetEnabled) => patch({ compactWidgetEnabled, petEnabled: compactWidgetEnabled ? false : config.petEnabled })} />
        <div className="setting-row compact-content-setting"><span><strong>{t("compactContent")}</strong><small>{t(IS_MACOS ? "compactContentDescMac" : "compactContentDesc")}</small></span><div className="segmented-control">
          <button className={config.compactShowFiveHour && !config.compactShowWeekly ? "active" : ""} onClick={() => patch({ compactShowFiveHour: true, compactShowWeekly: false })}>{t("onlyFiveHour")}</button>
          <button className={!config.compactShowFiveHour && config.compactShowWeekly ? "active" : ""} onClick={() => patch({ compactShowFiveHour: false, compactShowWeekly: true })}>{t("onlyWeekly")}</button>
          <button className={config.compactShowFiveHour && config.compactShowWeekly ? "active" : ""} onClick={() => patch({ compactShowFiveHour: true, compactShowWeekly: true })}>{t("showAll")}</button>
        </div></div>
        {!IS_MACOS && <Toggle label={t("lockCompact")} checked={config.compactWidgetLocked} onChange={(compactWidgetLocked) => patch({ compactWidgetLocked })} />}
        <Toggle label={t("enablePet")} checked={config.petEnabled} onChange={(petEnabled) => patch({ petEnabled, compactWidgetEnabled: petEnabled ? false : config.compactWidgetEnabled })} />
        <Toggle label={t("petAlwaysTop")} checked={config.petAlwaysOnTop} onChange={(petAlwaysOnTop) => patch({ petAlwaysOnTop })} />
        <div className="pet-picker">
          <div className="pet-picker__title"><strong>{t("builtInPets")}</strong><small>{t("petStateDesc")}</small></div>
          <div className="pet-preset-grid">
            {PET_PRESETS.map((preset) => <button key={preset.id} className={`pet-preset ${config.petPreset === preset.id ? "pet-preset--active" : ""}`} onClick={() => patch({ petPreset: preset.id, petEnabled: true, compactWidgetEnabled: false })}><PetAvatar preset={preset.id} energy={68} language={config.language} /><span>{petName(config.language, preset.id)}</span></button>)}
          </div>
        </div>
        <label className="setting-row"><span><strong>{t("customPet")}</strong><small>{config.petPreset === "custom" && config.petImage ? t("currentUsing") : config.petImage ? t("savedSelect") : t("formats")}</small></span><button className="button" onClick={choosePet}>{t("chooseImage")}</button></label>
        {petHistory.length > 0 && <div className="pet-history">
          <div className="pet-picker__title"><strong>{t("customPetHistory")}</strong><small>{t("customPetHistoryDesc")}</small></div>
          <div className="pet-history-grid">
            {petHistory.map((item) => <div className={`pet-history-item ${item.active ? "pet-history-item--active" : ""}`} key={item.id}>
              <button className="pet-history-select" onClick={() => void selectPetHistory(item.id)} title={item.importedAt ? formatDateTime(item.importedAt, config.language) : t("importedEarlier")}><img src={item.dataUrl} alt={t("customPetAlt")} /><span>{item.importedAt ? formatDateTime(item.importedAt, config.language) : t("importedEarlier")}</span></button>
              <button className="pet-history-remove" title={t("removeImage")} aria-label={t("removeImage")} onClick={() => void deletePetHistory(item.id)}>×</button>
            </div>)}
          </div>
        </div>}
        <label className="setting-row"><span><strong>{t("petScale")}</strong><small>{Math.round(config.petScale * 100)}%</small></span><input type="range" min="0.7" max="1.5" step="0.1" value={config.petScale} onChange={(event) => patch({ petScale: Number(event.target.value) })} /></label>
        <label className="setting-row"><span><strong>{t("opacity")}</strong><small>{Math.round(config.opacity * 100)}%</small></span><input type="range" min="0.35" max="1" step="0.05" value={config.opacity} onChange={(event) => patch({ opacity: Number(event.target.value) })} /></label>
      </SettingsSection>
    </div>
  );
}

function SettingsSection({ title, children }: { title: string; children: React.ReactNode }) {
  return <section className="settings-section"><h2>{title}</h2><div className="panel">{children}</div></section>;
}

function AboutPage() {
  const { runtime, appVersion, config } = useAppData();
  const t = translator(config?.language);
  return (
    <div className="page-content about-page">
      <div className="app-mark app-mark--large">Q</div><h1>QuotaMate</h1><p className="page-lead">{t("aboutLead")}</p>
      <section className="panel about-list">
        <div><span>{t("appVersion")}</span><strong>{appVersion}</strong></div>
        <div><span>{t("codexCli")}</span><strong>{runtime?.cliVersion ?? t("notDetected")}</strong></div>
        <div><span>{t("appServer")}</span><strong className={`connection connection--${runtime?.appServerStatus}`}>{runtime?.appServerStatus === "connected" ? t("connected") : runtime?.appServerStatus === "reconnecting" ? t("reconnecting") : runtime?.appServerStatus === "unavailable" ? t("unavailable") : t("starting")}</strong></div>
      </section>
      {runtime?.lastError && <div className="status-banner status-banner--unavailable">{runtime.lastError}</div>}
      <button className="button" onClick={() => void backend.openLogFolder()}>{t("openLogs")}</button>
      <p className="privacy-note">{t("aboutPrivacy")}</p>
    </div>
  );
}

function MainApp() {
  const [page, setPage] = useState<Page>("usage");
  const { config } = useAppData();
  const t = translator(config?.language);
  useEffect(() => { const promise = listen<Page>("navigate", ({ payload }) => setPage(payload)); return () => { void promise.then((unlisten) => unlisten()); }; }, []);
  const content = page === "usage" ? <UsageContent /> : page === "scheduler" ? <SchedulerPage /> : page === "settings" ? <SettingsPage /> : <AboutPage />;
  return (
    <div className="app-shell"><aside className="sidebar"><div className="brand"><span className="app-mark">Q</span><span>QuotaMate</span></div><nav>
      <NavButton active={page === "usage"} icon="◔" label={t("usage")} onClick={() => setPage("usage")} />
      <NavButton active={page === "scheduler"} icon="◷" label={t("scheduler")} onClick={() => setPage("scheduler")} />
      <NavButton active={page === "settings"} icon="⚙" label={t("settings")} onClick={() => setPage("settings")} />
    </nav><NavButton active={page === "about"} icon="ⓘ" label={t("about")} onClick={() => setPage("about")} /></aside><main className="main-panel">{content}</main></div>
  );
}

function NavButton({ active, icon, label, onClick }: { active: boolean; icon: string; label: string; onClick: () => void }) {
  return <button className={`nav-button ${active ? "nav-button--active" : ""}`} onClick={onClick}><span>{icon}</span>{label}</button>;
}

function CompactWidget() {
  const { usage, config, saveConfig } = useAppData();
  const [expanded, setExpanded] = useState(false);
  const t = translator(config?.language);
  useEffect(() => { const promise = listen("compact-collapse", () => setExpanded(false)); return () => { void promise.then((unlisten) => unlisten()); }; }, []);
  const expand = (next: boolean) => {
    if (expanded === next) return;
    setExpanded(next);
    void backend.setCompactExpanded(next);
  };
  const collapsedGesture = useClickOrDrag(!expanded, !config?.compactWidgetLocked, () => expand(true));
  if (!usage || !config) return <Loading />;
  const value = (amount: number | undefined) => amount === undefined ? "—" : `${Math.round(amount)}%`;
  return (
    <div className={`compact-widget floating-surface ${expanded ? "compact-widget--expanded" : ""} ${config.compactWidgetLocked ? "compact-widget--locked" : ""}`} style={{ opacity: config.opacity }} {...collapsedGesture} onMouseLeave={() => expand(false)} onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); void backend.showFloatingContextMenu("compact"); }}>
      {!expanded ? <div className="compact-collapsed">
        <span className="compact-logo">C</span>
        {config.compactShowFiveHour && <span>5h <strong>{value(usage.fiveHour?.remainingPercent)}</strong></span>}
        {config.compactShowFiveHour && config.compactShowWeekly && <i />}
        {config.compactShowWeekly && <span>W <strong>{value(usage.weekly?.remainingPercent)}</strong></span>}
      </div> : <div className="compact-details">
        <div className="compact-details__bar" data-tauri-drag-region><div><span className="compact-logo">C</span><strong>{t("codexUsage")}</strong><span className="plan-badge plan-badge--compact">{formatPlan(usage.planType, t("planUnavailable"))}</span></div><div>
          <button title={t("collapse")} onClick={(event) => { event.stopPropagation(); expand(false); }}>⌄</button>
          <button title={config.compactWidgetLocked ? t("unlock") : t("lock")} onClick={(event) => { event.stopPropagation(); void saveConfig({ ...config, compactWidgetLocked: !config.compactWidgetLocked }); }}>{config.compactWidgetLocked ? "●" : "○"}</button>
          <button title={t("close")} onClick={(event) => { event.stopPropagation(); void backend.disableDisplay("compact"); }}>×</button>
        </div></div>
        <UsageCard title={t("fiveHour")} shortTitle="5h" window={usage.fiveHour} showCountdown={config.showResetCountdown} language={config.language} />
        <UsageCard title={t("weekly")} shortTitle="W" window={usage.weekly} showCountdown={config.showResetCountdown} language={config.language} />
        <ResetCreditsCard credits={usage.rateLimitResetCredits} language={config.language} compact />
        <div className="compact-facts"><div><span>{t("nextTrigger")}</span><strong>{nextTrigger(config, t("none"))}</strong></div><div><span>{t("lastUpdated")}</span><strong>{formatLastUpdated(usage.lastUpdated, config.language)}</strong></div></div>
        <button className="text-button" onClick={(event) => { event.stopPropagation(); void backend.showWindow("settings"); }}>{t("settings")}</button>
      </div>}
    </div>
  );
}

function MenuBarPanel() {
  const { usage, config, refresh } = useAppData();
  const t = translator(config?.language);
  if (!usage || !config) return <Loading />;
  const openSettings = () => {
    void backend.hideMenuBarPanel().then(() => backend.showWindow("settings"));
  };
  return (
    <div className="menu-bar-panel floating-surface" onMouseLeave={() => void backend.hideMenuBarPanel()}>
      <div className="menu-bar-panel__header">
        <div><span className="compact-logo">C</span><strong>{t("codexUsage")}</strong><span className="plan-badge plan-badge--compact">{formatPlan(usage.planType, t("planUnavailable"))}</span></div>
        <div><button title={t("refresh")} onClick={refresh}>↻</button><button title={t("close")} onClick={() => void backend.hideMenuBarPanel()}>×</button></div>
      </div>
      <UsageCard title={t("fiveHour")} shortTitle="5h" window={usage.fiveHour} showCountdown={config.showResetCountdown} language={config.language} />
      <UsageCard title={t("weekly")} shortTitle="W" window={usage.weekly} showCountdown={config.showResetCountdown} language={config.language} />
      <ResetCreditsCard credits={usage.rateLimitResetCredits} language={config.language} compact />
      <div className="compact-facts"><div><span>{t("nextTrigger")}</span><strong>{nextTrigger(config, t("none"))}</strong></div><div><span>{t("lastUpdated")}</span><strong>{formatLastUpdated(usage.lastUpdated, config.language)}</strong></div></div>
      <button className="text-button" onClick={openSettings}>{t("settings")}</button>
    </div>
  );
}

function DesktopPet() {
  const { usage, config, refresh } = useAppData();
  const [expanded, setExpanded] = useState(false);
  const [image, setImage] = useState<string | null>(null);
  const t = translator(config?.language);
  useEffect(() => { void backend.getPetImageData().then(setImage).catch(() => setImage(null)); }, [config?.petImage]);
  useEffect(() => { const promise = listen("pet-collapse", () => setExpanded(false)); return () => { void promise.then((unlisten) => unlisten()); }; }, []);
  const toggleExpanded = () => {
    const next = !expanded;
    setExpanded(next);
    void backend.setPetExpanded(next);
  };
  const collapseExpanded = () => {
    if (!expanded) return;
    setExpanded(false);
    void backend.setPetExpanded(false);
  };
  const collapsedGesture = useClickOrDrag(!expanded, true, toggleExpanded);
  if (!usage || !config) return <Loading />;
  const energy = usageEnergy(usage);
  const state = energyState(energy, config.language);
  const preset = config.petPreset === "custom" ? "cat" : config.petPreset as BuiltInPet;
  const usesCustomImage = config.petPreset === "custom" && image;
  return (
    <div className={`pet-window pet-window--${state.key} ${expanded ? "pet-window--expanded" : ""}`} style={{ opacity: config.opacity, "--pet-energy": state.color } as React.CSSProperties} {...collapsedGesture} onMouseLeave={collapseExpanded} onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); void backend.showFloatingContextMenu("pet"); }}>
      <div className="pet-toolbar" data-tauri-drag-region={expanded ? true : undefined}><span>⠿</span><button data-no-window-gesture title={t("close")} onClick={() => void backend.disableDisplay("pet")}>×</button></div>
      {!expanded && <div className="pet-quota pet-quota--weekly"><span>{t("weekly")}</span><strong>{usage.weekly ? `${Math.round(usage.weekly.remainingPercent)}%` : "—"}</strong></div>}
      <button className="pet-character" aria-label={t("codexUsage")}>{usesCustomImage ? <img className="custom-pet-image" src={image} alt={t("customPetAlt")} /> : <PetAvatar preset={preset} energy={energy} language={config.language} />}</button>
      {!expanded ? <div className="pet-quota pet-quota--five"><span>5h</span><strong>{usage.fiveHour ? `${Math.round(usage.fiveHour.remainingPercent)}%` : "—"}</strong><small>{state.label}</small></div> : (
        <div className="pet-details floating-surface"><div className="pet-details__title"><div><span className="pet-details__heading"><strong>{t("codexUsage")}</strong><span className="plan-badge plan-badge--compact">{formatPlan(usage.planType, t("planUnavailable"))}</span></span><small style={{ color: state.color }}>{state.label} · {Math.round(energy)}%</small></div><div><button title={t("collapse")} onClick={toggleExpanded}>⌄</button><button title={t("refresh")} onClick={refresh}>↻</button></div></div><UsageCard title={t("fiveHour")} shortTitle="5h" window={usage.fiveHour} language={config.language} /><UsageCard title={t("weekly")} shortTitle="W" window={usage.weekly} language={config.language} /><ResetCreditsCard credits={usage.rateLimitResetCredits} language={config.language} compact /><div className="pet-next"><span>{t("nextTrigger")}</span><strong>{nextTrigger(config, t("none"))}</strong></div><button className="text-button" onClick={() => void backend.showWindow("settings")}>{t("settings")}</button></div>
      )}
    </div>
  );
}

function Loading() { return <div className="loading"><span className="spinner" />{translator(undefined)("loading")}</div>; }

function App() {
  const windowType = useMemo(() => new URLSearchParams(location.search).get("window") ?? "main", []);
  if (windowType === "compact") return <CompactWidget />;
  if (windowType === "pet") return <DesktopPet />;
  if (windowType === "menu-panel") return <MenuBarPanel />;
  return <MainApp />;
}

export default App;
