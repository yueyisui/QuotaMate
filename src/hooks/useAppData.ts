import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { backend } from "../services/backend";
import { resolvedLanguage } from "../i18n";
import { createConfigSaveQueue } from "../services/configSaveQueue";
import type { AppConfig, CodexUsage, RuntimeStatus } from "../types";

export function useAppData() {
  const [usage, setUsage] = useState<CodexUsage | null>(null);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [appVersion, setAppVersion] = useState("");
  const [error, setError] = useState<string | null>(null);
  const linuxSaves = useRef<ReturnType<typeof createConfigSaveQueue<AppConfig>> | null>(null);
  if (__QUOTAMATE_PLATFORM__ === "linux" && !linuxSaves.current) {
    linuxSaves.current = createConfigSaveQueue(
      backend.getConfig, backend.saveConfig, setConfig,
      (reason) => setError(reason === null ? null : String(reason)),
    );
  }

  const receiveConfig = useCallback((next: AppConfig) => {
    if (linuxSaves.current) linuxSaves.current.receive(next);
    else setConfig(next);
  }, []);

  useEffect(() => {
    document.documentElement.lang = resolvedLanguage(config?.language);
  }, [config?.language]);

  useEffect(() => {
    let mounted = true;
    Promise.all([
      backend.getUsage(),
      backend.getConfig(),
      backend.getRuntimeStatus(),
      backend.getAppVersion(),
    ])
      .then(([nextUsage, nextConfig, nextRuntime, version]) => {
        if (!mounted) return;
        setUsage(nextUsage);
        receiveConfig(nextConfig);
        setRuntime(nextRuntime);
        setAppVersion(version);
      })
      .catch((reason) => mounted && setError(String(reason)));

    const unlisteners = [
      listen<CodexUsage>("usage-updated", ({ payload }) => setUsage(payload)),
      listen<AppConfig>("config-updated", ({ payload }) => receiveConfig(payload)),
      listen<RuntimeStatus>("runtime-status-updated", ({ payload }) =>
        setRuntime(payload),
      ),
    ];
    return () => {
      mounted = false;
      void Promise.all(unlisteners).then((items) => items.forEach((unlisten) => unlisten()));
    };
  }, [receiveConfig]);

  const saveConfig = useCallback(async (next: AppConfig) => {
    if (linuxSaves.current) return linuxSaves.current.patch(next);
    setConfig(next);
    try {
      const saved = await backend.saveConfig(next);
      setConfig(saved);
      setError(null);
      return saved;
    } catch (reason) {
      setError(String(reason));
      const restored = await backend.getConfig();
      setConfig(restored);
      throw reason;
    }
  }, []);

  const patchConfig = useCallback((change: Partial<AppConfig>) => {
    if (!linuxSaves.current) throw new Error("Partial config saves are Linux only");
    return linuxSaves.current.patch(change);
  }, []);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      await backend.refreshUsage();
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  return { usage, config, runtime, appVersion, error, setError, setConfig: receiveConfig, saveConfig, patchConfig, refresh };
}
