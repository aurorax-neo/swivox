//! Antigravity 可逆界面汉化（零文件修改）与上下文监控圆环（CDP + 本地 `LanguageServerService` 只读 Connect-RPC）。

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use url::Url;

const DICT_UI_V2_JSON: &str = include_str!("../../../../assets/antigravity/dicts/ui_v2.json");
const DICT_COMMON_JSON: &str = include_str!("../../../../assets/antigravity/dicts/common.json");

const AG_RENDERER_TEMPLATE: &str = r#"(() => {
  if (window !== window.top) return;
  const DICT = __SWIVOX_AG_DICT__;
  const ENABLE_I18N = __SWIVOX_AG_ENABLE_I18N__;
  const ENABLE_CONTEXT = __SWIVOX_AG_ENABLE_CONTEXT__;
  const TARGET = __SWIVOX_AG_TARGET__;
  const BINDING = "__swivoxAgReport";

  const generation = (window.__easyagMountGeneration || 0) + 1;
  window.__easyagMountGeneration = generation;

  function installI18n(win, doc, dictionary, enabled) {
    if (!enabled) {
      win.__easyagI18n?.dispose();
      return;
    }
    if (win.__easyagI18n?.active === true) {
      win.__easyagI18n.scan?.(doc.body || doc.documentElement);
      return;
    }
    win.__easyagI18n?.dispose();
    const texts = new Map();
    const attributes = new Map();
    const ignored = 'script,style,pre,code,textarea,[contenteditable="true"],[data-testid*="message-content"],[data-testid="planner-response-text"],[data-testid="breadcrumb-segment"],[data-testid="project-header"],#swivox-ag-context-monitor';
    const ignoredElement = el => el?.closest?.(ignored);
    function lookup(key) {
      if (typeof dictionary[key] === 'string') return dictionary[key];
      const count = key.match(/^(\d+) active conversations$/);
      if (count && dictionary['{count} active conversations']) {
        return dictionary['{count} active conversations'].replace('{count}', count[1]);
      }
      const filesChanged = key.match(/^(\d+) files? changed$/i);
      if (filesChanged) {
        return `${filesChanged[1]} 个文件已更改`;
      }
      const quota = key.match(/^(You have used some of your (?:weekly|5-hour) limit, it will fully refresh in )(.+)\.$/);
      if (!quota) return null;
      const template = dictionary[quota[1] + '{time}.'];
      if (!template || !/^\d+ (?:days?|hours?|minutes?|seconds?)(?:, \d+ (?:days?|hours?|minutes?|seconds?))*$/.test(quota[2])) return null;
      const parts = quota[2].split(', ').map(part => {
        const [, number, unit] = part.match(/^(\d+) (\w+)$/);
        return dictionary['{count} ' + unit]?.replace('{count}', number);
      });
      return parts.every(Boolean) ? template.replace('{time}', parts.join('、')) : null;
    }
    function translateText(node) {
      if (ignoredElement(node.parentElement)) return;
      const raw = node.nodeValue;
      if (texts.get(node)?.translated === raw) return;
      const key = raw?.trim();
      const value = key && lookup(key);
      if (!value || value === key) return;
      const translated = raw.replace(key, value);
      texts.set(node, { original: raw, translated });
      node.nodeValue = translated;
    }
    function translateElement(element) {
      if (ignoredElement(element)) return;
      for (const name of ['placeholder', 'title', 'aria-label']) {
        const original = element.getAttribute(name);
        if (attributes.get(element)?.get(name)?.translated === original) continue;
        const key = original?.trim();
        const value = key && lookup(key);
        if (!value || value === key) continue;
        const translated = original.replace(key, value);
        if (!attributes.has(element)) attributes.set(element, new Map());
        attributes.get(element).set(name, { original, translated });
        element.setAttribute(name, translated);
      }
    }
    function translate(root) {
      if (!enabled || !root) return;
      if (root.nodeType === 3) return translateText(root);
      if (root.nodeType !== 1 && root.nodeType !== 9) return;
      if (root.nodeType === 1) translateElement(root);
      for (const el of root.querySelectorAll('[placeholder],[title],[aria-label]')) translateElement(el);
      const walker = doc.createTreeWalker(root, win.NodeFilter.SHOW_TEXT);
      while (walker.nextNode()) translateText(walker.currentNode);
    }
    const observer = new win.MutationObserver(mutations => {
      for (const m of mutations) {
        if (m.type === 'childList') for (const n of m.addedNodes) translate(n);
        if (m.type === 'characterData') translateText(m.target);
        if (m.type === 'attributes') translateElement(m.target);
      }
      for (const n of texts.keys()) if (!n.isConnected) texts.delete(n);
      for (const el of attributes.keys()) if (!el.isConnected) attributes.delete(el);
    });
    const runtime = {
      active: true,
      scan(root) {
        translate(root);
      },
      dispose() {
        runtime.active = false;
        observer.disconnect();
        for (const [node, rec] of texts) {
          if (node.nodeValue === rec.translated) node.nodeValue = rec.original;
        }
        for (const [el, recs] of attributes) {
          for (const [name, rec] of recs) {
            if (el.getAttribute(name) === rec.translated) el.setAttribute(name, rec.original);
          }
        }
        texts.clear();
        attributes.clear();
        if (win.__easyagI18n === runtime) delete win.__easyagI18n;
      }
    };
    win.__easyagI18n = runtime;
    translate(doc.body || doc.documentElement);
    observer.observe(doc.documentElement, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
      attributeFilter: ['placeholder', 'title', 'aria-label']
    });
  }

  function currentConversationId(win, doc, target) {
    const guid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
    if (target !== 'ide') {
      const id = win.location.pathname.match(/\/c\/([^/]+)/)?.[1];
      return id && guid.test(id) ? id.toLowerCase() : null;
    }
    if (!['vscode-file:', 'file:'].includes(win.location.protocol) || !/\/workbench\.html$/.test(win.location.pathname)) return null;
    const boxes = [...doc.querySelectorAll('[id="antigravity.agentSidePanelInputBox"]')]
      .filter(box => box.isConnected && box.getClientRects().length
        && !box.closest('[hidden],[inert]') && win.getComputedStyle(box).visibility !== 'hidden'
        && box.querySelector('[data-testid="model-selector-trigger"]'));
    if (boxes.length !== 1) return null;
    const box = boxes[0], root = box.closest('.antigravity-agent-side-panel');
    if (!root?.__k) return null;
    const queue = [root.__k], seen = new Set(), ids = new Set();
    while (queue.length && seen.size < 5000) {
      let vnode = queue.pop();
      if (!vnode || typeof vnode !== 'object' || seen.has(vnode)) continue;
      seen.add(vnode);
      if (typeof vnode.type === 'string' && vnode.__e && vnode.__e !== box
        && typeof vnode.__e.contains === 'function' && !vnode.__e.contains(box)) continue;
      if (vnode.__e === box) {
        for (let depth = 0; vnode && depth < 16; depth++, vnode = vnode.__) {
          if (Object.prototype.hasOwnProperty.call(vnode.props || {}, 'cascadeId')) {
            const id = vnode.props.cascadeId;
            if (typeof id !== 'string' || !guid.test(id)) return null;
            ids.add(id.toLowerCase());
            break;
          }
        }
      } else if (Array.isArray(vnode.__k)) queue.push(...vnode.__k);
    }
    return !queue.length && ids.size === 1 ? [...ids][0] : null;
  }

  const WIDGET_VERSION = 2;

  function installContextWidget(win, doc, enabled, report) {
    if (!enabled) {
      win.__easyagContext?.dispose();
      return;
    }
    if (win.__easyagContext?.active === true && win.__easyagContext?.version === WIDGET_VERSION) {
      win.__easyagContext.updateReporter?.(report);
      win.__easyagContext.syncNow?.();
      return;
    }
    win.__easyagContext?.dispose();
    const canDirectFetch = TARGET !== 'ide' && (win.location.protocol === 'https:' || win.location.protocol === 'http:');
    let currentReport = report;
    let node, shadow, button, card, barFill, composer, data, route = '', pending, lastFetch = 0, sequence = 0, disposed = false;
    let modelMap = {}, modelMapAt = 0, modelMapPromise = null;
    let cursor = { conversationId: '', nextOffset: 0, merged: null, maxByModel: {}, fullSyncAt: 0, useLegacyRpc: false };

    const visible = el => el?.isConnected && !el.closest('[hidden],[inert]') && el.getClientRects().length && win.getComputedStyle(el).visibility !== 'hidden';
    function safeLayout(pair) {
      const style = win.getComputedStyle(pair.row), bStyle = win.getComputedStyle(pair.branch);
      if (!['flex', 'inline-flex'].includes(style.display) || style.flexDirection !== 'row'
        || ['absolute', 'fixed'].includes(style.position) || ['absolute', 'fixed'].includes(bStyle.position)
        || pair.row.closest('header,[role="banner"]')) return false;
      const r = pair.row.getBoundingClientRect(), e = pair.editor.getBoundingClientRect(), p = pair.row.parentElement.getBoundingClientRect();
      return r.width > 0 && r.height > 0 && e.width > 0 && e.height > 0
        && Math.max(0, r.top - e.bottom, e.top - r.bottom) <= 48
        && Math.min(r.bottom, e.bottom) - Math.max(r.top, e.top) <= 2
        && Math.min(r.right, e.right) > Math.max(r.left, e.left) && p.height <= e.height + r.height + 96;
    }
    function resolveInsertAnchor(row, branch, anchor) {
      if (branch !== anchor) {
        const bStyle = win.getComputedStyle(branch);
        if (['flex', 'inline-flex'].includes(bStyle.display) && bStyle.flexDirection === 'row') {
          let sub = anchor;
          while (sub?.parentElement && sub.parentElement !== branch) sub = sub.parentElement;
          if (sub?.parentElement === branch) return { insertParent: branch, insertAfter: sub };
        }
      }
      return { insertParent: row, insertAfter: branch };
    }
    function findComposer() {
      const editors = [...doc.querySelectorAll('[contenteditable="true"][role="textbox"],[contenteditable="true"][role="combobox"]')].filter(visible);
      const pairs = [];
      for (const anchor of doc.querySelectorAll('[data-testid="model-selector-trigger"]')) {
        if (!visible(anchor)) continue;
        let bestForAnchor = null;
        for (const editor of editors) {
          let row = anchor.parentElement, distance = 0;
          while (row?.parentElement && !row.parentElement.contains(editor) && row.parentElement !== doc.body) { row = row.parentElement; distance++; }
          if (!row || row.contains(editor) || !row.parentElement?.contains(editor) || [doc.body, doc.documentElement].includes(row.parentElement)) continue;
          let branch = anchor;
          while (branch.parentElement !== row) branch = branch.parentElement;
          const slot = resolveInsertAnchor(row, branch, anchor);
          const pair = {
            row, branch, editor, anchor, distance,
            insertParent: slot.insertParent,
            insertAfter: slot.insertAfter,
            isFooter: row.classList?.contains('agy-composer-footer') || Boolean(row.closest?.('.agy-composer-footer')),
            bottom: row.getBoundingClientRect().bottom
          };
          if (safeLayout(pair) && (!bestForAnchor || pair.distance < bestForAnchor.distance)) bestForAnchor = pair;
        }
        if (bestForAnchor) pairs.push(bestForAnchor);
      }
      if (!pairs.length) return null;
      if (TARGET === 'ide') {
        pairs.sort((a, b) => a.distance - b.distance);
        return pairs.length === 1 || pairs[0].distance < pairs[1].distance ? pairs[0] : null;
      }
      pairs.sort((a, b) => (Number(b.isFooter) - Number(a.isFooter)) || (b.bottom - a.bottom) || (a.distance - b.distance));
      return pairs[0];
    }
    function hideCard() {
      if (!card) return;
      if (card.hasAttribute('popover') && card.matches(':popover-open')) card.hidePopover();
      card.hidden = true;
      button?.setAttribute('aria-expanded', 'false');
    }
    function positionCard() {
      if (!card || card.hidden || !node?.isConnected) return;
      const r = button.getBoundingClientRect(), width = Math.min(312, win.innerWidth - 16);
      card.style.width = Math.max(0, width) + 'px';
      card.style.left = Math.max(8, Math.min(r.left, win.innerWidth - width - 8)) + 'px';
      const height = card.getBoundingClientRect().height;
      const preferTop = r.top - height - 8;
      card.style.top = (preferTop >= 8 ? preferTop : Math.max(8, Math.min(r.bottom + 8, win.innerHeight - height - 8))) + 'px';
    }
    function showCard() {
      if (!node?.isConnected || node.hidden) return;
      hideCard();
      card.hidden = false;
      if (card.hasAttribute('popover') && !card.matches(':popover-open')) card.showPopover();
      button.setAttribute('aria-expanded', 'true');
      positionCard();
      if (canDirectFetch && route && Date.now() - cursor.fullSyncAt > 5000) refresh(true);
    }
    const onDocPointerDown = ev => {
      if (!card || card.hidden || !node) return;
      const path = ev.composedPath?.();
      if (Array.isArray(path) ? !path.includes(node) : !node.contains(ev.target)) hideCard();
    };
    const onDocKeyDown = ev => {
      if (ev.key === 'Escape' && card && !card.hidden) hideCard();
    };
    doc.addEventListener('pointerdown', onDocPointerDown, true);
    doc.addEventListener('keydown', onDocKeyDown);

    const compact = v => {
      if (typeof v !== 'number' || !Number.isFinite(v)) return '0';
      if (v >= 1000000) return (v / 1000000).toFixed(2).replace(/\.?0+$/, '') + 'M';
      if (v >= 1000) return (v / 1000).toFixed(1).replace(/\.0$/, '') + 'K';
      return String(v);
    };
    const parseIntVal = v => {
      if (typeof v === 'number' && Number.isFinite(v) && v >= 0) return Math.floor(v);
      if (typeof v === 'string') {
        const n = Number(v.trim());
        if (Number.isFinite(n) && n >= 0) return Math.floor(n);
      }
      return null;
    };
    const parseDurSecs = v => {
      if (typeof v === 'string') {
        const m = v.trim().match(/^(-?\d+(?:\.\d+)?)s$/);
        return m ? Number(m[1]) : null;
      }
      if (v && typeof v === 'object') {
        const s = Number(v.seconds || 0), ns = Number(v.nanos || 0);
        return Number.isFinite(s) && Number.isFinite(ns) ? s + ns / 1e9 : null;
      }
      return null;
    };
    const readCsrfToken = () => {
      try {
        return win.localStorage?.getItem('agy_session_csrf_token')
          || win.sessionStorage?.getItem('agy_session_csrf_token') || '';
      } catch {
        return '';
      }
    };
    async function rpcPost(method, body) {
      const headers = {
        'Content-Type': 'application/json',
        'Connect-Protocol-Version': '1'
      };
      const csrf = readCsrfToken();
      if (csrf) headers['x-codeium-csrf-token'] = csrf;
      const resp = await win.fetch(`/exa.language_server_pb.LanguageServerService/${method}`, {
        method: 'POST',
        credentials: 'include',
        headers,
        body: JSON.stringify(body)
      });
      if (!resp.ok) {
        const err = new Error(`HTTP ${resp.status}`);
        err.status = resp.status;
        throw err;
      }
      return resp.json();
    }
    async function ensureModelMap() {
      if (Date.now() - modelMapAt < 300000 && Object.keys(modelMap).length) return modelMap;
      if (modelMapPromise) return modelMapPromise;
      modelMapPromise = (async () => {
        try {
          const val = await rpcPost('GetUserStatus', {});
          const configs = val?.userStatus?.cascadeModelConfigData?.clientModelConfigs;
          if (Array.isArray(configs)) {
            const next = {};
            for (const cfg of configs) {
              const label = typeof cfg?.label === 'string' ? cfg.label : '';
              if (!label) continue;
              const m = cfg?.modelOrAlias?.model;
              const id = cfg?.modelId;
              if (typeof m === 'string' && m) next[m] = label;
              if (typeof id === 'string' && id) next[id] = label;
            }
            modelMap = next;
            modelMapAt = Date.now();
          }
        } catch {}
        modelMapPromise = null;
        return modelMap;
      })();
      return modelMapPromise;
    }
    function buildMetricsFromItems(convId, items, labels, maxByModel, previous) {
      for (const item of items) {
        const chat = item?.chatModel;
        if (!chat) continue;
        const cwm = chat?.chatStartMetadata?.contextWindowMetadata || item?.chatStartMetadata?.contextWindowMetadata;
        const rawModel = chat.modelDisplayName || chat.modelName || chat.model || '';
        const maxT = parseIntVal(cwm?.maxContextTokens);
        if (rawModel && maxT && maxT > 0) maxByModel[rawModel] = maxT;
      }
      let contextVal = null;
      let speedVal = null;
      for (let i = items.length - 1; i >= 0; i--) {
        const item = items[i];
        const chat = item?.chatModel;
        if (!chat) continue;
        if (!contextVal) {
          const cwm = chat?.chatStartMetadata?.contextWindowMetadata || item?.chatStartMetadata?.contextWindowMetadata;
          const u = chat?.usage;
          const uIn = parseIntVal(u?.inputTokens) || 0;
          const uCache = parseIntVal(u?.cacheReadTokens) || 0;
          const usageFallback = uIn + uCache > 0 ? uIn + uCache : null;
          const est = parseIntVal(cwm?.estimatedTokensUsed)
            ?? parseIntVal(cwm?.tokenBreakdown?.totalTokens)
            ?? usageFallback;
          if (cwm || est !== null) {
            const used = est ?? 0;
            const rawModel = chat.modelDisplayName || chat.modelName || chat.model || '';
            const maxTokens = parseIntVal(cwm?.maxContextTokens)
              || (rawModel ? maxByModel[rawModel] : null)
              || null;
            const usedFraction = maxTokens && maxTokens > 0 ? Math.max(0, Math.min(1, used / maxTokens)) : null;
            const model = labels[rawModel] || rawModel || '';
            const rawEnd = chat.responseModel || chat?.usage?.responseModel || '';
            const endpointModel = typeof rawEnd === 'string' && !rawEnd.startsWith('MODEL_PLACEHOLDER_') ? rawEnd : '';
            contextVal = { usedTokens: used, maxTokens, usedFraction, model, endpointModel };
          }
        }
        if (!speedVal) {
          const dur = parseDurSecs(chat.streamingDuration);
          const u = chat.usage;
          if (dur !== null && dur > 0.05 && u) {
            const outTokens = parseIntVal(u.responseOutputTokens) ?? parseIntVal(u.outputTokens) ?? 0;
            if (outTokens > 0) {
              const ttft = parseDurSecs(chat.timeToFirstToken);
              speedVal = {
                tokensPerSecond: outTokens / dur,
                ttftSeconds: ttft,
                basis: 'response',
                shortSample: dur < 1.0
              };
            }
          }
        }
        if (contextVal && speedVal) break;
      }
      if (!items.length && !previous) {
        return {
          conversationId: convId,
          state: 'empty',
          sampledAt: Date.now(),
          context: { usedTokens: 0, maxTokens: null, usedFraction: 0, model: '', endpointModel: '' },
          speed: null
        };
      }
      return {
        conversationId: convId,
        state: 'live',
        sampledAt: Date.now(),
        context: contextVal || previous?.context || null,
        speed: speedVal || previous?.speed || null
      };
    }
    async function fetchDirectMetrics(convId, forceFull) {
      if (cursor.conversationId !== convId) {
        cursor = { conversationId: convId, nextOffset: 0, merged: null, maxByModel: {}, fullSyncAt: 0, useLegacyRpc: cursor.useLegacyRpc };
      }
      await ensureModelMap();
      const periodicFull = Date.now() - cursor.fullSyncAt > 60000;
      const offset = forceFull || periodicFull || !cursor.merged ? 0 : cursor.nextOffset;
      let items = null;
      if (!cursor.useLegacyRpc) {
        try {
          const res = await rpcPost('GetCascadeTrajectoryGeneratorMetadata', {
            cascadeId: convId,
            generatorMetadataOffset: offset,
            includeMessages: false
          });
          if (Array.isArray(res?.generatorMetadata) || (res && typeof res === 'object' && Object.keys(res).length === 0)) {
            items = Array.isArray(res?.generatorMetadata) ? res.generatorMetadata : [];
          }
        } catch (err) {
          if (err?.status === 404 || err?.status === 501) cursor.useLegacyRpc = true;
          else throw err;
        }
      }
      let activeOffset = offset;
      if (items === null) {
        activeOffset = 0;
        const legacy = await rpcPost('GetCascadeTrajectory', { cascadeId: convId, trajectoryVerbosity: 2 });
        items = Array.isArray(legacy?.trajectory?.generatorMetadata) ? legacy.trajectory.generatorMetadata : [];
      }
      if (activeOffset === 0) cursor.fullSyncAt = Date.now();
      if (activeOffset > 0 && items.length === 0 && cursor.merged) {
        return { ...cursor.merged, conversationId: convId, sampledAt: Date.now() };
      }
      const nextMerged = buildMetricsFromItems(
        convId,
        items,
        modelMap,
        cursor.maxByModel,
        activeOffset > 0 ? cursor.merged : null
      );
      cursor.merged = nextMerged;
      if (items.length > 0) {
        const tailChat = items[items.length - 1]?.chatModel;
        const tailDone = Boolean(tailChat?.streamingDuration && tailChat?.usage);
        cursor.nextOffset = tailDone ? activeOffset + items.length : Math.max(0, activeOffset + items.length - 1);
      } else if (activeOffset === 0) {
        cursor.nextOffset = 0;
      }
      return nextMerged;
    }

    function render() {
      if (!shadow) return;
      const context = data?.context, speed = data?.speed, isEmpty = data?.state === 'empty';
      const hasFrac = typeof context?.usedFraction === 'number';
      const pct = hasFrac ? Math.round(context.usedFraction * 100) : (isEmpty ? 0 : null);
      const valText = pct !== null ? `${Math.min(999, pct)}${pct > 999 ? '+' : ''}%` : '—';
      shadow.getElementById('context-value').textContent = valText;
      const frac = hasFrac ? Math.max(0, Math.min(1, context.usedFraction)) : 0;
      const fill = shadow.getElementById('context-fill');
      fill.style.strokeDashoffset = (47.124 * (1 - frac)).toFixed(3);
      button.dataset.tone = pct !== null && pct >= 90 ? 'high' : pct !== null && pct >= 75 ? 'medium' : 'normal';
      button.dataset.empty = pct === null ? 'true' : (frac > 0 ? 'false' : 'zero');
      button.title = pct !== null
        ? (isEmpty ? 'Swivox 上下文监控：新会话 0% · 点击查看详情' : `Swivox 上下文占用 ${pct}% · 点击查看详情`)
        : 'Swivox 上下文估算暂不可用 · 点击查看详情';
      if (barFill) {
        barFill.style.width = `${(frac * 100).toFixed(1)}%`;
        barFill.dataset.tone = button.dataset.tone;
      }
      shadow.getElementById('usage').textContent = isEmpty
        ? '0 tokens（新会话）'
        : context
          ? `${compact(context.usedTokens)} / ${context.maxTokens ? compact(context.maxTokens) : '未知'} tokens`
          : '暂不可用';
      const domModel = typeof composer?.anchor?.textContent === 'string' ? composer.anchor.textContent.replace(/\s+/g, ' ').trim().slice(0, 80) : '';
      shadow.getElementById('model').textContent = context?.model || speed?.model || domModel || '—';
      shadow.getElementById('endpoint').textContent = context?.endpointModel || speed?.endpointModel || '—';
      shadow.getElementById('speed').textContent = speed ? `${speed.tokensPerSecond.toFixed(1)} tok/s` : (isEmpty ? '尚无生成记录' : '等待有效请求');
      shadow.getElementById('ttft').textContent = typeof speed?.ttftSeconds === 'number' ? `${speed.ttftSeconds.toFixed(2)} 秒` : '—';
      positionCard();
    }
    function createNode() {
      node = doc.createElement('span');
      node.id = 'swivox-ag-context-monitor';
      node.style.cssText = 'display:inline-flex;align-items:center;flex:0 0 auto;min-width:0;margin:0 2px;position:relative;overflow:visible;';
      shadow = node.attachShadow({ mode: 'open' });
      function make(tag, attrs, ...children) {
        const el = ['svg', 'circle'].includes(tag) ? doc.createElementNS('http://www.w3.org/2000/svg', tag) : doc.createElement(tag);
        for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
        for (const c of children) el.appendChild(typeof c === 'string' ? doc.createTextNode(c) : c);
        return el;
      }
      shadow.appendChild(make('style', {}, `
        :host{font:12px/1.2 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;color:inherit}
        .strip{display:inline-flex;align-items:center}
        button{font:inherit;color:inherit;cursor:pointer;border:0;background:transparent}
        .indicator{display:inline-flex;align-items:center;gap:5px;height:28px;padding:0 8px 0 6px;border-radius:9999px;font-size:12px;font-weight:500;font-variant-numeric:tabular-nums;color:var(--vscode-descriptionForeground,color-mix(in srgb,var(--foreground,currentColor) 76%,transparent));transition:background .15s ease,color .15s ease}
        .indicator:hover{background:var(--vscode-toolbar-hoverBackground,color-mix(in srgb,var(--foreground,currentColor) 8%,transparent));color:var(--vscode-foreground,var(--foreground,currentColor))}
        .indicator svg{width:15px;height:15px;flex:0 0 auto}
        .ring-track{stroke:currentColor;opacity:.2}
        .ring-fill{transform:rotate(-90deg);transform-origin:10px 10px;stroke:#2563eb;transition:stroke-dashoffset .35s ease,stroke .2s ease}
        .indicator[data-empty=zero] .ring-fill,.indicator[data-empty=true] .ring-fill{stroke:currentColor;opacity:.45}
        .indicator[data-tone=medium]{color:#d97706}
        .indicator[data-tone=medium] .ring-fill{stroke:#d97706}
        .indicator[data-tone=high]{color:#dc2626}
        .indicator[data-tone=high] .ring-fill{stroke:#dc2626}
        .card{box-sizing:border-box;position:fixed;inset:auto;margin:0;z-index:2147483000;padding:12px 14px;border-radius:12px;border:1px solid var(--vscode-widget-border,color-mix(in srgb,var(--foreground,currentColor) 14%,transparent));background:var(--vscode-editorWidget-background,var(--card,var(--background,Canvas)));color:var(--vscode-editor-foreground,var(--foreground,CanvasText));box-shadow:0 10px 28px rgba(0,0,0,.14),0 2px 6px rgba(0,0,0,.06);font:12px/1.4 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif}
        [hidden]{display:none!important}
        .head{display:flex;justify-content:space-between;align-items:center;gap:8px;margin-bottom:8px}
        .head strong{font-size:12.5px;font-weight:600}
        .close-btn{width:22px;height:22px;display:inline-flex;align-items:center;justify-content:center;border-radius:6px;font-size:15px;line-height:1;opacity:.65}
        .close-btn:hover{opacity:1;background:color-mix(in srgb,var(--foreground,currentColor) 8%,transparent)}
        .bar-track{height:4px;border-radius:9999px;background:color-mix(in srgb,var(--foreground,currentColor) 10%,transparent);overflow:hidden;margin-bottom:10px}
        .bar-fill{height:100%;width:0;border-radius:9999px;background:#2563eb;transition:width .3s ease,background .2s ease}
        .bar-fill[data-tone=medium]{background:#d97706}
        .bar-fill[data-tone=high]{background:#dc2626}
        dl{margin:0}
        dl div{display:flex;gap:12px;justify-content:space-between;align-items:baseline;margin:6px 0}
        dl div:last-child{margin-bottom:0}
        dt{opacity:.68;flex:0 0 auto}
        dd{margin:0;max-width:66%;text-align:right;font-variant-numeric:tabular-nums;overflow-wrap:anywhere;font-weight:500}
      `));
      shadow.appendChild(make('div', { class: 'strip' },
        make('button', { id: 'trigger', class: 'indicator', type: 'button', 'aria-expanded': 'false', 'data-tone': 'normal', 'data-empty': 'true' },
          make('svg', { viewBox: '0 0 20 20', 'aria-hidden': 'true' },
            make('circle', { class: 'ring-track', cx: '10', cy: '10', r: '7.5', fill: 'none', 'stroke-width': '2' }),
            make('circle', { id: 'context-fill', class: 'ring-fill', cx: '10', cy: '10', r: '7.5', fill: 'none', 'stroke-width': '2', 'stroke-linecap': 'round', 'stroke-dasharray': '47.124', 'stroke-dashoffset': '47.124' })
          ),
          make('span', { id: 'context-value' }, '—')
        )
      ));
      shadow.appendChild(make('section', { id: 'details', class: 'card', hidden: '' },
        make('div', { class: 'head' }, make('strong', {}, 'Swivox 上下文监控'), make('button', { id: 'close', class: 'close-btn', type: 'button', 'aria-label': '关闭' }, '×')),
        make('div', { class: 'bar-track' }, make('div', { id: 'bar-fill', class: 'bar-fill' })),
        make('dl', {},
          make('div', {}, make('dt', {}, '占用 / 容量'), make('dd', { id: 'usage' })),
          make('div', {}, make('dt', {}, '采样模型'), make('dd', { id: 'model' })),
          make('div', {}, make('dt', {}, '底层端点模型'), make('dd', { id: 'endpoint' })),
          make('div', {}, make('dt', {}, '最近生成速度'), make('dd', { id: 'speed' })),
          make('div', {}, make('dt', {}, '首 token 等待'), make('dd', { id: 'ttft' }))
        )
      ));
      button = shadow.getElementById('trigger');
      card = shadow.getElementById('details');
      barFill = shadow.getElementById('bar-fill');
      if (typeof card.showPopover === 'function') card.setAttribute('popover', 'manual');
      button.addEventListener('click', () => { if (card.hidden) showCard(); else hideCard(); });
      shadow.getElementById('close').addEventListener('click', () => hideCard());
      for (const t of ['pointerdown', 'mousedown', 'click']) node.addEventListener(t, ev => ev.stopPropagation());
      render();
    }
    function mount() {
      const next = currentConversationId(win, doc, TARGET);
      if (next !== route) { route = next; pending = null; data = null; lastFetch = 0; hideCard(); render(); }
      const found = route && findComposer();
      if (!found) { hideCard(); node?.remove(); composer = null; return; }
      const changed = !node || !node.isConnected
        || node.parentElement !== found.insertParent
        || node.previousElementSibling !== found.insertAfter;
      composer = found;
      if (!node) createNode();
      if (changed) {
        hideCard();
        found.insertParent.insertBefore(node, found.insertAfter.nextSibling);
        render();
      }
    }
    function refresh(force = false) {
      if (!enabled || disposed || (!force && doc.hidden) || !route || !node?.isConnected) return;
      if (force) pending = null;
      if (pending && Date.now() - pending.at < 15000) return;
      if (!force && Date.now() - lastFetch < 2400) return;
      const requestId = String(++sequence);
      const targetRoute = route;
      pending = { requestId, conversationId: targetRoute, at: Date.now() };
      lastFetch = Date.now();
      if (canDirectFetch) {
        fetchDirectMetrics(targetRoute, force)
          .then(res => receive(requestId, res))
          .catch(() => {
            if (pending?.requestId === requestId) pending = null;
            if (!data && route === targetRoute) {
              data = { conversationId: targetRoute, state: 'unavailable' };
              render();
            }
          });
        return;
      }
      try { currentReport({ type: 'context-metrics', requestId, conversationId: targetRoute }); } catch { pending = null; }
    }
    function receive(requestId, result) {
      if (disposed || !pending || pending.requestId !== requestId || result?.conversationId !== pending.conversationId) return;
      pending = null; data = result; render();
    }
    const timer = win.setInterval(() => {
      mount();
      if (canDirectFetch) refresh(false);
    }, canDirectFetch ? 2500 : 1500);
    win.__easyagContext = {
      active: true,
      version: WIDGET_VERSION,
      hasRoute() {
        return Boolean(route && node?.isConnected);
      },
      usesHostBridge() {
        return !canDirectFetch;
      },
      updateReporter(fn) {
        if (typeof fn === 'function') currentReport = fn;
      },
      syncNow() {
        mount();
        refresh(true);
      },
      receive,
      dispose() {
        disposed = true;
        doc.removeEventListener('pointerdown', onDocPointerDown, true);
        doc.removeEventListener('keydown', onDocKeyDown);
        hideCard();
        node?.remove();
        win.clearInterval(timer);
        delete win.__easyagContext;
      }
    };
    mount(); refresh(true);
  }

  function boot() {
    if (window.__easyagMountGeneration !== generation) return;
    if (TARGET === 'ide') {
      window.__easyagI18n?.dispose();
    } else {
      installI18n(window, document, DICT, ENABLE_I18N);
    }
    installContextWidget(window, document, ENABLE_CONTEXT, ev => {
      if (typeof window[BINDING] === 'function') window[BINDING](JSON.stringify(ev));
    });
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot, { once: true });
    return { hasRoute: false, usesHostBridge: TARGET === 'ide' };
  }
  boot();
  return {
    hasRoute: Boolean(window.__easyagContext?.hasRoute?.()),
    usesHostBridge: Boolean(window.__easyagContext?.usesHostBridge?.())
  };
})();"#;

