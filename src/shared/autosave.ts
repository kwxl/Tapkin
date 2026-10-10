// Debounce edits and serialize writes so an older request cannot overwrite a newer one.
export function createAutosave<T>(options: {
  delay: number;
  save: (value: T) => Promise<unknown>;
  saved: () => void;
  failed: (error: unknown) => void;
}) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: { value: T; revision: number } | undefined;
  let revision = 0;
  let saving = false;

  function clearTimer() {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  }

  async function flush() {
    clearTimer();
    if (saving || !pending) return;
    const request = pending;
    pending = undefined;
    saving = true;
    try {
      await options.save(request.value);
      if (request.revision === revision) options.saved();
    } catch (error) {
      if (request.revision === revision) options.failed(error);
    } finally {
      saving = false;
      if (pending && timer === undefined) void flush();
    }
  }

  return {
    update(value: T) {
      clearTimer();
      pending = { value, revision: ++revision };
      timer = setTimeout(() => void flush(), options.delay);
    },
    // Invalidate queued edits, including success callbacks from in-flight requests.
    cancel() {
      clearTimer();
      pending = undefined;
      revision += 1;
    },
    flush,
  };
}