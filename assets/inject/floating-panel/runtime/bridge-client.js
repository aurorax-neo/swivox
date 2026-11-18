
  function bridgeCall(path, payload) {
    if (typeof window[PAGE_BRIDGE] !== "function") {
      return Promise.resolve({ error: "page bridge is not installed", items: [] });
    }
    let timer = 0;
    const timeout = new Promise((resolve) => {
      timer = window.setTimeout(() => resolve({ error: "page bridge timed out", items: [] }), BRIDGE_TIMEOUT_MS);
    });
    const args = INTERNAL_BRIDGE_TOKEN !== undefined
      ? [path, payload || {}, INTERNAL_BRIDGE_TOKEN]
      : [path, payload || {}];
    const request = Promise.resolve(window[PAGE_BRIDGE](...args));
    return Promise.race([request, timeout]).finally(() => window.clearTimeout(timer));
  }