fn merged_dictionary_json() -> &'static String {
    static DICT_CACHE: OnceLock<String> = OnceLock::new();
    DICT_CACHE.get_or_init(|| {
        let mut merged = BTreeMap::<String, String>::new();
        for raw in [DICT_UI_V2_JSON, DICT_COMMON_JSON] {
            if let Ok(map) = serde_json::from_str::<BTreeMap<String, Value>>(raw) {
                for (k, v) in map {
                    if let Some(s) = v.as_str()
                        && k.len() <= 2000
                        && s.len() <= 2000
                    {
                        merged.insert(k, s.to_string());
                    }
                }
            }
        }
        serde_json::to_string(&merged).unwrap_or_else(|_| "{}".to_string())
    })
}

pub fn build_renderer_injection_script(
    enable_i18n: bool,
    enable_context_ring: bool,
    is_ide: bool,
) -> String {
    let dict = if enable_i18n && !is_ide {
        merged_dictionary_json().as_str()
    } else {
        "{}"
    };
    AG_RENDERER_TEMPLATE
        .replace("__SWIVOX_AG_DICT__", dict)
        .replace(
            "__SWIVOX_AG_ENABLE_I18N__",
            if enable_i18n && !is_ide { "true" } else { "false" },
        )
        .replace(
            "__SWIVOX_AG_ENABLE_CONTEXT__",
            if enable_context_ring { "true" } else { "false" },
        )
        .replace("__SWIVOX_AG_TARGET__", if is_ide { "\"ide\"" } else { "\"hub\"" })
}

