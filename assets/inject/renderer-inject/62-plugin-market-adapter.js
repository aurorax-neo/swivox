  /**
   * 复用原生插件页面：仅接管带独立索引标记的详情与安装，其他插件仍走原接口。
   * 当前验证的传输有 client.sendRequest、fetch envelope 和 mcp-request envelope。
   * 安装请求只发一次；桥接超时后查询后台任务，不能回落到原生 plugin/install。
   */
  const swivoxPluginNativeVersion = "1";
  const swivoxPluginNativeState = window.__swivoxPluginNativeState || {
    bypass: Symbol("swivox-plugin-native-bypass"), requests: new Map(), synthetic: new WeakSet(),
    ids: new Map(), catalogs: new Map(), installJobs: new Map(),
  };
  window.__swivoxPluginNativeState = swivoxPluginNativeState;
  const swivoxPluginNativeBypass = swivoxPluginNativeState.bypass;
  const swivoxPluginNativeRequests = swivoxPluginNativeState.requests;
  const swivoxPluginNativeSyntheticMessages = swivoxPluginNativeState.synthetic;
  const swivoxPluginNativeIds = swivoxPluginNativeState.ids;
  const swivoxPluginNativeCatalogs = swivoxPluginNativeState.catalogs;
  const swivoxPluginNativeInstallJobs = swivoxPluginNativeState.installJobs;

  function swivoxPluginNativeOperation(method, params, hostId) {
    const wrapped = String(method || "") === "send-cli-request-for-host";
    const requestParams = wrapped ? params?.params || {} : params || {};
    const hosts = [hostId, params?.hostId, wrapped ? requestParams?.hostId : null].filter((value) => value != null);
    return {
      method: appServerModelRequestMethod(String(method || ""), params),
      params: requestParams,
      hostId: hosts.find((value) => value !== "local") ?? hosts[0],
    };
  }

  function swivoxPluginNativeSource(params) {
    const path = String(params?.marketplacePath || "").replace(/\\/g, "/");
    const pathMatch = /(?:^|\/)swivox-plugin-market\/virtual-(public|full)\/\.agents\/plugins\/marketplace\.json$/.exec(path);
    if (pathMatch) return pathMatch[1];
    const name = String(params?.remoteMarketplaceName || params?.localMarketplaceName || "");
    const nameMatch = /^swivox-index-(public|full)$/.exec(name);
    return nameMatch ? nameMatch[1] : "";
  }

  function swivoxPluginNativeLocal(operation) {
    return operation.hostId == null || operation.hostId === "local";
  }

  function swivoxPluginNativeListAllowed(operation) {
    if (!swivoxPluginNativeLocal(operation)) return false;
    if (operation.method === "installed-plugins") return true;
    if (operation.method !== "list-plugins") return false;
    const kinds = operation.params?.marketplaceKinds;
    return !Array.isArray(kinds) || kinds.length === 0 || kinds.includes("local");
  }

  function swivoxPluginNativeRemember(result) {
    const marketplaces = result?.marketplaces;
    if (!Array.isArray(marketplaces)) return;
    marketplaces.forEach((market) => {
      const source = swivoxPluginNativeSource({ marketplacePath: market.path, localMarketplaceName: market.name });
      if (!source || !Array.isArray(market.plugins)) return;
      market.plugins.forEach((plugin) => {
        if (typeof plugin?.name === "string" && typeof plugin?.swivoxIndexId === "string") {
          swivoxPluginNativeIds.set(`${source}:${plugin.name}`, plugin.swivoxIndexId);
        }
      });
    });
  }

  async function swivoxPluginNativeBackend(method, params, source) {
    let result;
    try {
      result = await postJson("/plugin-market/native", { method, params: params || {}, ...(source ? { source } : {}) });
    } catch (error) {
      // 传输中断不能证明后台未接到请求；安装提交后的异常要查询原任务。
      const failure = error instanceof Error ? error : new Error(String(error));
      failure.outcomeUnknown = true;
      failure.timeout = /timeout|timed out|超时|deadline/i.test(failure.message);
      throw failure;
    }
    if (!result || typeof result !== "object" || result.status === "failed") {
      const error = new Error(result?.message || "Codex 插件市场请求失败");
      error.timeout = result?.timeout === true || /timeout|timed out|超时/i.test(error.message);
      throw error;
    }
    return result;
  }

  async function swivoxPluginNativeCatalog(operation) {
    const key = operation.method;
    const now = Date.now();
    const cached = swivoxPluginNativeCatalogs.get(key);
    const ttl = key === "installed-plugins" ? 1000 : 60_000;
    if (cached?.promise) return JSON.parse(JSON.stringify(await cached.promise));
    if (cached?.value && !operation.params?.forceRefetch && now - cached.at < ttl) {
      return JSON.parse(JSON.stringify(cached.value));
    }
    const entry = { at: now, value: null, promise: null };
    const promise = swivoxPluginNativeBackend(operation.method, operation.params).then((result) => {
      if (!Array.isArray(result.marketplaces)) throw new Error("Codex 插件市场列表格式错误");
      swivoxPluginNativeRemember(result);
      entry.value = result;
      entry.at = Date.now();
      return result;
    }).finally(() => { entry.promise = null; });
    entry.promise = promise;
    swivoxPluginNativeCatalogs.set(key, entry);
    return JSON.parse(JSON.stringify(await promise));
  }

  function swivoxPluginNativeMerge(original, extra, method) {
    const base = original && typeof original === "object" ? original : {};
    const managedIds = new Set((extra?.marketplaces || []).flatMap((market) =>
      swivoxPluginNativeSource({ marketplacePath: market.path, localMarketplaceName: market.name })
        ? (market.plugins || []).map((plugin) => plugin.id) : []));
    const markets = Array.isArray(base.marketplaces) ? base.marketplaces.map((market) => {
      if (!/^swivox-plugin-market-[a-f0-9]{12}$/.test(market.name || "") || !Array.isArray(market.plugins)) return market;
      // 已安装目录也会从原生本地市场返回；只消除我们自己 namespace 内同 id 的重复。
      return { ...market, plugins: market.plugins.filter((plugin) => !managedIds.has(plugin.id)) };
    }) : [];
    for (const market of extra?.marketplaces || []) {
      const existing = markets.find((item) => item.name === market.name && item.path === market.path);
      if (!existing) {
        markets.push(market);
      } else {
        const ids = new Set((existing.plugins || []).map((plugin) => plugin.id));
        existing.plugins = [...(existing.plugins || []), ...(market.plugins || []).filter((plugin) => !ids.has(plugin.id))];
      }
    }
    const result = {
      ...base, marketplaces: markets,
      marketplaceLoadErrors: [...(Array.isArray(base.marketplaceLoadErrors) ? base.marketplaceLoadErrors : []), ...(extra?.marketplaceLoadErrors || [])],
      __swivoxIndexMerged: true,
    };
    if (method === "list-plugins") result.featuredPluginIds = [...new Set([...(base.featuredPluginIds || []), ...(extra?.featuredPluginIds || [])])];
    swivoxPluginNativeRemember(result);
    return result;
  }

  async function swivoxPluginNativeExtend(operation, original) {
    if (original?.__swivoxIndexMerged) return original;
    try {
      return swivoxPluginNativeMerge(original, await swivoxPluginNativeCatalog(operation), operation.method);
    } catch (error) {
      // 自定义市场暂时离线时保留原生列表，不能把系统插件清空。
      const fallbackPath = swivoxIsWindowsPlatform ? "C:\\swivox-plugin-market" : "/swivox-plugin-market";
      return swivoxPluginNativeMerge(original, { marketplaceLoadErrors: [{ marketplacePath: fallbackPath, message: error?.message || "Codex 插件市场暂时不可用" }] }, operation.method);
    }
  }

  function swivoxPluginNativeInvalidate() {
    swivoxPluginNativeCatalogs.clear();
    clearPluginMarketplaceQueryCache();
    showToast("插件已安装，新开聊天或重启 Codex 后加载。");
  }

  async function swivoxPluginNativeManaged(operation, source) {
    if (!swivoxPluginNativeLocal(operation)) throw new Error("Codex 插件市场目前仅支持本机安装，请切换到本机。");
    if (operation.method === "read-plugin") {
      const result = await swivoxPluginNativeBackend(operation.method, operation.params, source);
      const summary = result?.plugin?.summary;
      if (typeof summary?.swivoxIndexId === "string") swivoxPluginNativeIds.set(`${source}:${operation.params.pluginName}`, summary.swivoxIndexId);
      return result;
    }
    const key = `${source}:${operation.params.pluginName || ""}`;
    if (swivoxPluginNativeInstallJobs.has(key)) return await swivoxPluginNativeInstallJobs.get(key);
    const promise = (async () => {
      let id = swivoxPluginNativeIds.get(key);
      if (!id) {
        // 未浏览列表也可能直接进入详情，先获取真实索引 id；这一步不会下载插件文件。
        const detail = await swivoxPluginNativeBackend("read-plugin", operation.params, source);
        id = detail?.plugin?.summary?.swivoxIndexId;
        if (typeof id !== "string" || !id) throw new Error("插件索引缺少安装标识，请刷新目录后重试。");
        swivoxPluginNativeIds.set(key, id);
      }
      try {
        const result = await swivoxPluginNativeBackend("install-plugin", operation.params, source);
        if (result?.authPolicy && Array.isArray(result.appsNeedingAuth)) {
          swivoxPluginNativeInvalidate();
          return result;
        }
        throw new Error("正在等待插件安装结果");
      } catch (error) {
        if (!error?.timeout && !error?.outcomeUnknown && !/正在等待/.test(error?.message || "")) throw error;
      }
      // 安装已经提交，后续只能读取同一后台任务，绝不能再次提交或发给原生接口。
      const deadline = Date.now() + 10 * 60_000;
      let communicationFailures = 0;
      const outcomeUnknown = () => new Error("暂时无法确认安装结果，请刷新已安装目录查看；后台任务可能仍在运行。");
      while (Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, 1500));
        let status;
        try { status = await postJson("/plugin-market/install-status", { source, id }); } catch {
          if (++communicationFailures >= 5) throw outcomeUnknown();
          continue;
        }
        if (status?.busy === true) { communicationFailures = 0; continue; }
        if (status?.busy === false && status.stage === "complete" && status.nativeResult?.authPolicy && Array.isArray(status.nativeResult.appsNeedingAuth)) {
          swivoxPluginNativeInvalidate();
          return status.nativeResult;
        }
        if (status?.busy === false) throw new Error(status.message || "插件安装未完成，请重试。");
        if (++communicationFailures >= 5) throw outcomeUnknown();
      }
      throw outcomeUnknown();
    })().finally(() => { swivoxPluginNativeInstallJobs.delete(key); });
    swivoxPluginNativeInstallJobs.set(key, promise);
    return await promise;
  }

  /** 返回 null 表示原接口继续执行；返回 Promise 表示已由本层接管。 */
  function swivoxPluginNativeInterceptClient(method, params, options, original, hostId) {
    if (!codexPluginMarketplacePatchEnabled() || options?.[swivoxPluginNativeBypass]) return null;
    const operation = swivoxPluginNativeOperation(method, params, hostId);
    const source = swivoxPluginNativeSource(operation.params);
    if (source && ["read-plugin", "install-plugin"].includes(operation.method)) return swivoxPluginNativeManaged(operation, source);
    if (!swivoxPluginNativeListAllowed(operation)) return null;
    return (async () => {
      let result;
      try {
        result = await original({ ...(options || {}), [swivoxPluginNativeBypass]: true });
      } catch (error) {
        if (!pluginMarketplaceRemoteAuthError(error)) throw error;
        result = { marketplaces: [], marketplaceLoadErrors: [], featuredPluginIds: [] };
      }
      return await swivoxPluginNativeExtend(operation, result);
    })();
  }

  function swivoxPluginNativeEnvelope(message) {
    if (!message || typeof message !== "object") return null;
    if (message.type === "mcp-request" && message.request?.id != null) {
      return {
        transport: "mcp", id: message.request.id,
        key: `mcp:${message.hostId || "local"}:${message.request.id}`,
        operation: swivoxPluginNativeOperation(message.request.method, message.request.params, message.hostId),
        request: message,
      };
    }
    if (message.type === "fetch" && message.requestId != null && typeof message.url === "string") {
      let params = message.body || {};
      if (typeof params === "string") {
        try { params = JSON.parse(params); } catch { return null; }
      }
      return {
        transport: "fetch", id: message.requestId, key: `fetch:${message.requestId}`,
        operation: swivoxPluginNativeOperation(message.url, params, message.hostId), request: message,
      };
    }
    return null;
  }

  function swivoxPluginNativeDeliver(data) {
    swivoxPluginNativeSyntheticMessages.add(data);
    window.dispatchEvent(new MessageEvent("message", { data, source: window, origin: window.location.origin }));
  }

  function swivoxPluginNativeReply(envelope, result, error) {
    if (envelope.transport === "fetch") {
      swivoxPluginNativeDeliver({ type: "fetch-response", requestId: envelope.id, responseType: error ? "error" : "success", status: error ? 500 : 200,
        headers: { "content-type": "application/json" }, ...(error ? { error: error.message || String(error) } : { bodyJsonString: JSON.stringify(result) }) });
    } else {
      swivoxPluginNativeDeliver({ type: "mcp-response", hostId: envelope.request.hostId || "local", message: { id: envelope.id,
        ...(error ? { error: { code: -32000, message: error.message || String(error) } } : { result }) } });
    }
  }

  function swivoxPluginNativeInterceptOutgoing(message) {
    if (!codexPluginMarketplacePatchEnabled()) return false;
    const envelope = swivoxPluginNativeEnvelope(message);
    if (!envelope) return false;
    const source = swivoxPluginNativeSource(envelope.operation.params);
    if (source && ["read-plugin", "install-plugin"].includes(envelope.operation.method)) {
      void swivoxPluginNativeManaged(envelope.operation, source).then(
        (result) => swivoxPluginNativeReply(envelope, result),
        (error) => swivoxPluginNativeReply(envelope, null, error));
      return true;
    }
    if (swivoxPluginNativeListAllowed(envelope.operation)) {
      swivoxPluginNativeRequests.set(envelope.key, envelope);
      // 只跟踪当前原生请求；超时不自发重发任何请求。
      setTimeout(() => swivoxPluginNativeRequests.delete(envelope.key), 90_000);
    }
    return false;
  }

  function swivoxPluginNativeSuppressForwarded(message) {
    if (!codexPluginMarketplacePatchEnabled()) return false;
    const envelope = swivoxPluginNativeEnvelope(message);
    return Boolean(envelope && swivoxPluginNativeSource(envelope.operation.params)
      && ["read-plugin", "install-plugin"].includes(envelope.operation.method));
  }

  function swivoxPluginNativeInterceptIncoming(data) {
    if (!data || swivoxPluginNativeSyntheticMessages.has(data)) return false;
    const fetchResponse = data.type === "fetch-response";
    const mcpResponse = data.type === "mcp-response";
    if (!fetchResponse && !mcpResponse) return false;
    const key = fetchResponse ? `fetch:${data.requestId}` : `mcp:${data.hostId || "local"}:${data.message?.id}`;
    const envelope = swivoxPluginNativeRequests.get(key);
    if (!envelope) return false;
    swivoxPluginNativeRequests.delete(key);
    void (async () => {
      let payload;
      let nativeError;
      let wrapped = false;
      if (fetchResponse) {
        if (data.responseType === "error" || Number(data.status) >= 400) nativeError = data.error || data.bodyJsonString || "原生插件接口请求失败";
        try { payload = "body" in data ? data.body : JSON.parse(data.bodyJsonString || "{}"); } catch {
          swivoxPluginNativeDeliver(data);
          return;
        }
        if (payload?.error) nativeError = payload.error;
        wrapped = payload && typeof payload === "object" && Object.prototype.hasOwnProperty.call(payload, "id");
      } else {
        payload = data.message?.result;
        nativeError = data.message?.error;
      }
      if (nativeError && !pluginMarketplaceRemoteAuthError(nativeError)) {
        swivoxPluginNativeDeliver(data);
        return;
      }
      const original = nativeError ? {} : wrapped ? payload.result : payload;
      const merged = await swivoxPluginNativeExtend(envelope.operation, original);
      if (fetchResponse) {
        const response = wrapped ? { ...payload, result: merged } : merged;
        if (wrapped) delete response.error;
        const next = { ...data, responseType: "success", status: 200, bodyJsonString: JSON.stringify(response) };
        delete next.body;
        delete next.error;
        swivoxPluginNativeDeliver(next);
      } else {
        const message = { ...data.message, result: merged };
        delete message.error;
        swivoxPluginNativeDeliver({ ...data, message });
      }
    })().catch((error) => swivoxPluginNativeReply(envelope, null, error));
    return true;
  }

  function installSwivoxPluginNativeTransportListener() {
    if (window.__swivoxPluginNativeResponseListener) window.removeEventListener("message", window.__swivoxPluginNativeResponseListener, true);
    window.__swivoxPluginNativeResponseListener = (event) => {
      if (event.source != null && event.source !== window) return;
      if (swivoxPluginNativeInterceptIncoming(event.data)) event.stopImmediatePropagation();
    };
    window.addEventListener("message", window.__swivoxPluginNativeResponseListener, true);
  }

  function ensureSwivoxPluginNativeTransportReinjection() {
    if (!codexPluginMarketplacePatchEnabled()) return;
    const bridge = window.electronBridge;
    if (bridge?.__codexPluginMarketplaceOriginalSendMessageFromView
        && bridge.__codexPluginMarketplaceBridgePatch !== codexPluginMarketplaceUnlockVersion
        && bridge.sendMessageFromView?.__swivoxPluginNativeTransport !== swivoxPluginNativeVersion) {
      const original = bridge.sendMessageFromView.bind(bridge);
      const patched = (message) => swivoxPluginNativeInterceptOutgoing(message) ? Promise.resolve() : original(message);
      patched.__swivoxPluginNativeTransport = swivoxPluginNativeVersion;
      bridge.sendMessageFromView = patched;
    }
    if (window.__codexPluginMarketplaceOriginalDispatchEvent
        && window.__codexPluginMarketplaceWindowEventPatch !== codexPluginMarketplaceUnlockVersion
        && window.dispatchEvent?.__swivoxPluginNativeTransport !== swivoxPluginNativeVersion) {
      const original = window.dispatchEvent.bind(window);
      const patched = (event) => {
        if (event?.type === "codex-message-from-view") {
          if (event.__codexForwardedViaBridge && swivoxPluginNativeSuppressForwarded(event.detail)) return true;
          if (!event.__codexForwardedViaBridge && swivoxPluginNativeInterceptOutgoing(event.detail)) return true;
        }
        if (event?.type === "message" && swivoxPluginNativeInterceptIncoming(event.data)) return true;
        return original(event);
      };
      patched.__swivoxPluginNativeTransport = swivoxPluginNativeVersion;
      window.dispatchEvent = patched;
    }
  }

  function swivoxNativePluginNavigationEntry() {
    // 每次重查原生导航；注入入口和第三方 Plugins 按钮不能充当原生目标。
    const destinations = Array.from(document.querySelectorAll('nav [data-sidebar-destination], aside.app-shell-left-panel nav button, nav[data-app-navigation-rail] button'));
    return destinations.find((button) => {
      if (button.closest('[data-swivox-ext], [data-swivox-rail]')) return false;
      if (button.closest(`#${swivoxSidebarPluginMarketId}, #${swivoxRailPluginMarketId}`)) return false;
      if (typeof isExtensionUiNode === "function" && isExtensionUiNode(button)) return false;
      if (typeof visibleElement === "function" && !visibleElement(button)) return false;
      const destination = (button.getAttribute("data-sidebar-destination") || "").trim();
      const label = (button.getAttribute("aria-label") || button.textContent || "").replace(/\s+/g, " ").trim();
      return destination === "plugins" || destination === "builtin:plugins" || /^(插件|Plugins)$/i.test(label);
    });
  }

  function openSwivoxNativePluginMarket() {
    closeSwivoxPage();
    clearPluginMarketplaceQueryCache();
    const native = swivoxNativePluginNavigationEntry();
    if (native && !native.disabled && native.getAttribute("aria-disabled") !== "true") {
      native.click();
      return;
    }
    // 没有可验证的原生入口时打开管理工具，不猜测内部路由或跳转外部浏览器。
    void postJson("/manager/open", { page: "pluginMarket" });
  }
