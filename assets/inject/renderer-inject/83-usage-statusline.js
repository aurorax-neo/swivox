  /**
   * 输入框下方用量状态栏：生成速度 / 缓存命中率 / 上下文占用。
   *
   * 数据全部来自 Codex 自己写的 rollout 记录（`event_msg.token_count`），中文档口径
   * 与 Codex 一致：缓存命中 = cached_input_tokens / input_tokens，上下文占用 =
   * last_token_usage.total_tokens / model_context_window。这里的 model_context_window
   * 正是 Swivox 注入 model_catalog_json 后 Codex 认到的每模型窗口，所以按模型配置
   * 窗口的功能会直接反映在这一项上。
   *
   * 为什么不复用 `/thread-usage-history`：那条路由按会话整读 rollout 并解析全部行，
   * 真实会话的 rollout 可以到几百 MB，状态栏按秒级轮询扛不住。这里走
   * `/thread-usage-live`，宿主只读文件尾部。
   *
   * 节点挂在输入卡片（[data-composer-surface-variant]）之后、同一父容器内，也就是
   * 「输入框下方、卡片外」。节点带 data-swivox-ext，扫描调度器会把它排除在
   * 自喂循环之外（issue #1960）。
   */
  registerSwivoxExtensionSelector('[data-swivox-ext="builtin-usage-statusline"]');
  window.__swivoxUsageStatusLineRuntime?.dispose?.();

  const swivoxUsageStatusLineId = "swivox-usage-statusline";
  const swivoxUsageStyleId = "swivox-usage-statusline-style";
  /** 空闲轮询间隔；最近有 token 事件时切到活跃间隔。 */
  const swivoxUsageIdleIntervalMs = 5000;
  /** 本轮在跑（最近有事件）时的间隔。 */
  const swivoxUsageActiveIntervalMs = 1200;
  /** 已经拿到会话、但还没写出第一条 token 事件时的间隔。 */
  const swivoxUsageWarmupIntervalMs = 2500;
  /** 最近一条 token 事件超过这个时间就认为本轮已经结束，速度值转为淡化显示。 */
  const swivoxUsageStaleMs = 120000;
  /** 连续失败到这个次数后停掉轮询，避免桥接不可用时每秒写诊断日志。 */
  const swivoxUsageFailureLimit = 5;

  const swivoxUsageState = {
    disposed: false,
    timer: 0,
    node: null,
    parts: null,
    threadId: "",
    failures: 0,
    inFlight: false,
    lastFingerprint: "",
  };

  const swivoxUsageSvgNs = "http://www.w3.org/2000/svg";

  function swivoxUsageText() {
    const language = (document.documentElement?.lang || navigator.language || "").toLowerCase();
    return language.startsWith("zh")
      ? { speed: (value) => `${value} tok/s`, cache: (value) => `缓存命中 ${value}`, context: (value) => value, aria: "用量状态栏" }
      : { speed: (value) => `${value} tok/s`, cache: (value) => `cache hit ${value}`, context: (value) => value, aria: "Usage status line" };
  }

  function swivoxUsageInstallStyle() {
    if (document.getElementById(swivoxUsageStyleId)) return;
    const style = document.createElement("style");
    style.id = swivoxUsageStyleId;
    style.dataset.swivoxExt = "builtin-usage-statusline";
    style.textContent = `
      /* 颜色只用一层透明度：主题的 --color-token-text-secondary 本身就是 65% 透明，
         再叠容器 opacity 会掉到 0.3 出头，实测在 #141414 上几乎看不清。
         这里以主文字色取 82%，既够亮又略低于输入框文字。 */
      /* 三项作为一组居中，和 DSH 那条一致；两端对齐会把「上下文占用」甩到最右边，
         和速度、缓存拉得太开，读起来是两段信息。 */
      #${swivoxUsageStatusLineId}{display:flex;align-items:center;justify-content:center;gap:24px;box-sizing:border-box;width:100%;margin:6px 0 0;padding:0 10px;font:inherit;font-size:12px;line-height:17px;color:var(--color-token-text-secondary,rgba(205,205,215,.95));color:color-mix(in srgb,var(--color-token-text-primary,#dfdfdf) 82%,transparent);user-select:none;cursor:default}
      #${swivoxUsageStatusLineId}[data-hidden="true"]{display:none}
      #${swivoxUsageStatusLineId} .swivox-usage-item{display:inline-flex;align-items:center;gap:5px;white-space:nowrap;font-variant-numeric:tabular-nums;transition:opacity .2s}
      #${swivoxUsageStatusLineId} .swivox-usage-value{font-weight:500}
      #${swivoxUsageStatusLineId} .swivox-usage-item svg{flex:0 0 auto;opacity:.8}
      /* 本轮结束超过阈值只淡化速度：缓存命中与上下文占用仍然是当前值，不该跟着变灰。 */
      #${swivoxUsageStatusLineId}[data-stale="true"] .swivox-usage-item[data-usage="speed"]{opacity:.55}
    `;
    document.head?.appendChild(style);
  }

  /** 描边图标，尺寸与颜色都跟随文字，避免在浅色主题下露白。 */
  function swivoxUsageIcon(shapes) {
    const svg = document.createElementNS(swivoxUsageSvgNs, "svg");
    svg.setAttribute("viewBox", "0 0 16 16");
    svg.setAttribute("width", "13");
    svg.setAttribute("height", "13");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "1.4");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("aria-hidden", "true");
    const created = shapes.map((shape) => {
      const node = document.createElementNS(swivoxUsageSvgNs, shape.tag);
      Object.entries(shape.attrs).forEach(([key, value]) => node.setAttribute(key, value));
      svg.appendChild(node);
      return node;
    });
    return { svg, created };
  }

  /** 上下文用进度环，环长 2πr；r=6 时周长约 37.7。 */
  const swivoxUsageRingCircumference = 37.7;

  function swivoxUsageRingIcon() {
    const icon = swivoxUsageIcon([
      { tag: "circle", attrs: { cx: "8", cy: "8", r: "6", "stroke-opacity": ".3" } },
      { tag: "circle", attrs: { cx: "8", cy: "8", r: "6", "stroke-dasharray": `0 ${swivoxUsageRingCircumference}`, transform: "rotate(-90 8 8)" } },
    ]);
    return { svg: icon.svg, ring: icon.created[1] };
  }

  function swivoxUsageItem(node, key, label, icon) {
    const item = document.createElement("span");
    item.className = "swivox-usage-item";
    item.dataset.usage = key;
    item.appendChild(icon);
    const value = document.createElement("span");
    value.className = "swivox-usage-value";
    value.textContent = "--";
    item.appendChild(value);
    node.appendChild(item);
    return { item, value, label };
  }

  /** DOM 只建一次；之后每轮只改文本与环长，避免每秒重建节点触发扫描。 */
  function swivoxUsageBuild(node) {
    node.textContent = "";
    const ring = swivoxUsageRingIcon();
    const parts = {
      speed: swivoxUsageItem(node, "speed", "speed", swivoxUsageIcon([
        { tag: "path", attrs: { d: "M2 12.5a6 6 0 0 1 12 0" } },
        { tag: "path", attrs: { d: "M8 12.5 10.8 8.6" } },
      ]).svg),
      cache: swivoxUsageItem(node, "cache", "cache", swivoxUsageIcon([
        { tag: "path", attrs: { d: "M8 2.2c2.5 0 4.5.8 4.5 1.8S10.5 5.8 8 5.8 3.5 5 3.5 4 5.5 2.2 8 2.2Z" } },
        { tag: "path", attrs: { d: "M3.5 4v8c0 1 2 1.8 4.5 1.8s4.5-.8 4.5-1.8V4" } },
        { tag: "path", attrs: { d: "M3.5 8c0 1 2 1.8 4.5 1.8S12.5 9 12.5 8" } },
      ]).svg),
      context: swivoxUsageItem(node, "context", "context", ring.svg),
      ring: ring.ring,
    };
    swivoxUsageState.parts = parts;
    return parts;
  }

  const swivoxUsageThreadSpeedCache = window.__swivoxUsageThreadSpeedCache
    || (window.__swivoxUsageThreadSpeedCache = new Map());
  const swivoxUsageEditorSelector = '.ProseMirror[contenteditable="true"], textarea:not(:disabled):not([readonly]), [role="textbox"][contenteditable="true"]';

  function swivoxUsageComposerCard() {
    const primary = [...document.querySelectorAll("[data-composer-surface-variant]")]
      .filter((card) => visibleElement(card) && !card.closest("[data-swivox-ext]"));
    if (primary.length) {
      return primary.find((card) => card.querySelector(swivoxUsageEditorSelector)) || primary[0];
    }
    const fallbacks = [
      ...document.querySelectorAll('[data-codex-composer-root], .composer-surface-chrome, [class*="_ComposerLayoutRoot_"]'),
    ].filter((el) => visibleElement(el) && !el.closest("[data-swivox-ext]") && el.querySelector(swivoxUsageEditorSelector));
    if (fallbacks.length) return fallbacks[0];
    const editor = [...document.querySelectorAll(swivoxUsageEditorSelector)]
      .find((el) => visibleElement(el) && !el.closest("[data-swivox-ext]"));
    return editor?.closest("form") || null;
  }

  function swivoxUsageThreadId(card) {
    const composerRoot = card?.closest("[data-codex-composer-root]");
    const fromComposer = (composerRoot || card)
      ?.querySelector("[data-above-composer-conversation-id]")
      ?.getAttribute("data-above-composer-conversation-id");
    const placement = composerRoot?.getAttribute("data-composer-placement") || "";
    const allowFallback = !placement || placement === "thread";
    const fromSidebar = allowFallback
      ? document
          .querySelector('[data-app-action-sidebar-thread-selected="true"][data-app-action-sidebar-thread-id], [data-app-action-sidebar-thread-active="true"][data-app-action-sidebar-thread-id]')
          ?.getAttribute("data-app-action-sidebar-thread-id")
      : null;
    const fromLocation = allowFallback && typeof locationThreadId === "function" ? locationThreadId() : "";
    const raw = String(fromComposer || fromSidebar || fromLocation || "").trim();
    if (/^(?:local:)?client-new-thread:/i.test(raw)) return "";
    const id = raw.replace(/^local:/, "").trim();
    if (!id || id === "__proto__" || id === "prototype" || id === "constructor") return "";
    return /^[A-Za-z0-9_.-]{8,128}$/.test(id) ? id : "";
  }

  function swivoxUsageEnsureNode(card) {
    const host = card?.parentElement;
    if (!host) return null;
    let node = swivoxUsageState.node;
    if (node?.isConnected && node.previousElementSibling === card) return node;
    if (!node) {
      node = document.createElement("div");
      node.id = swivoxUsageStatusLineId;
      node.setAttribute("data-swivox-ext", "builtin-usage-statusline");
      node.setAttribute("role", "status");
      swivoxUsageState.node = node;
    }
    card.insertAdjacentElement("afterend", node);
    return node;
  }

  function swivoxUsageHide() {
    const node = swivoxUsageState.node;
    if (node) {
      // 只切属性、不摘节点：新会话在第一条 token 事件写出前会反复走空数据路径，
      // 每秒增删一次节点既浪费又会让 Codex 的 MutationObserver 空转。
      node.dataset.hidden = "true";
    }
    swivoxUsageState.lastFingerprint = "";
  }

  function swivoxUsageShow(node) {
    if (node.dataset.hidden !== "true") return;
    node.dataset.hidden = "false";
    swivoxUsageState.lastFingerprint = "";
  }

  function swivoxUsagePercent(numerator, denominator) {
    if (!(denominator > 0) || !(numerator >= 0)) return null;
    return Math.min(100, Math.max(0, (numerator / denominator) * 100));
  }

  /**
   * 生成速度（token/s）。
   *
   * Codex 只在每次模型调用结束时写一条 token_count，事件间隔里同时包含生成和工具
   * 执行时间，所以这是「吞吐」而不是纯生成速度，会比真实解码速度低。用同一轮内
   * 相邻事件的累计输出差除以时间差得到区间速度，再取最近几个区间平均，避免单次
   * 抖动。换轮时 turn_output_tokens 会回落，靠这个切段。
   */
  function swivoxUsageRate(events) {
    let totalTokens = 0;
    let totalSeconds = 0;
    for (let index = 1; index < events.length; index += 1) {
      const previous = events[index - 1];
      const current = events[index];
      if (previous.turn_id && current.turn_id && previous.turn_id !== current.turn_id) continue;
      const previousOutput = Number(previous.turn_output_tokens) || 0;
      const currentOutput = Number(current.turn_output_tokens) || 0;
      if (currentOutput <= previousOutput) continue;
      const startedAt = Date.parse(previous.t);
      const endedAt = Date.parse(current.t);
      const seconds = (endedAt - startedAt) / 1000;
      if (!(seconds >= 0.5) || seconds > 120) continue;
      totalTokens += currentOutput - previousOutput;
      totalSeconds += seconds;
    }
    return totalSeconds > 0 ? totalTokens / totalSeconds : null;
  }

  function swivoxUsageSnapshot(events, threadId = "") {
    if (!Array.isArray(events) || !events.length) return null;
    const latest = events[events.length - 1];
    const observedAt = Date.parse(latest.t);
    let speed = swivoxUsageRate(events);
    if (speed != null && threadId) {
      swivoxUsageThreadSpeedCache.set(threadId, speed);
      if (swivoxUsageThreadSpeedCache.size > 64) {
        swivoxUsageThreadSpeedCache.delete(swivoxUsageThreadSpeedCache.keys().next().value);
      }
    } else if (speed == null && threadId && swivoxUsageThreadSpeedCache.has(threadId)) {
      speed = swivoxUsageThreadSpeedCache.get(threadId);
    }
    return {
      speed,
      cache: swivoxUsagePercent(Number(latest.cached_input_tokens), Number(latest.input_tokens)),
      context: swivoxUsagePercent(Number(latest.total_tokens), Number(latest.context_window)),
      stale: Number.isFinite(observedAt) ? Date.now() - observedAt > swivoxUsageStaleMs : false,
      modelContextWindow: Number(latest.context_window) || 0,
    };
  }

  /** `snapshot` 为 null 时渲染占位符：会话已确定但还没有第一条 token 事件。 */
  function swivoxUsageRender(snapshot) {
    const node = swivoxUsageState.node;
    if (!node?.isConnected) return;
    const parts = swivoxUsageState.parts || swivoxUsageBuild(node);
    const text = swivoxUsageText();
    const format = (value) => (value === null || value === undefined || Number.isNaN(value) ? "--" : value);
    const speed = snapshot?.speed == null ? "--" : String(Math.max(0, Math.round(snapshot.speed)));
    const cache = snapshot?.cache == null ? "--" : `${snapshot.cache.toFixed(1)}%`;
    const context = snapshot?.context == null ? "--" : `${Math.round(snapshot.context)}%`;
    const stale = snapshot?.stale === true;
    const contextWindow = snapshot?.modelContextWindow || 0;
    // 指纹一样就不写 DOM：每秒一次无意义赋值会让 Codex 的 MutationObserver 空转。
    const fingerprint = [speed, cache, context, stale, contextWindow].join("|");
    if (fingerprint === swivoxUsageState.lastFingerprint) return;
    swivoxUsageState.lastFingerprint = fingerprint;

    parts.speed.value.textContent = text.speed(speed);
    parts.cache.value.textContent = text.cache(cache);
    parts.context.value.textContent = text.context(context);
    if (parts.ring) {
      const filled = snapshot?.context == null ? 0 : (snapshot.context / 100) * swivoxUsageRingCircumference;
      parts.ring.setAttribute("stroke-dasharray", `${filled.toFixed(2)} ${swivoxUsageRingCircumference}`);
    }
    node.dataset.stale = String(stale);
    node.setAttribute("title", [
      `生成速度 ${format(speed)} tok/s`,
      `缓存命中 ${format(cache)}`,
      `上下文占用 ${format(context)}`,
      contextWindow > 0 ? `窗口 ${contextWindow}` : "",
    ].filter(Boolean).join(" · "));
    node.setAttribute("aria-label", `${text.aria}：${text.speed(speed)}，${text.cache(cache)}，${context}`);
  }

  function swivoxUsageSchedule(delay) {
    if (swivoxUsageState.disposed) return;
    clearTimeout(swivoxUsageState.timer);
    swivoxUsageState.timer = window.setTimeout(swivoxUsageTick, delay);
  }

  function swivoxUsageEnabled() {
    return !swivoxUsageState.disposed
      && swivoxBackendSettingsLoaded
      && swivoxSettings().usageStatusLine === true;
  }

  async function swivoxUsageRefresh(threadId) {
    if (swivoxUsageState.inFlight) return null;
    swivoxUsageState.inFlight = true;
    try {
      const result = await postJson("/thread-usage-live", { session_id: threadId });
      if (result?.status !== "ok" || !Array.isArray(result.events)) {
        swivoxUsageState.failures += 1;
        return null;
      }
      swivoxUsageState.failures = 0;
      return result.events;
    } finally {
      swivoxUsageState.inFlight = false;
    }
  }

  async function swivoxUsageTick() {
    if (swivoxUsageState.disposed) return;
    if (!swivoxUsageEnabled()) {
      swivoxUsageHide();
      swivoxUsageSchedule(swivoxUsageIdleIntervalMs);
      return;
    }
    // 页面不可见时不拉数据，但保留轮询，切回来下一拍就会刷新。
    if (document.hidden) {
      swivoxUsageSchedule(swivoxUsageIdleIntervalMs);
      return;
    }
    const card = swivoxUsageComposerCard();
    const threadId = swivoxUsageThreadId(card);
    if (!card || !threadId) {
      swivoxUsageHide();
      swivoxUsageState.threadId = "";
      swivoxUsageSchedule(swivoxUsageIdleIntervalMs);
      return;
    }
    swivoxUsageInstallStyle();
    const node = swivoxUsageEnsureNode(card);
    if (!node) {
      swivoxUsageSchedule(swivoxUsageIdleIntervalMs);
      return;
    }
    // 换会话时清掉上一个会话的读数，避免把旧数字挂在新会话上。
    if (swivoxUsageState.threadId !== threadId && swivoxUsageState.node === node) {
      swivoxUsageState.threadId = threadId;
      swivoxUsageState.lastFingerprint = "";
    }
    let events = null;
    try {
      events = await swivoxUsageRefresh(threadId);
    } catch (error) {
      swivoxUsageState.failures += 1;
      if (swivoxUsageState.failures === swivoxUsageFailureLimit) {
        sendSwivoxDiagnostic("usage_statusline_bridge_failed", {
          errorName: error?.name || "",
          errorMessage: error?.message || String(error),
        });
      }
    }
    if (swivoxUsageState.disposed) return;
    const snapshot = events ? swivoxUsageSnapshot(events, threadId) : null;
    if (snapshot) {
      swivoxUsageShow(node);
      swivoxUsageRender(snapshot);
      // 刚有事件说明本轮在跑，加密轮询；本轮已经结束就拉长间隔。
      swivoxUsageSchedule(snapshot.stale ? swivoxUsageIdleIntervalMs : swivoxUsageActiveIntervalMs);
      return;
    }
    if (swivoxUsageState.failures >= swivoxUsageFailureLimit) {
      // 桥接连续失败：整条隐藏，退到慢轮询，等宿主恢复。
      swivoxUsageHide();
      swivoxUsageSchedule(30000);
      return;
    }
    // 会话已经确定、只是还没写出 token 事件（新会话第一轮）：先显示占位符，
    // 用较慢的间隔等第一条数据，避免空转。
    swivoxUsageShow(node);
    swivoxUsageRender(null);
    swivoxUsageSchedule(swivoxUsageWarmupIntervalMs);
  }

  swivoxUsageInstallStyle();
  swivoxUsageSchedule(600);
  window.__swivoxUsageStatusLineRuntime = {
    version: 1,
    sync: () => swivoxUsageSchedule(0),
    dispose() {
      swivoxUsageState.disposed = true;
      clearTimeout(swivoxUsageState.timer);
      swivoxUsageState.timer = 0;
      // 卸载时把节点整个摘掉：注入会被重跑，留一个隐藏节点等于每次重注入多挂一个。
      swivoxUsageState.node?.remove();
      swivoxUsageState.node = null;
      swivoxUsageState.parts = null;
      swivoxUsageState.lastFingerprint = "";
      globalThis.__SWIVOX_USAGE_STATUSLINE_SYNC__ = undefined;
    },
  };
  globalThis.__SWIVOX_USAGE_STATUSLINE_SYNC__ = () => swivoxUsageSchedule(0);
