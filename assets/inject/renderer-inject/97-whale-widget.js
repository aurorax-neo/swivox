  // 完整版鲸鱼由上游原始引擎 + Codex 数据适配层组成；默认关闭。
  window.__swivoxWhaleWidgetRuntime?.dispose?.();
  const swivoxWhaleState = { disposed: false, runtime: null, profile: "", missingSource: false, failedUntil: 0, notice: null };
  function swivoxWhaleEnabled() {
    return !swivoxWhaleState.disposed && swivoxBackendSettingsLoaded && swivoxSettings().whaleWidget === true;
  }
  function swivoxWhaleStop() {
    swivoxWhaleState.runtime?.dispose();
    swivoxWhaleState.runtime = null;
    swivoxWhaleState.notice?.remove(); swivoxWhaleState.notice = null;
  }
  function swivoxWhaleInitializationFailed(runtime) {
    runtime?.dispose(); swivoxWhaleState.runtime = null; swivoxWhaleState.failedUntil = Date.now() + 10000;
    if (swivoxWhaleState.notice) return;
    const notice = document.createElement("div"); notice.setAttribute("data-swivox-ext", "whale-widget"); notice.setAttribute("role", "status");
    notice.style.cssText = "position:fixed;right:20px;bottom:20px;z-index:2147483600;max-width:320px;padding:12px;border-radius:10px;background:#fff5ec;color:#7c3218;font:12px/1.6 system-ui;box-shadow:0 3px 16px #0002";
    notice.textContent = "鲸鱼挂件暂时无法加载，10 秒后自动重试。可在 Swivox 增强设置关闭后重新开启。";
    document.body.appendChild(notice); swivoxWhaleState.notice = notice;
  }
  function swivoxWhaleProfileFingerprint() {
    const profile = codexRemoteSessionActiveProfile?.();
    const settings = swivoxBackendSettings || {};
    return JSON.stringify([profile?.id || "", profile?.relayMode || "", settings.codexAppWhaleBalanceProtocol,
      settings.codexAppWhaleBalancePath, settings.codexAppWhaleBalanceField, settings.codexAppWhaleBalanceCurrency,
      settings.codexAppWhaleBalanceScale]);
  }
  function syncSwivoxWhaleWidget() {
    if (!swivoxWhaleEnabled()) { swivoxWhaleStop(); swivoxWhaleState.failedUntil = 0; return; }
    const profile = swivoxWhaleProfileFingerprint();
    if (swivoxWhaleState.profile !== profile) { swivoxWhaleStop(); swivoxWhaleState.failedUntil = 0; }
    swivoxWhaleState.profile = profile;
    if (swivoxWhaleState.runtime) {
      swivoxWhaleState.runtime.context();
      swivoxWhaleState.runtime.subscribe();
      swivoxWhaleState.runtime.pause(document.hidden === true);
      return;
    }
    // 隐藏窗口等恢复可见后再挂载，不在后台构造音频或轮询会话日志。
    if (document.hidden) return;
    if (Date.now() < swivoxWhaleState.failedUntil) return;
    if (typeof window.__SWIVOX_WHALE_ENGINE__ !== "function") {
      if (!swivoxWhaleState.missingSource) console.warn("[Swivox] 完整鲸鱼运行时未随启动器加载，请重新构建启动器。");
      swivoxWhaleState.missingSource = true; swivoxWhaleInitializationFailed(null); return;
    }
    const runtime = createSwivoxWhaleFullRuntime();
    swivoxWhaleState.runtime = runtime;
    registerSwivoxExtensionSelector("[data-swivox-ext=whale-widget]");
    void runtime.initialize().then(() => {
      if (runtime.active() && swivoxWhaleState.runtime === runtime && swivoxWhaleEnabled()) {
        window.__SWIVOX_WHALE_ENGINE__(...runtime.environment);
        if (runtime.state.initializationError || !runtime.state.controls) throw new Error("完整鲸鱼初始化未完成");
        swivoxWhaleState.notice?.remove(); swivoxWhaleState.notice = null;
      }
    }).catch((error) => {
      if (!runtime.active()) return;
      if (swivoxWhaleState.runtime === runtime) swivoxWhaleInitializationFailed(runtime);
    });
  }
  window.__swivoxWhaleWidgetRuntime = {
    version: 3,
    sync: syncSwivoxWhaleWidget,
    dispose() { swivoxWhaleStop(); swivoxWhaleState.disposed = true; },
    get state() { return swivoxWhaleState.runtime?.state || null; },
  };
