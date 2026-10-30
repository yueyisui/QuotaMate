/** Serialize Linux settings writes while keeping newer edits visible. */
export function createConfigSaveQueue<T extends object>(
  read: () => Promise<T>,
  write: (next: T) => Promise<T>,
  onSnapshot: (next: T) => void,
  onError: (reason: unknown | null) => void,
) {
  let snapshot: T | null = null;
  let confirmed: T | null = null;
  let pending: Partial<T> | null = null;
  let active: Promise<T> | null = null;

  const publish = (next: T) => {
    snapshot = next;
    onSnapshot(next);
  };

  const drain = async () => {
    let committed = confirmed!;
    let retried = false;
    for (;;) {
      if (!pending) {
        // Include changes made by the scheduler or native menu during a save.
        try { committed = await read(); } catch { /* Keep the successful reply. */ }
        if (!pending) {
          confirmed = committed;
          publish(committed);
          onError(null);
          return committed;
        }
      }
      const changes = pending!;
      pending = null;
      try {
        const base = await read();
        committed = base;
        committed = await write({ ...base, ...changes });
        confirmed = committed;
        retried = false;
        // A reply for an earlier edit must not replace newer optimistic edits.
        publish({ ...committed, ...(pending as Partial<T> | null) });
      } catch (reason) {
        try { committed = await read(); } catch { /* Restore the last known state. */ }
        if (pending && !retried) {
          // Preserve edits queued while the failed write was in flight. Retry
          // once from the backend snapshot, with newer fields taking priority.
          pending = { ...changes, ...(pending as Partial<T>) };
          retried = true;
          publish({ ...committed, ...pending });
          continue;
        }
        pending = null;
        confirmed = committed;
        publish(committed);
        onError(reason);
        throw reason;
      }
    }
  };

  return {
    receive(next: T) {
      // save_config emits before its reply. Wait for the drain to reconcile
      // those snapshots instead of replaying an older selection into the UI.
      if (!active) {
        confirmed = next;
        publish(next);
      }
    },
    patch(change: Partial<T>) {
      if (!snapshot) return Promise.reject(new Error("Configuration is not loaded"));
      pending = { ...pending, ...change };
      publish({ ...snapshot, ...change });
      if (!active) active = Promise.resolve().then(drain).finally(() => { active = null; });
      return active;
    },
  };
}