#[derive(Debug, Deserialize)]
struct CdpPageTarget {
    #[serde(default, rename = "type")]
    target_type: String,
    #[serde(default)]
    url: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    ws_url: String,
}

fn is_safe_loopback_ws(ws: &str) -> bool {
    let Ok(u) = Url::parse(ws) else { return false };
    u.scheme() == "ws"
        && matches!(
            u.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("[::1]")
        )
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityEnhanceStatus {
    pub connected: bool,
    pub cdp_port: u16,
    pub attached_pages: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

static ENHANCE_SUPERVISOR_RUNNING: OnceLock<Mutex<bool>> = OnceLock::new();

#[derive(Debug, Clone)]
struct LanguageServerEndpoint {
    pid: u32,
    base_url: String,
    is_https: bool,
    csrf: String,
}

static CACHED_LS_ENDPOINT: OnceLock<Mutex<Option<LanguageServerEndpoint>>> = OnceLock::new();

/// 只向 Electron 内部协议或本地回环加载的应用页注入：调试端口若被其他浏览器占用，
/// 外部网页（http/https）、data/blob 闪屏页、chrome:// 与 devtools:// 页面一律跳过。
fn is_antigravity_renderer_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    if lower.is_empty()
        || lower.starts_with("chrome")
        || lower.starts_with("devtools://")
        || lower.starts_with("about:")
        || lower.starts_with("data:")
        || lower.starts_with("blob:")
    {
        return false;
    }
    if lower.starts_with("http://") || lower.starts_with("https://") {
        // 应用自带的本地页面（回环地址）放行；外部网站一律不注入。
        return url::Url::parse(&lower)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_string))
            .is_some_and(|host| matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]"));
    }
    true
}

