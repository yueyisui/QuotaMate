import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { backend } from "../services/backend";
import { resolvedLanguage } from "../i18n";
import type { AppConfig, CodexUsage, RuntimeStatus } from "../types";

export function useAppData() {
  const [usage, setUsage] = useState<CodexUsage | null>(null);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [appVersion, setAppVersion] = useState("0.1.0");
  const [error, setError] = useState<string | null>(null);

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
        setConfig(nextConfig);
        setRuntime(nextRuntime);
        setAppVersion(version);
      })
      .catch((reason) => mounted && setError(String(reason)));

    const unlisteners = [
      listen<CodexUsage>("usage-updated", ({ payload }) => setUsage(payload)),
      listen<AppConfig>("config-updated", ({ payload }) => setConfig(payload)),
      listen<RuntimeStatus>("runtime-status-updated", ({ payload }) =>
        setRuntime(payload),
      ),
    ];
    return () => {
      mounted = false;
      void Promise.all(unlisteners).then((items) => items.forEach((unlisten) => unlisten()));
    };
  }, []);

  const saveConfig = useCallback(async (next: AppConfig) => {
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

  const refresh = useCallback(async () => {
    try {
      setError(null);
      await backend.refreshUsage();
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  return { usage, config, runtime, appVersion, error, setError, setConfig, saveConfig, refresh };
}
