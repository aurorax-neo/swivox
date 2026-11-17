  // API Key 听写使用独立的 ASR 服务。新版原生 capability / app-host RPC 是只读的，
  // 因此入口与录音由 CPP 管理，识别结果通过原生编辑器事务回填，保持草稿和附件。
  window.__swivoxDictationRuntime?.dispose?.();
  const swivoxDictation = {
    phase: "idle", revision: 0, target: null, stream: null, recorder: null,
    chunks: [], blob: null, text: "", error: "", action: "insert", startedAt: 0,
    controller: null, timer: null, status: null, statusAt: 0, statusPromise: null,
    nativeApi: null, nativeApiPromise: null, pendingSend: false,
  };
  const swivoxDictationMaxBytes = 25 * 1024 * 1024;
  registerSwivoxExtensionSelector('[data-swivox-ext="builtin-dictation"]');

  function swivoxDictationEnabled() {
    return swivoxBackendSettingsLoaded && swivoxBackendSettings.enhancementsEnabled !== false
      && swivoxBackendSettings.dictation?.enabled === true;
  }

  function swivoxDictationEditor(scope) {
    return [...scope.querySelectorAll('.ProseMirror[contenteditable="true"], textarea:not(:disabled):not([readonly]), [role="textbox"][contenteditable="true"]')]
      .find((node) => visibleElement(node) && !isExtensionUiNode(node) && !node.closest('[inert], [aria-hidden="true"]')) || null;
  }

  function swivoxDictationScope(footer) {
    let scope = footer?.parentElement;
    while (scope && scope !== document.body) {
      if (swivoxDictationEditor(scope)) return scope;
      if (scope.matches?.('[data-codex-composer-root], [data-composer-surface-variant], form')) break;
      scope = scope.parentElement;
    }
    return null;
  }

  function swivoxDictationTargetEditor() {
    const target = swivoxDictation.target;
    if (!target || window.location.href !== target.href) return null;
    const editor = [...document.querySelectorAll('[data-swivox-dictation-target]')]
      .find((node) => node.dataset.swivoxDictationTarget === target.id && visibleElement(node));
    if (editor) return editor;
    const root = [...document.querySelectorAll('[data-swivox-dictation-root]')]
      .find((node) => node.dataset.swivoxDictationRoot === target.id && visibleElement(node));
    return root ? swivoxDictationEditor(root) : null;
  }

  async function loadSwivoxDictationNativeApi() {
    if (swivoxDictation.nativeApiPromise) return swivoxDictation.nativeApiPromise;
    swivoxDictation.nativeApiPromise = loadOptionalCodexAppModule("app-initial-").then((module) => {
      // 构建间压缩导出名会变化，只按能力识别函数；从不写 ESM namespace。
      const functions = Object.values(module || {}).filter((value) => typeof value === "function");
      const append = functions.find((fn) => {
        const source = String(fn);
        return source.length < 3000 && source.includes(".composerId===") && source.includes(".appendPromptText")
          && source.includes(".isDictationInProgress") && source.includes("[readonly]") && source.includes(".focus()");
      });
      const context = functions.find((fn) => {
        const source = String(fn);
        return source.length < 3000 && source.includes("document.activeElement?.closest")
          && source.includes("[data-codex-composer-root]") && source.includes("composerId:") && source.includes("root:");
      });
      swivoxDictation.nativeApi = append && context ? { append, context } : null;
      return swivoxDictation.nativeApi;
    }).catch(() => null);
    return swivoxDictation.nativeApiPromise;
  }

  function swivoxDictationView(editor) {
    const view = editor?.pmViewDesc?.view;
    return view && !view.isDestroyed && view.dom === editor && view.state?.tr && typeof view.dispatch === "function" ? view : null;
  }

  function swivoxInsertDictation(editor, transcript, nativeApi = swivoxDictation.nativeApi, composerId = swivoxDictation.target?.composerId) {
    const text = String(transcript || "").trim();
    if (!text || !editor?.isConnected || editor.matches?.(':disabled, [readonly], [contenteditable="false"]')
        || editor.closest?.('[inert], [aria-hidden="true"]')) return false;
    editor.focus();
    if (nativeApi && composerId) {
      const context = nativeApi.context();
      if (context?.composerId !== composerId || !context.root?.contains(editor)) return false;
      return nativeApi.append(composerId, text) === true;
    }
    const view = swivoxDictationView(editor);
    if (view) {
      // 用原生事务更新 React/ProseMirror 状态；不能直接改 innerHTML/textContent。
      const { from, to } = view.state.selection;
      const before = view.state.doc.textBetween(Math.max(0, from - 1), from, "\n");
      const prefix = before && !/\s$/.test(before) ? " " : "";
      view.dispatch(view.state.tr.insertText(prefix + text, from, to).scrollIntoView());
      return true;
    }
    if (editor instanceof HTMLTextAreaElement) {
      const from = editor.selectionStart ?? editor.value.length;
      const to = editor.selectionEnd ?? from;
      const prefix = from > 0 && !/\s/.test(editor.value[from - 1]) ? " " : "";
      const inserted = prefix + text;
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
      if (!setter) return false;
      setter.call(editor, editor.value.slice(0, from) + inserted + editor.value.slice(to));
      editor.setSelectionRange(from + inserted.length, from + inserted.length);
      editor.dispatchEvent(new Event("input", { bubbles: true }));
      return true;
    }
    // 老版本的 contenteditable 交给浏览器编辑命令，失败时保留可复制的识别结果。
    const selection = window.getSelection?.();
    if (document.activeElement !== editor || !selection?.anchorNode || !editor.contains(selection.anchorNode)) return false;
    return document.execCommand("insertText", false, text) === true;
  }

  function swivoxDictationSendButton(editor) {
    let scope = editor.parentElement;
    while (scope && scope !== document.body) {
      const button = [...scope.querySelectorAll('button')].find((node) => {
        if (isExtensionUiNode(node) || !visibleElement(node) || node.disabled || node.getAttribute("aria-disabled") === "true") return false;
        const label = `${node.getAttribute("aria-label") || ""} ${node.getAttribute("title") || ""}`.trim();
        return /^(?:Send|Send message|Send prompt|Submit|发送|发送消息|发送提示|提交)(?:\s|\(|（|$)/i.test(label)
          || node.getAttribute("data-testid") === "send-button" || node.getAttribute("data-action") === "send";
      });
      if (button) return button;
      if (scope.hasAttribute("data-codex-composer-root") || scope.hasAttribute("data-composer-surface-variant") || scope.tagName === "FORM") break;
      scope = scope.parentElement;
    }
    return null;
  }

  function swivoxDictationStopTracks() {
    swivoxDictation.stream?.getTracks().forEach((track) => track.stop());
    swivoxDictation.stream = null;
    clearInterval(swivoxDictation.timer);
    swivoxDictation.timer = null;
  }

  function cancelSwivoxDictation() {
    ++swivoxDictation.revision;
    swivoxDictation.controller?.abort();
    swivoxDictation.controller = null;
    const recorder = swivoxDictation.recorder;
    swivoxDictation.recorder = null;
    if (recorder?.state !== "inactive") { try { recorder?.stop(); } catch {} }
    swivoxDictationStopTracks();
    document.querySelectorAll('[data-swivox-dictation-target], [data-swivox-dictation-root]').forEach((node) => {
      delete node.dataset.swivoxDictationTarget; delete node.dataset.swivoxDictationRoot;
    });
    Object.assign(swivoxDictation, { phase: "idle", target: null, chunks: [], blob: null, text: "", error: "", pendingSend: false });
    renderSwivoxDictation();
  }

  async function loadSwivoxDictationStatus(force = false) {
    if (swivoxDictation.statusPromise) return swivoxDictation.statusPromise;
    if (!force && Date.now() - swivoxDictation.statusAt < 5000) return swivoxDictation.status;
    swivoxDictation.statusPromise = postJson("/dictation/status", {}).then((status) => {
      swivoxDictation.status = status;
      swivoxDictation.statusAt = Date.now();
      return status;
    }).finally(() => { swivoxDictation.statusPromise = null; });
    return swivoxDictation.statusPromise;
  }

  async function startSwivoxDictation(footer) {
    if (!swivoxDictationEnabled() || swivoxDictation.phase !== "idle") return;
    const editor = swivoxDictationScope(footer) && swivoxDictationEditor(swivoxDictationScope(footer));
    if (!editor) { showToast("无法找到可编辑的 Codex 输入框", null); return; }
    if (footer.getAttribute("data-dictation-view")) { showToast("请先结束原生语音输入", null); return; }
    if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder !== "function") {
      showToast("当前环境不支持麦克风录音", null); return;
    }
    const revision = ++swivoxDictation.revision;
    const id = crypto.randomUUID();
    editor.dataset.swivoxDictationTarget = id;
    Object.assign(swivoxDictation, {
      phase: "requesting", target: { id, href: window.location.href },
      error: "", text: "", chunks: [], blob: null, action: "insert",
    });
    installSwivoxDictation();
    try {
      const nativeApi = await loadSwivoxDictationNativeApi();
      if (revision !== swivoxDictation.revision) return;
      if (!swivoxDictationTargetEditor()) throw new Error("输入框或会话已变化，请返回目标会话后重录");
      editor.focus();
      const context = nativeApi?.context();
      if (context?.root?.contains(editor) && context.composerId) {
        swivoxDictation.target.composerId = context.composerId;
        context.root.dataset.swivoxDictationRoot = id;
      }
      const status = await loadSwivoxDictationStatus(true);
      if (revision !== swivoxDictation.revision) return;
      if (!swivoxDictationEnabled()) { cancelSwivoxDictation(); return; }
      if (!status?.enabled || !status?.configured || !status?.helperToken) throw new Error(status?.message || "请先在管理工具中配置并启用语音服务");
      const stream = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1 } });
      if (revision !== swivoxDictation.revision) {
        stream.getTracks().forEach((track) => track.stop()); return;
      }
      if (!swivoxDictationEnabled()) {
        stream.getTracks().forEach((track) => track.stop()); cancelSwivoxDictation(); return;
      }
      swivoxDictation.stream = stream;
      const mimeType = ["audio/webm;codecs=opus", "audio/webm", "audio/mp4"]
        .find((type) => typeof MediaRecorder.isTypeSupported === "function" && MediaRecorder.isTypeSupported(type));
      const recorder = new MediaRecorder(stream, mimeType ? { mimeType } : undefined);
      swivoxDictation.recorder = recorder;
      recorder.ondataavailable = (event) => {
        if (revision !== swivoxDictation.revision || !event.data?.size) return;
        swivoxDictation.chunks.push(event.data);
        if (swivoxDictation.chunks.reduce((sum, chunk) => sum + chunk.size, 0) > swivoxDictationMaxBytes) {
          swivoxDictation.error = "录音超过 25 MiB，请缩短后重录";
          stopSwivoxDictation("discard");
        }
      };
      recorder.onerror = () => {
        if (revision !== swivoxDictation.revision) return;
        swivoxDictation.error = "录音失败，请检查麦克风后重录";
        stopSwivoxDictation("discard");
      };
      recorder.onstop = () => {
        if (revision !== swivoxDictation.revision) return;
        swivoxDictationStopTracks();
        swivoxDictation.recorder = null;
        const blob = new Blob(swivoxDictation.chunks, { type: recorder.mimeType || "audio/webm" });
        swivoxDictation.chunks = [];
        if (swivoxDictation.action === "discard" || blob.size > swivoxDictationMaxBytes) {
          swivoxDictation.phase = "error"; renderSwivoxDictation(); return;
        }
        if (!blob.size || Date.now() - swivoxDictation.startedAt < 250) { cancelSwivoxDictation(); return; }
        swivoxDictation.blob = blob;
        void transcribeSwivoxDictation();
      };
      recorder.start(1000);
      swivoxDictation.phase = "recording";
      swivoxDictation.startedAt = Date.now();
      swivoxDictation.timer = setInterval(() => {
        if (Date.now() - swivoxDictation.startedAt >= 595000) stopSwivoxDictation("insert");
        renderSwivoxDictation();
      }, 1000);
      renderSwivoxDictation();
    } catch (error) {
      if (revision !== swivoxDictation.revision) return;
      swivoxDictationStopTracks();
      swivoxDictation.phase = "error";
      swivoxDictation.error = ["NotAllowedError", "SecurityError"].includes(error?.name)
        ? "麦克风权限被拒绝，请在系统设置中允许 Codex 使用麦克风后重录"
        : error?.name === "NotFoundError" ? "未找到麦克风" : error?.message || "无法开始录音";
      renderSwivoxDictation();
    }
  }

  function stopSwivoxDictation(action) {
    swivoxDictation.action = action;
    if (swivoxDictation.phase === "transcribing") { renderSwivoxDictation(); return; }
    if (swivoxDictation.phase !== "recording") return;
    swivoxDictation.phase = "transcribing";
    clearInterval(swivoxDictation.timer);
    try { swivoxDictation.recorder.stop(); }
    catch { swivoxDictationStopTracks(); swivoxDictation.phase = "error"; swivoxDictation.error = "无法结束录音，请重录"; }
    renderSwivoxDictation();
  }

  async function finishSwivoxDictation() {
    if (!swivoxDictationEnabled()) { cancelSwivoxDictation(); return; }
    const editor = swivoxDictationTargetEditor();
    if (!editor || !swivoxInsertDictation(editor, swivoxDictation.text)) {
      swivoxDictation.phase = "result";
      swivoxDictation.error = "输入框或会话已变化，请复制识别结果到目标输入框";
      renderSwivoxDictation(); return;
    }
    const send = swivoxDictation.action === "send";
    const target = { ...swivoxDictation.target };
    cancelSwivoxDictation();
    if (send) {
      const revision = swivoxDictation.revision;
      swivoxDictation.pendingSend = true;
      // 给原生状态一个渲染周期，再点击它自己的发送按钮；找不到时保留已插入的草稿。
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      if (revision !== swivoxDictation.revision) return;
      swivoxDictation.pendingSend = false;
      if (!swivoxDictationEnabled() || window.location.href !== target.href) return;
      const context = swivoxDictation.nativeApi?.context();
      const currentEditor = target.composerId && context?.composerId === target.composerId
        ? swivoxDictationEditor(context.root) : editor.isConnected && !target.composerId ? editor : null;
      if (!currentEditor) return;
      const button = swivoxDictationSendButton(currentEditor);
      if (button) button.click();
      else showToast("识别结果已插入，请点击发送", null);
    }
  }

  async function swivoxDictationAudioBase64(blob, signal) {
    if (blob.size > swivoxDictationMaxBytes) throw new Error("录音超过 25 MiB，请缩短后重录");
    const bytes = new Uint8Array(await blob.arrayBuffer());
    let encoded = "";
    // 按 3 字节对齐分段编码，避免一次展开大录音耗尽调用栈；让取消事件有机会执行。
    for (let offset = 0; offset < bytes.length; offset += 49152) {
      if (signal.aborted) throw new DOMException("转写已取消", "AbortError");
      encoded += btoa(String.fromCharCode(...bytes.subarray(offset, offset + 49152)));
      if (offset && offset % (49152 * 8) === 0) await new Promise((resolve) => setTimeout(resolve, 0));
    }
    return encoded;
  }

  // SenseVoice 接收 16 kHz/单声道 PCM WAV；MediaRecorder 在 Chromium 中通常只提供 WebM/Opus，
  // 因此只在选择本地 provider 时通过浏览器音频解码器做一次无损 PCM 转换。
  async function swivoxDictationSenseVoiceWav(blob, signal) {
    if (signal.aborted) throw new DOMException("转写已取消", "AbortError");
    const AudioContextCtor = window.AudioContext || window.webkitAudioContext;
    const OfflineContextCtor = window.OfflineAudioContext || window.webkitOfflineAudioContext;
    if (!AudioContextCtor || !OfflineContextCtor) throw new Error("当前环境无法转换本地 SenseVoice 音频");
    const context = new AudioContextCtor();
    try {
      const decoded = await context.decodeAudioData(await blob.arrayBuffer());
      if (signal.aborted) throw new DOMException("转写已取消", "AbortError");
      const frames = Math.max(1, Math.ceil(decoded.duration * 16000));
      const offline = new OfflineContextCtor(1, frames, 16000);
      const source = offline.createBufferSource();
      source.buffer = decoded;
      source.connect(offline.destination);
      source.start();
      const rendered = await offline.startRendering();
      const samples = rendered.getChannelData(0);
      const bytes = new ArrayBuffer(44 + samples.length * 2);
      const view = new DataView(bytes);
      const write = (offset, value) => { for (let i = 0; i < value.length; i++) view.setUint8(offset + i, value.charCodeAt(i)); };
      write(0, "RIFF"); view.setUint32(4, 36 + samples.length * 2, true); write(8, "WAVE");
      write(12, "fmt "); view.setUint32(16, 16, true); view.setUint16(20, 1, true); view.setUint16(22, 1, true);
      view.setUint32(24, 16000, true); view.setUint32(28, 32000, true); view.setUint16(32, 2, true); view.setUint16(34, 16, true);
      write(36, "data"); view.setUint32(40, samples.length * 2, true);
      for (let i = 0; i < samples.length; i++) {
        const sample = Math.max(-1, Math.min(1, samples[i]));
        view.setInt16(44 + i * 2, sample < 0 ? sample * 0x8000 : sample * 0x7fff, true);
      }
      return new Blob([bytes], { type: "audio/wav" });
    } finally {
      void context.close?.();
    }
  }

  async function transcribeSwivoxDictation() {
    if (!swivoxDictationEnabled()) { cancelSwivoxDictation(); return; }
    if (!swivoxDictation.blob) return;
    const revision = ++swivoxDictation.revision;
    const controller = new AbortController();
    swivoxDictation.controller = controller;
    swivoxDictation.phase = "transcribing";
    swivoxDictation.error = "";
    renderSwivoxDictation();
    let cancelRequest = null;
    try {
      const status = await loadSwivoxDictationStatus(true);
      if (revision !== swivoxDictation.revision) return;
      if (!status?.enabled || !status?.configured || !status?.helperToken) throw new Error(status?.message || "语音服务尚未配置");
      const requestId = crypto.randomUUID();
      let requestStarted = false;
      cancelRequest = () => {
        if (requestStarted) void postJson("/dictation/cancel", { requestId, helperToken: status.helperToken }).catch(() => {});
      };
      controller.signal.addEventListener("abort", cancelRequest, { once: true });
      const uploadBlob = status.provider === "sensevoice"
        ? await swivoxDictationSenseVoiceWav(swivoxDictation.blob, controller.signal)
        : swivoxDictation.blob;
      const audioBase64 = await swivoxDictationAudioBase64(uploadBlob, controller.signal);
      if (revision !== swivoxDictation.revision || controller.signal.aborted) return;
      if (!swivoxDictationEnabled()) { cancelSwivoxDictation(); return; }
      const extension = uploadBlob.type.includes("wav") ? "wav" : uploadBlob.type.includes("mp4") ? "m4a" : "webm";
      requestStarted = true;
      // app:// 的 CSP 不允许 renderer 直接 fetch localhost；音频经既有特权桥接交给 Rust。
      const result = await postJson("/dictation/transcribe", {
        requestId, audioBase64, mimeType: uploadBlob.type || "audio/webm",
        filename: `dictation.${extension}`, helperToken: status.helperToken,
      });
      controller.signal.removeEventListener("abort", cancelRequest);
      cancelRequest = null;
      if (revision !== swivoxDictation.revision) return;
      if (result?.status === "failed" || result?.status === "cancelled") throw new Error(result?.message || "语音转写失败");
      if (typeof result?.text !== "string") throw new Error("语音服务没有返回 text 字段");
      swivoxDictation.text = result.text.trim();
      if (!swivoxDictation.text) { cancelSwivoxDictation(); showToast("未识别到文字，请重录", null); return; }
      await finishSwivoxDictation();
    } catch (error) {
      if (revision !== swivoxDictation.revision) return;
      swivoxDictation.phase = "error";
      swivoxDictation.error = error?.name === "AbortError" ? "转写已取消" : error?.message || "语音转写失败";
      renderSwivoxDictation();
    } finally {
      if (cancelRequest) controller.signal.removeEventListener("abort", cancelRequest);
      if (revision === swivoxDictation.revision) swivoxDictation.controller = null;
    }
  }

  function swivoxDictationControl(action, label, disabled = false) {
    const button = document.createElement("button");
    button.type = "button"; button.textContent = label; button.dataset.dictationAction = action;
    button.disabled = disabled;
    return button;
  }

  /**
   * 空闲态的麦克风图标。
   *
   * 之前空闲态是一个带边框的「语音输入」文字按钮，在 composer 底栏里比原生图标按钮宽
   * 一倍多，把「完全访问」和模型选择器挤得很开。改成和原生同规格的图标按钮，文字降级
   * 成 aria-label / title。录音、转写这些状态仍然保留文字——那些状态要传达信息，
   * 光靠图标说不清。
   */
  function swivoxDictationMicIcon() {
    const ns = "http://www.w3.org/2000/svg";
    const svg = document.createElementNS(ns, "svg");
    svg.setAttribute("viewBox", "0 0 16 16");
    svg.setAttribute("width", "16"); svg.setAttribute("height", "16");
    svg.setAttribute("fill", "none"); svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "1.4"); svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round"); svg.setAttribute("aria-hidden", "true");
    ["M8 2.4a2.2 2.2 0 0 1 2.2 2.2v3.4a2.2 2.2 0 0 1-4.4 0V4.6A2.2 2.2 0 0 1 8 2.4Z",
      "M3.9 7.3a4.1 4.1 0 0 0 8.2 0", "M8 11.4v1.7", "M5.9 13.4h4.2"].forEach((d) => {
      const path = document.createElementNS(ns, "path");
      path.setAttribute("d", d);
      svg.appendChild(path);
    });
    return svg;
  }

  function swivoxDictationMicButton(disabled) {
    const button = swivoxDictationControl("start", "", disabled);
    button.className = "swivox-dictation-mic";
    button.setAttribute("aria-label", "语音输入");
    button.setAttribute("title", "语音输入");
    button.append(swivoxDictationMicIcon());
    return button;
  }

  function renderSwivoxDictation() {
    const state = swivoxDictation;
    document.querySelectorAll('[data-swivox-ext="builtin-dictation"][data-dictation-controls]').forEach((root) => {
      const active = state.phase !== "idle" && (root.dataset.dictationTarget === state.target?.id || root.dataset.floating === "true");
      const seconds = Math.floor((Date.now() - state.startedAt) / 1000);
      const recordingLabel = `● 录音 ${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
      const key = active ? `${state.phase}:${state.action}:${state.error}:${state.text}` : "idle";
      if (root.dataset.renderKey === key) {
        // 计时只改文本，不能每秒销毁按钮，否则键盘焦点和进行中的点击会丢失。
        if (active && state.phase === "recording") root.querySelector('[role="status"]').textContent = recordingLabel;
        return;
      }
      root.dataset.renderKey = key; root.replaceChildren();
      if (!active) { root.append(swivoxDictationMicButton(state.phase !== "idle")); return; }
      const status = document.createElement("span"); status.setAttribute("role", "status");
      if (state.phase === "recording") status.setAttribute("aria-live", "off");
      status.textContent = state.phase === "requesting" ? "正在请求麦克风…"
        : state.phase === "recording" ? recordingLabel
        : state.phase === "transcribing" ? "正在转写…" : state.error;
      root.append(status);
      if (state.phase === "recording") {
        root.append(swivoxDictationControl("insert", "停止并插入"), swivoxDictationControl("send", "停止并发送"));
      } else if (state.phase === "transcribing") {
        root.append(swivoxDictationControl("send", "完成后发送", state.action === "send"));
      } else if (state.phase === "error") {
        if (state.blob) root.append(swivoxDictationControl("retry", "重试转写"));
        root.append(swivoxDictationControl("rerecord", "重录"));
      } else if (state.phase === "result") {
        const text = document.createElement("textarea"); text.readOnly = true; text.value = state.text; text.setAttribute("aria-label", "语音识别结果");
        root.append(text, swivoxDictationControl("copy", "复制文字"));
      }
      root.append(swivoxDictationControl("cancel", state.phase === "result" ? "关闭" : "取消"));
    });
    const floating = document.getElementById("swivox-dictation-recovery");
    if (state.phase === "idle") floating?.remove();
    else if (!swivoxDictationTargetEditor() && !floating) {
      const root = createSwivoxDictationControls();
      root.id = "swivox-dictation-recovery"; root.dataset.floating = "true";
      document.body.append(root); renderSwivoxDictation();
    }
  }

  function createSwivoxDictationControls() {
    const root = document.createElement("div");
    root.dataset.swivoxExt = "builtin-dictation"; root.dataset.dictationControls = "true";
    root.className = "swivox-dictation-controls";
    root.addEventListener("pointerdown", (event) => { if (event.target.closest("button")) event.preventDefault(); });
    root.addEventListener("click", (event) => {
      const action = event.target.closest('[data-dictation-action]')?.dataset.dictationAction;
      if (!action) return;
      event.preventDefault(); event.stopPropagation();
      if (action === "start") void startSwivoxDictation(root.closest(".composer-footer, [data-composer-footer-responsive]"));
      else if (action === "cancel") cancelSwivoxDictation();
      else if (action === "retry") void transcribeSwivoxDictation();
      else if (action === "rerecord") {
        const footer = swivoxDictationTargetEditor()?.closest('[data-composer-surface-variant], form')?.querySelector('.composer-footer, [data-composer-footer-responsive]')
          || root.closest(".composer-footer, [data-composer-footer-responsive]");
        cancelSwivoxDictation();
        if (footer) void startSwivoxDictation(footer);
        else showToast("请返回目标会话后重新录音", null);
      } else if (action === "copy") {
        if (!navigator.clipboard?.writeText) { showToast("请选中识别结果手动复制", null); return; }
        void navigator.clipboard.writeText(swivoxDictation.text)
          .then(() => showToast("已复制识别结果", null)).catch(() => showToast("复制失败，请选中识别结果手动复制", null));
      } else stopSwivoxDictation(action);
    });
    return root;
  }

  function swivoxDictationFooterScore(footer, editor) {
    let score = footer.contains(editor) ? 4 : 0;
    const send = swivoxDictationSendButton(editor);
    if (send && footer.contains(send)) score += 2;
    return score;
  }

  function swivoxDictationFooterControls(footer) {
    const node = footer.querySelector('[data-dictation-controls]');
    return node && node.closest(".composer-footer, [data-composer-footer-responsive]") === footer ? node : null;
  }

  function installSwivoxDictation() {
    if (!swivoxDictationEnabled()) {
      if (swivoxDictation.phase !== "idle" || swivoxDictation.pendingSend) cancelSwivoxDictation();
      document.querySelectorAll('[data-swivox-ext="builtin-dictation"]').forEach((node) => node.remove());
      return;
    }
    if (!document.getElementById("swivox-dictation-style")) {
      const style = document.createElement("style"); style.id = "swivox-dictation-style";
      style.dataset.swivoxExt = "builtin-dictation";
      style.textContent = `.swivox-dictation-controls{display:flex;align-items:center;gap:6px;flex-wrap:wrap;margin-inline:6px;font-size:12px;max-width:100%}
        .swivox-dictation-controls button{border:1px solid color-mix(in srgb,currentColor 20%,transparent);border-radius:8px;padding:5px 8px;background:transparent;color:inherit;cursor:pointer;white-space:nowrap}
        .swivox-dictation-controls button:disabled{opacity:.5;cursor:default}.swivox-dictation-controls button:focus-visible{outline:2px solid currentColor;outline-offset:2px}
        /* 颜色取主文字色而不是继承 footer 的弱化文字：图标按钮没有边框和标签，
           再叠一层弱化会被误读成禁用态。 */
        .swivox-dictation-controls .swivox-dictation-mic{display:inline-flex;align-items:center;justify-content:center;width:28px;height:28px;padding:0;border:0;border-radius:8px;background:transparent;color:var(--color-token-text-primary,currentColor);opacity:.9;cursor:pointer}
        .swivox-dictation-controls .swivox-dictation-mic:hover{background:color-mix(in srgb,currentColor 12%,transparent);opacity:1}
        .swivox-dictation-controls .swivox-dictation-mic:disabled{opacity:.4;cursor:default;background:transparent}
        #swivox-dictation-recovery{position:fixed;bottom:24px;right:24px;z-index:2147483000;padding:12px;border-radius:12px;background:var(--background,#252525);color:var(--foreground,#eee);box-shadow:0 4px 24px #0006;max-width:480px}
        #swivox-dictation-recovery textarea{width:100%;min-height:80px;color:inherit;background:transparent}`;
      document.head.append(style);
    }
    // 同一输入框可能同时命中多个 footer（例如新版首页的顶部上下文栏和底部
    // 控制行都带 data-composer-footer-responsive），一个输入框只挂一个入口，
    // 否则会出现两个「语音输入」。优先挂在包含输入框的 footer 上，其次是
    // 包含发送按钮的；同分时取文档中更靠后的（更内层）候选项。
    const candidates = [];
    document.querySelectorAll(".composer-footer, [data-composer-footer-responsive]").forEach((footer) => {
      if (!visibleElement(footer) || isExtensionUiNode(footer)) return;
      const scope = swivoxDictationScope(footer);
      const editor = scope && swivoxDictationEditor(scope);
      if (!editor) return;
      candidates.push({ footer, editor, score: swivoxDictationFooterScore(footer, editor) });
    });
    const hosts = new Map();
    for (const item of candidates) {
      const current = hosts.get(item.editor);
      if (!current || item.score >= current.score) hosts.set(item.editor, item);
    }
    const hostFooters = new Set([...hosts.values()].map((item) => item.footer));
    document.querySelectorAll('[data-swivox-ext="builtin-dictation"][data-dictation-controls]').forEach((node) => {
      if (node.dataset.floating === "true") return;
      const owner = node.closest(".composer-footer, [data-composer-footer-responsive]");
      if (owner && !hostFooters.has(owner)) node.remove();
    });
    for (const item of hosts.values()) {
      let controls = swivoxDictationFooterControls(item.footer);
      if (!controls) { controls = createSwivoxDictationControls(); item.footer.append(controls); }
      const target = item.editor.dataset.swivoxDictationTarget || item.editor.closest('[data-swivox-dictation-root]')?.dataset.swivoxDictationRoot || "";
      if (controls.dataset.dictationTarget !== target) { controls.dataset.dictationTarget = target; delete controls.dataset.renderKey; }
    }
    renderSwivoxDictation();
  }

  const swivoxDictationKeyHandler = (event) => {
    if (event.key === "Escape" && (swivoxDictation.phase !== "idle" || swivoxDictation.pendingSend)) {
      event.preventDefault(); event.stopImmediatePropagation(); cancelSwivoxDictation();
      return;
    }
    const editor = swivoxDictationTargetEditor();
    if (event.key === "Enter" && !event.isComposing && !event.shiftKey && !event.ctrlKey && !event.metaKey && !event.altKey
        && editor && (document.activeElement === editor || editor.contains?.(document.activeElement))
        && ["requesting", "recording", "transcribing"].includes(swivoxDictation.phase)) {
      // 在录音期间，Enter 先结束转写再发送，不能让原生输入框先提交旧草稿。
      event.preventDefault(); event.stopImmediatePropagation();
      if (swivoxDictation.phase !== "requesting") stopSwivoxDictation("send");
    }
  };
  document.addEventListener("keydown", swivoxDictationKeyHandler, true);
  const swivoxDictationDispose = () => {
    cancelSwivoxDictation();
    document.removeEventListener("keydown", swivoxDictationKeyHandler, true);
    document.querySelectorAll('[data-swivox-ext="builtin-dictation"]').forEach((node) => node.remove());
  };
  window.addEventListener("pagehide", swivoxDictationDispose, { once: true });
  window.__swivoxDictationRuntime = { dispose: () => {
    window.removeEventListener("pagehide", swivoxDictationDispose);
    swivoxDictationDispose();
  } };