async fn is_non_codex_cdp_endpoint(client: &reqwest::Client, port: u16) -> bool {
    let ver_url = format!("http://127.0.0.1:{port}/json/version");
    let Ok(resp) = client.get(&ver_url).send().await else {
        return true;
    };
    let Ok(val) = resp.json::<Value>().await else {
        return true;
    };
    let ua = val
        .get("User-Agent")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    !ua.contains("codex") && !ua.contains("chatgpt")
}

/// 自动探测运行中 Antigravity（含 `DevToolsActivePort` 随机端口与显式配置端口）的所有页面，
/// 即时同步最新可逆汉化与上下文圆环脚本（开关关闭时立即无损卸载还原原文）。
pub async fn apply_cdp_enhancements_once(
    cdp_port: u16,
    enable_i18n: bool,
    enable_context_ring: bool,
) -> AntigravityEnhanceStatus {
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(1200))
        .build()
    else {
        return AntigravityEnhanceStatus {
            connected: false,
            cdp_port,
            attached_pages: 0,
            last_error: Some("初始化本地 HTTP 客户端失败".to_string()),
        };
    };

    let mut candidate_ports = super::process::discover_active_cdp_ports(cdp_port);
    if candidate_ports.is_empty() && cdp_port > 0 {
        candidate_ports.push(cdp_port);
    }

    let mut attached_pages = 0usize;
    let mut active_port: Option<u16> = None;

    for port in candidate_ports {
        if !is_non_codex_cdp_endpoint(&client, port).await {
            continue;
        }
        let list_url = format!("http://127.0.0.1:{port}/json/list");
        let Ok(resp) = client.get(&list_url).send().await else {
            continue;
        };
        let Ok(targets) = resp.json::<Vec<CdpPageTarget>>().await else {
            continue;
        };

        for target in targets {
            if target.target_type != "page"
                || !is_safe_loopback_ws(&target.ws_url)
                || !is_antigravity_renderer_url(&target.url)
            {
                continue;
            }
            let is_ide = target.url.ends_with("/workbench.html");
            let script = build_renderer_injection_script(enable_i18n, enable_context_ring, is_ide);
            if eval_on_cdp_page(&target.ws_url, &script, enable_context_ring)
                .await
                .is_ok()
            {
                attached_pages += 1;
                if active_port.is_none() {
                    active_port = Some(port);
                }
            }
        }
    }

    AntigravityEnhanceStatus {
        connected: attached_pages > 0,
        cdp_port: active_port.unwrap_or(cdp_port),
        attached_pages,
        last_error: None,
    }
}

async fn eval_on_cdp_page(ws_url: &str, script: &str, bind_context: bool) -> anyhow::Result<()> {
    let (mut ws, _) = tokio::time::timeout(
        Duration::from_millis(1500),
        tokio_tungstenite::connect_async(ws_url),
    )
    .await??;

    let commands = if bind_context {
        vec![
            json!({ "id": 1, "method": "Runtime.enable", "params": {} }),
            json!({ "id": 2, "method": "Page.enable", "params": {} }),
            json!({ "id": 3, "method": "Runtime.addBinding", "params": { "name": "__swivoxAgReport" } }),
            json!({ "id": 4, "method": "Runtime.evaluate", "params": { "expression": script, "returnByValue": true } }),
        ]
    } else {
        vec![
            json!({ "id": 1, "method": "Runtime.enable", "params": {} }),
            json!({ "id": 2, "method": "Runtime.evaluate", "params": { "expression": script, "returnByValue": true } }),
        ]
    };

    for cmd in &commands {
        ws.send(Message::Text(cmd.to_string().into())).await?;
    }

    // 读取响应并处理立即到达的 context-metrics 查询请求；无活跃会话或回写完成后立即退出。
    let deadline = tokio::time::Instant::now() + Duration::from_millis(2200);
    let mut metrics_requested = false;
    while tokio::time::Instant::now() < deadline {
        let Ok(Some(Ok(Message::Text(txt)))) =
            tokio::time::timeout_at(deadline, ws.next()).await
        else {
            break;
        };
        if let Ok(msg) = serde_json::from_str::<Value>(&txt) {
            if msg.get("method").and_then(Value::as_str) == Some("Runtime.bindingCalled")
                && let Some(payload_str) = msg
                    .get("params")
                    .and_then(|p| p.get("payload"))
                    .and_then(Value::as_str)
                && let Ok(ev) = serde_json::from_str::<Value>(payload_str)
                && ev.get("type").and_then(Value::as_str) == Some("context-metrics")
            {
                let req_id = ev
                    .get("requestId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let conv_id = ev
                    .get("conversationId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if !req_id.is_empty() && !conv_id.is_empty() {
                    metrics_requested = true;
                    let metrics = query_local_language_server_metrics(&conv_id).await;
                    let reply_expr = format!(
                        "window.__easyagContext?.receive({}, {})",
                        serde_json::to_string(&req_id).unwrap_or_default(),
                        serde_json::to_string(&metrics).unwrap_or_default()
                    );
                    let _ = ws
                        .send(Message::Text(
                            json!({
                                "id": 99,
                                "method": "Runtime.evaluate",
                                "params": { "expression": reply_expr }
                            })
                            .to_string()
                            .into(),
                        ))
                        .await;
                }
            }
            let msg_id = msg.get("id").and_then(Value::as_i64);
            if !bind_context && msg_id == Some(commands.len() as i64) {
                break;
            }
            if bind_context && msg_id == Some(99) {
                break;
            }
            if bind_context
                && msg_id == Some(4)
                && !metrics_requested
                && let Some(val) = msg.pointer("/result/result/value")
                && (val.get("hasRoute").and_then(Value::as_bool) == Some(false)
                    || val.get("usesHostBridge").and_then(Value::as_bool) == Some(false))
            {
                break;
            }
        }
    }

    let _ = ws.close(None).await;
    Ok(())
}

fn build_ls_http_client(is_https: bool, timeout: Duration) -> Option<reqwest::Client> {
    let mut builder = reqwest::Client::builder().no_proxy().timeout(timeout);
    if is_https {
        // 仅用于连接 127.0.0.1 上的本地 language_server 自签名回环端口。
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder.build().ok()
}

fn apply_ls_headers(req: reqwest::RequestBuilder, csrf: &str) -> reqwest::RequestBuilder {
    let req = req
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1");
    if csrf.is_empty() {
        req
    } else {
        req.header("x-codeium-csrf-token", csrf)
    }
}

/// 通过本机 `language_server` 只读 Connect-RPC (`GetUserStatus` + `GetCascadeTrajectoryGeneratorMetadata` / `GetCascadeTrajectory`) 查询会话上下文估算与生成速度。
async fn query_local_language_server_metrics(conversation_id: &str) -> Value {
    let Some(ep) = discover_local_language_server_endpoint().await else {
        return json!({
            "conversationId": conversation_id,
            "state": "unavailable"
        });
    };

    let Some(client) = build_ls_http_client(ep.is_https, Duration::from_millis(1800)) else {
        return json!({ "conversationId": conversation_id, "state": "unavailable" });
    };

    let base = &ep.base_url;
    let mut placeholder_labels = BTreeMap::<String, String>::new();
    if let Ok(resp) = apply_ls_headers(client.post(format!("{base}/GetUserStatus")), &ep.csrf)
        .json(&json!({}))
        .send()
        .await
        && let Ok(val) = resp.json::<Value>().await
        && let Some(configs) = val
            .pointer("/userStatus/cascadeModelConfigData/clientModelConfigs")
            .and_then(Value::as_array)
    {
        for cfg in configs {
            if let Some(label) = cfg.get("label").and_then(Value::as_str) {
                if let Some(model) = cfg.pointer("/modelOrAlias/model").and_then(Value::as_str) {
                    placeholder_labels.insert(model.to_string(), label.to_string());
                }
                if let Some(model_id) = cfg.get("modelId").and_then(Value::as_str) {
                    placeholder_labels.insert(model_id.to_string(), label.to_string());
                }
            }
        }
    }

    // 优先请求轻量级 GetCascadeTrajectoryGeneratorMetadata（不含庞大 steps），若不支持再回退全量 GetCascadeTrajectory。
    if let Ok(meta_resp) = apply_ls_headers(
        client.post(format!("{base}/GetCascadeTrajectoryGeneratorMetadata")),
        &ep.csrf,
    )
    .json(&json!({
        "cascadeId": conversation_id,
        "generatorMetadataOffset": 0,
        "includeMessages": false
    }))
    .send()
    .await
        && meta_resp.status().is_success()
        && let Ok(meta_json) = meta_resp.json::<Value>().await
        && (meta_json.get("generatorMetadata").is_some()
            || meta_json.as_object().is_some_and(|m| m.is_empty()))
    {
        return extract_trajectory_metrics(conversation_id, &meta_json, &placeholder_labels);
    }

    let Ok(traj_resp) =
        apply_ls_headers(client.post(format!("{base}/GetCascadeTrajectory")), &ep.csrf)
            .json(&json!({
                "cascadeId": conversation_id,
                "trajectoryVerbosity": 2
            }))
            .send()
            .await
    else {
        if let Ok(mut guard) = CACHED_LS_ENDPOINT.get_or_init(|| Mutex::new(None)).lock() {
            *guard = None;
        }
        return json!({ "conversationId": conversation_id, "state": "unavailable" });
    };

    if !traj_resp.status().is_success() {
        return json!({ "conversationId": conversation_id, "state": "unavailable" });
    }
    let Ok(traj_json) = traj_resp.json::<Value>().await else {
        return json!({ "conversationId": conversation_id, "state": "unavailable" });
    };

    extract_trajectory_metrics(conversation_id, &traj_json, &placeholder_labels)
}

fn parse_u64_json(val: Option<&Value>) -> Option<u64> {
    let v = val?;
    v.as_u64().or_else(|| v.as_str()?.trim().parse::<u64>().ok())
}

fn extract_trajectory_metrics(
    conversation_id: &str,
    traj_json: &Value,
    placeholder_labels: &BTreeMap<String, String>,
) -> Value {
    let items = traj_json
        .get("generatorMetadata")
        .or_else(|| traj_json.pointer("/trajectory/generatorMetadata"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let now_ms = chrono::Utc::now().timestamp_millis();
    if items.is_empty() {
        return json!({
            "conversationId": conversation_id,
            "state": "empty",
            "sampledAt": now_ms,
            "context": {
                "usedTokens": 0,
                "maxTokens": Value::Null,
                "usedFraction": 0.0,
                "model": "",
                "endpointModel": "",
            },
            "speed": Value::Null,
        });
    }

    // 预扫描同模型历史出现过的最大上下文窗口容量，避免新轮次缺省 maxContextTokens 时丢失分母。
    let mut max_by_model = BTreeMap::<String, u64>::new();
    for item in &items {
        let Some(chat) = item.get("chatModel") else { continue };
        let cwm = chat
            .pointer("/chatStartMetadata/contextWindowMetadata")
            .or_else(|| item.pointer("/chatStartMetadata/contextWindowMetadata"));
        let raw_model = chat
            .get("modelDisplayName")
            .or_else(|| chat.get("modelName"))
            .or_else(|| chat.get("model"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if !raw_model.is_empty()
            && let Some(max_toks) = cwm.and_then(|c| parse_u64_json(c.get("maxContextTokens")))
            && max_toks > 0
        {
            max_by_model.insert(raw_model.to_string(), max_toks);
        }
    }

    let mut context_val = Value::Null;
    let mut speed_val = Value::Null;

    for item in items.iter().rev() {
        let Some(chat) = item.get("chatModel") else { continue };
        if context_val.is_null() {
            // Antigravity 2.x 将 chatStartMetadata 挂在 chatModel 下；同时兼容旧版顶层挂法与 tokenBreakdown/usage 估算兜底。
            let cwm = chat
                .pointer("/chatStartMetadata/contextWindowMetadata")
                .or_else(|| item.pointer("/chatStartMetadata/contextWindowMetadata"));
            let usage_fallback_tokens = chat.get("usage").and_then(|u| {
                let input = parse_u64_json(u.get("inputTokens")).unwrap_or(0);
                let cached = parse_u64_json(u.get("cacheReadTokens")).unwrap_or(0);
                let total = input.saturating_add(cached);
                (total > 0).then_some(total)
            });

            if cwm.is_some() || usage_fallback_tokens.is_some() {
                let used = cwm
                    .and_then(|c| {
                        parse_u64_json(c.get("estimatedTokensUsed"))
                            .or_else(|| parse_u64_json(c.pointer("/tokenBreakdown/totalTokens")))
                    })
                    .or(usage_fallback_tokens)
                    .unwrap_or(0);
                let raw_model = chat
                    .get("modelDisplayName")
                    .or_else(|| chat.get("modelName"))
                    .or_else(|| chat.get("model"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let max_toks = cwm
                    .and_then(|c| parse_u64_json(c.get("maxContextTokens")))
                    .filter(|m| *m > 0)
                    .or_else(|| (!raw_model.is_empty()).then(|| max_by_model.get(raw_model).copied()).flatten());
                let frac = max_toks
                    .filter(|m| *m > 0)
                    .map(|m| (used as f64 / m as f64).clamp(0.0, 1.0));
                let model_label = placeholder_labels
                    .get(raw_model)
                    .cloned()
                    .unwrap_or_else(|| raw_model.to_string());
                let endpoint_model = chat
                    .get("responseModel")
                    .or_else(|| chat.pointer("/usage/responseModel"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.starts_with("MODEL_PLACEHOLDER_"))
                    .unwrap_or("");
                context_val = json!({
                    "usedTokens": used,
                    "maxTokens": max_toks,
                    "usedFraction": frac,
                    "model": model_label,
                    "endpointModel": endpoint_model,
                });
            }
        }

        if speed_val.is_null()
            && let Some(dur_secs) = parse_proto_duration_secs(chat.get("streamingDuration"))
            && dur_secs > 0.05
            && let Some(usage) = chat.get("usage")
        {
            let out_tokens = parse_u64_json(usage.get("responseOutputTokens"))
                .or_else(|| parse_u64_json(usage.get("outputTokens")))
                .unwrap_or(0);
            if out_tokens > 0 {
                let ttft = parse_proto_duration_secs(chat.get("timeToFirstToken"));
                speed_val = json!({
                    "tokensPerSecond": out_tokens as f64 / dur_secs,
                    "ttftSeconds": ttft,
                    "basis": "response",
                    "shortSample": dur_secs < 1.0,
                });
            }
        }

        if !context_val.is_null() && !speed_val.is_null() {
            break;
        }
    }

    json!({
        "conversationId": conversation_id,
        "state": "live",
        "sampledAt": now_ms,
        "context": context_val,
        "speed": speed_val,
    })
}

fn parse_proto_duration_secs(val: Option<&Value>) -> Option<f64> {
    let v = val?;
    if let Some(s) = v.as_str() {
        return s.trim().strip_suffix('s')?.parse::<f64>().ok();
    }
    if let Some(obj) = v.as_object() {
        let secs = obj
            .get("seconds")
            .and_then(|x| x.as_f64().or_else(|| x.as_str()?.parse().ok()))
            .unwrap_or(0.0);
        let nanos = obj
            .get("nanos")
            .and_then(|x| x.as_f64().or_else(|| x.as_str()?.parse().ok()))
            .unwrap_or(0.0);
        return Some(secs + nanos / 1_000_000_000.0);
    }
    None
}

async fn discover_local_language_server_endpoint() -> Option<LanguageServerEndpoint> {
    let procs = super::process::enumerate_antigravity_processes();
    let candidates: Vec<_> = procs
        .iter()
        .filter(|p| p.is_language_server)
        .map(|p| (p.pid, extract_csrf_from_cmdline(&p.command_line).unwrap_or_default()))
        .collect();
    if candidates.is_empty() {
        return None;
    }

    let cache_slot = CACHED_LS_ENDPOINT.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache_slot.lock()
        && let Some(cached) = guard.clone()
        && candidates
            .iter()
            .any(|(pid, csrf)| *pid == cached.pid && *csrf == cached.csrf)
    {
        return Some(cached);
    }

    for (pid, csrf) in candidates {
        let ports = discover_listening_ports_for_pid(pid);
        if ports.is_empty() {
            continue;
        }
        for (scheme, is_https) in [("http", false), ("https", true)] {
            let Some(client) = build_ls_http_client(is_https, Duration::from_millis(800)) else {
                continue;
            };
            for port in &ports {
                let base_url = format!(
                    "{scheme}://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService"
                );
                if let Ok(resp) =
                    apply_ls_headers(client.post(format!("{base_url}/GetUserStatus")), &csrf)
                        .json(&json!({}))
                        .send()
                        .await
                    && resp.status().is_success()
                {
                    let ep = LanguageServerEndpoint {
                        pid,
                        base_url,
                        is_https,
                        csrf: csrf.clone(),
                    };
                    if let Ok(mut guard) = cache_slot.lock() {
                        *guard = Some(ep.clone());
                    }
                    return Some(ep);
                }
            }
        }
    }
    None
}

fn discover_listening_ports_for_pid(pid: u32) -> Vec<u16> {
    #[cfg(unix)]
    {
        let Ok(out) = std::process::Command::new("/usr/sbin/lsof")
            .args([
                "-nP",
                "-a",
                "-p",
                &pid.to_string(),
                "-iTCP",
                "-sTCP:LISTEN",
                "-Fn",
            ])
            .output()
        else {
            return Vec::new();
        };
        let text = String::from_utf8_lossy(&out.stdout);
        text.lines()
            .filter_map(|line| line.strip_prefix('n')?.rsplit(':').next()?.parse::<u16>().ok())
            .collect()
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        let mut cmd = std::process::Command::new("netstat");
        cmd.args(["-ano", "-p", "tcp"]).creation_flags(0x0800_0000);
        let Ok(out) = cmd.output() else {
            return Vec::new();
        };
        parse_windows_netstat_listening_ports(&String::from_utf8_lossy(&out.stdout), pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        Vec::new()
    }
}

#[cfg(any(windows, test))]
fn parse_windows_netstat_listening_ports(text: &str, target_pid: u32) -> Vec<u16> {
    let mut ports = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 || !cols[0].eq_ignore_ascii_case("TCP") {
            continue;
        }
        if !cols[3].eq_ignore_ascii_case("LISTENING") {
            continue;
        }
        let Ok(pid) = cols[4].parse::<u32>() else {
            continue;
        };
        if pid != target_pid {
            continue;
        }
        if let Some(port_str) = cols[1].rsplit(':').next()
            && let Ok(port) = port_str.parse::<u16>()
            && port > 0
            && !ports.contains(&port)
        {
            ports.push(port);
        }
    }
    ports
}

fn extract_csrf_from_cmdline(cmd: &str) -> Option<String> {
    let marker = "--csrf_token";
    let idx = cmd.find(marker)?;
    let rest = cmd[idx + marker.len()..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim_start();
    let token: String = rest
        .trim_start_matches('"')
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '"')
        .collect();
    (!token.is_empty()).then_some(token)
}

/// 启动常驻后台监督任务：当开启了汉化或上下文圆环时，自动发现运行中 Antigravity 的 `DevToolsActivePort` 并保持实时同步。
pub fn ensure_background_enhance_supervisor(handle: tokio::runtime::Handle) {
    let slot = ENHANCE_SUPERVISOR_RUNNING.get_or_init(|| Mutex::new(false));
    if let Ok(mut guard) = slot.lock() {
        if *guard {
            return;
        }
        *guard = true;
    }

    handle.spawn(async move {
        let mut first_tick = true;
        loop {
            tokio::time::sleep(Duration::from_millis(if first_tick { 800 } else { 3500 })).await;
            first_tick = false;
            let state = super::load_antigravity_state();
            if !state.enable_ui_localization && !state.enable_context_ring {
                continue;
            }
            let port = if state.cdp_port == 0 {
                super::process::DEFAULT_ANTIGRAVITY_CDP_PORT
            } else {
                state.cdp_port
            };
            if !super::process::discover_active_cdp_ports(port).is_empty() {
                let _ = apply_cdp_enhancements_once(
                    port,
                    state.enable_ui_localization,
                    state.enable_context_ring,
                )
                .await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_url_filter_skips_splash_and_external_pages() {
        assert!(!is_antigravity_renderer_url("data:text/html,<h1>splash</h1>"));
        assert!(!is_antigravity_renderer_url("blob:https://127.0.0.1:50602/abc"));
        assert!(!is_antigravity_renderer_url("about:srcdoc"));
        assert!(!is_antigravity_renderer_url("https://example.com/c/123"));
        assert!(is_antigravity_renderer_url(
            "https://127.0.0.1:50602/c/0f76994e-012a-4a47-9334-91eeecdb0da2"
        ));
        assert!(is_antigravity_renderer_url(
            "vscode-file://vscode-app/Applications/Antigravity%20IDE.app/workbench.html"
        ));
    }

    #[test]
    fn extracts_context_window_from_chat_model_nested_start_metadata() {
        let traj = json!({
            "trajectory": {
                "generatorMetadata": [{
                    "chatModel": {
                        "model": "MODEL_PLACEHOLDER_M526",
                        "modelDisplayName": "Gemini 4 Hydrogen (Medium)",
                        "responseModel": "gemini-4-hydrogen",
                        "streamingDuration": "2.0s",
                        "timeToFirstToken": "0.5s",
                        "usage": {
                            "inputTokens": "2600",
                            "cacheReadTokens": "110000",
                            "responseOutputTokens": "400"
                        },
                        "chatStartMetadata": {
                            "contextWindowMetadata": {
                                "estimatedTokensUsed": 118240,
                                "maxContextTokens": 512000
                            }
                        }
                    }
                }]
            }
        });
        let out = extract_trajectory_metrics("conv-1", &traj, &BTreeMap::new());
        assert_eq!(out["context"]["usedTokens"], 118240);
        assert_eq!(out["context"]["maxTokens"], 512000);
        assert_eq!(out["context"]["model"], "Gemini 4 Hydrogen (Medium)");
        assert_eq!(out["context"]["endpointModel"], "gemini-4-hydrogen");
        assert_eq!(out["speed"]["tokensPerSecond"], 200.0);
        assert_eq!(out["speed"]["ttftSeconds"], 0.5);
    }

    #[test]
    fn extracts_top_level_generator_metadata_with_total_tokens_and_max_backfill() {
        let resp = json!({
            "generatorMetadata": [
                {
                    "chatModel": {
                        "modelDisplayName": "Claude Opus 5.5 (High)",
                        "chatStartMetadata": {
                            "contextWindowMetadata": {
                                "estimatedTokensUsed": 120000,
                                "maxContextTokens": 840000
                            }
                        }
                    }
                },
                {
                    "chatModel": {
                        "modelDisplayName": "Claude Opus 5.5 (High)",
                        "responseModel": "claude-opus-5-5-high",
                        "streamingDuration": "2.0s",
                        "timeToFirstToken": "0.4s",
                        "usage": {
                            "responseOutputTokens": "300"
                        },
                        "chatStartMetadata": {
                            "contextWindowMetadata": {
                                "tokenBreakdown": { "totalTokens": 652678 }
                            }
                        }
                    }
                }
            ]
        });
        let out = extract_trajectory_metrics("conv-2", &resp, &BTreeMap::new());
        assert_eq!(out["state"], "live");
        assert_eq!(out["context"]["usedTokens"], 652678);
        assert_eq!(out["context"]["maxTokens"], 840000);
        assert_eq!(out["context"]["endpointModel"], "claude-opus-5-5-high");
        assert_eq!(out["speed"]["tokensPerSecond"], 150.0);
    }

    #[test]
    fn empty_generator_metadata_returns_empty_state() {
        let out = extract_trajectory_metrics("conv-empty", &json!({}), &BTreeMap::new());
        assert_eq!(out["state"], "empty");
        assert_eq!(out["context"]["usedTokens"], 0);
        assert_eq!(out["context"]["usedFraction"], 0.0);
    }

    #[test]
    fn renderer_script_contains_direct_rpc_and_v2_widget_markers() {
        let script = build_renderer_injection_script(false, true, false);
        assert!(script.contains("const WIDGET_VERSION = 2;"));
        assert!(script.contains("GetCascadeTrajectoryGeneratorMetadata"));
        assert!(script.contains("agy_session_csrf_token"));
    }

    #[test]
    fn parses_windows_netstat_listening_ports_for_pid() {
        let sample = "\
  Proto  Local Address          Foreign Address        State           PID
  TCP    127.0.0.1:50602        0.0.0.0:0              LISTENING       76326
  TCP    127.0.0.1:50603        0.0.0.0:0              LISTENING       76326
  TCP    127.0.0.1:50603        127.0.0.1:59123        ESTABLISHED     76326
  TCP    [::1]:50628            [::]:0                 LISTENING       76326
  TCP    127.0.0.1:9999         0.0.0.0:0              LISTENING       1234
";
        assert_eq!(
            parse_windows_netstat_listening_ports(sample, 76326),
            vec![50602, 50603, 50628]
        );
    }
}
