  function installStyle() {
    if (!installStyle.sheet) {
      const text = `
      .${actionGroupClass} {
        position: absolute;
        right: var(--codex-session-actions-right, 28px);
        top: 50%;
        transform: translateY(-50%);
        z-index: 20;
        opacity: 0;
        pointer-events: none;
        display: inline-flex;
        align-items: center;
        gap: 2px;
        background: transparent;
      }
      .${actionButtonClass} {
        width: 20px;
        height: 20px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: var(--codex-session-action-color, var(--color-token-text-tertiary, var(--swivox-text-tertiary, rgba(255,255,255,.5))));
        font: 14px/1 system-ui, sans-serif;
        padding: 0;
        cursor: default;
        text-align: center;
      }
      .${actionButtonClass} svg {
        display: block;
        width: 16px;
        height: 16px;
      }
      .${actionButtonClass}:hover,
      .${actionButtonClass}:focus-visible {
        background: var(--codex-session-action-hover-background, transparent);
        color: var(--codex-session-action-hover-color, var(--codex-session-action-color, var(--color-token-text-primary, #f4f4f5)));
        outline: none;
      }
      .${moreMenuClass} {
        position: fixed;
        z-index: 2147483201;
        min-width: 104px;
        border: 1px solid var(--swivox-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--swivox-bg-elevated);
        color: var(--swivox-text);
        box-shadow: var(--ui-menu-shadow, var(--shadow-300, 0 8px 24px rgba(0,0,0,.16)));
        padding: 4px;
      }
      .${moreMenuClass}[hidden] { display: none !important; }
      .${moreMenuClass}.codex-session-more-menu-open-up {
        transform: translateY(calc(-100% - 34px));
      }
      .codex-session-more-menu-item {
        width: 100%;
        border: 0;
        border-radius: var(--border-radius-sm, 6px);
        background: transparent;
        color: inherit;
        cursor: default;
        display: flex;
        align-items: center;
        gap: 8px;
        font: inherit;
        font-size: 13px;
        line-height: 18px;
        padding: 6px 8px;
        text-align: left;
      }
      .codex-session-more-menu-item:hover,
      .codex-session-more-menu-item:focus-visible {
        background: var(--swivox-bg-hover);
        outline: none;
      }
      .codex-session-more-menu-icon {
        width: 16px;
        text-align: center;
      }
      .${threadIdBadgeClass} {
        flex: 0 0 auto;
        display: inline-flex;
        align-items: center;
        max-width: 152px;
        margin-right: 8px;
        color: var(--text-secondary, var(--color-token-text-secondary, var(--swivox-text-secondary, rgba(142,142,160,.95))));
        font: 11px/1.1 ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
        letter-spacing: .01em;
        opacity: .9;
        white-space: nowrap;
        user-select: text;
      }
      ${selectors.sidebarThread} [data-codex-thread-id-badge-wrap="true"] {
        display: inline-flex;
        align-items: center;
        min-width: 0;
        max-width: 100%;
      }
      ${selectors.sidebarThread} [data-codex-thread-id-badge-wrap="true"] ${selectors.threadTitle},
      ${selectors.sidebarThread} [data-codex-thread-id-badge-wrap="true"] .truncate.select-none,
      ${selectors.sidebarThread} [data-codex-thread-id-badge-wrap="true"] .truncate.text-base {
        min-width: 0;
      }
      .codex-archive-row-button {
        border: 1px solid var(--color-token-border-light, var(--color-token-border-default, rgba(0,0,0,.12)));
        border-radius: var(--border-radius-sm, 6px);
        background: var(--color-token-bg-secondary, transparent);
        color: var(--color-token-text-secondary, inherit);
        font: inherit;
        font-size: 13px;
        line-height: 16px;
        padding: 3px 8px;
        cursor: pointer;
      }
      .codex-archive-row-button.${buttonClass} {
        border-color: var(--color-border-danger, #dc2626);
        background: var(--color-background-danger-soft, rgba(220,38,38,.1));
        color: var(--color-text-danger, #dc2626);
      }
      .codex-archive-row-button.${exportButtonClass} {
        border-color: var(--color-token-border-light, var(--color-token-border-default, rgba(0,0,0,.12)));
        background: var(--color-token-bg-secondary, transparent);
        color: var(--color-token-text-primary, inherit);
      }
      .${sessionCopyMenuItemClass} {
        cursor: pointer;
      }
      .${sessionShareButtonClass} {
        position: static;
        flex: 0 0 auto;
        pointer-events: auto;
        -webkit-app-region: no-drag;
        margin-left: 2px;
        z-index: 2147483001;
        min-height: var(--height-button-composer, 32px);
        border-radius: var(--border-radius-lg, 8px);
        font: inherit;
        font-size: 13px;
        line-height: 18px;
        cursor: pointer;
        box-shadow: none;
      }
      .${sessionShareButtonClass}:hover,
      .${sessionShareButtonClass}:focus-visible {
        background: var(--color-token-list-hover-background, var(--swivox-bg-hover));
        color: var(--color-token-text-primary, var(--swivox-text));
        outline: none;
      }
      .${sessionShareButtonClass}[aria-busy="true"] {
        cursor: wait;
        opacity: .65;
      }
      [data-codex-delete-row="true"]:hover .${actionGroupClass} {
        opacity: 1;
        pointer-events: auto;
      }
      [data-codex-delete-row="true"]:hover ${selectors.threadTitle},
      [data-codex-delete-row="true"]:focus-within ${selectors.threadTitle},
      [data-codex-delete-row="true"].codex-session-more-open ${selectors.threadTitle} {
        flex: 0 1 auto;
        width: var(--codex-session-title-max-width, auto);
        max-width: var(--codex-session-title-max-width, 100%);
        overflow: hidden;
      }
      [data-codex-delete-row="true"].codex-session-more-open .${actionGroupClass} {
        opacity: 1;
        pointer-events: auto;
        z-index: 2147483201;
      }
      [data-codex-delete-row="true"].codex-archive-confirm-visible .${actionGroupClass} {
        right: max(66px, var(--codex-session-actions-right, 28px));
      }
      .${actionTooltipClass} {
        position: fixed;
        z-index: 2147483201;
        max-width: min(220px, calc(100vw - 32px));
        border: 1px solid var(--swivox-border);
        border-radius: var(--border-radius-md, 6px);
        background: var(--swivox-bg-elevated);
        color: var(--swivox-text);
        font: inherit;
        font-size: 13px;
        line-height: 16px;
        padding: 6px 8px;
        box-shadow: var(--tooltip-box-shadow, var(--shadow-200, 0 4px 12px rgba(0,0,0,.14)));
        pointer-events: none;
        white-space: nowrap;
      }
      .codex-archive-delete-all {
        border: 1px solid var(--color-border-danger, #dc2626);
        border-radius: var(--border-radius-sm, 6px);
        background: var(--color-background-danger-soft, rgba(220,38,38,.1));
        color: var(--color-text-danger, #dc2626);
        font: inherit;
        font-size: 13px;
        line-height: 16px;
        padding: 3px 8px;
        cursor: pointer;
      }
      .codex-archive-action-bar {
        position: fixed;
        right: 28px;
        top: 86px;
        z-index: 2147482999;
        box-shadow: 0 8px 24px rgba(0,0,0,.18);
      }
      .codex-delete-toast {
        position: fixed;
        right: 18px;
        bottom: 18px;
        z-index: 2147483000;
        padding: 10px 12px;
        border: 1px solid var(--swivox-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--swivox-bg-elevated);
        color: var(--swivox-text);
        font: inherit;
        font-size: 13px;
        box-shadow: var(--ui-menu-shadow, var(--shadow-300, 0 8px 24px rgba(0,0,0,.16)));
        pointer-events: none;
      }
      .codex-delete-toast button { margin-left: 10px; pointer-events: auto; }
      /* 拓展与内置提示共用的类型配色。不传 type 时保持上面的默认外观。 */
      .codex-delete-toast[data-toast-type="success"] { border-color: var(--swivox-success, #2f9e63); }
      .codex-delete-toast[data-toast-type="warn"] { border-color: var(--swivox-warn, #b7791f); }
      .codex-delete-toast[data-toast-type="error"] { border-color: var(--swivox-error, #c53030); }
      .codex-delete-confirm-overlay {
        position: fixed;
        inset: 0;
        z-index: 2147483200;
        display: flex;
        align-items: center;
        justify-content: center;
        background: var(--modal-backdrop-dim-shadow, rgba(0,0,0,.32));
        backdrop-filter: blur(1px);
      }
      .codex-delete-confirm-content {
        width: min(420px, calc(100vw - 48px));
        border: 1px solid var(--swivox-border);
        border-radius: var(--border-radius-xl, 12px);
        background: var(--swivox-bg-elevated);
        color: var(--swivox-text);
        font: inherit;
        font-size: 14px;
        box-shadow: var(--shadow-400, 0 16px 48px rgba(0,0,0,.2));
        padding: 20px;
      }
      .codex-delete-confirm-title { font-size: 16px; font-weight: 650; }
      .codex-delete-confirm-message { margin-top: 8px; color: var(--swivox-text-secondary); line-height: 1.45; }
      .codex-delete-confirm-actions {
        display: flex;
        justify-content: flex-end;
        gap: 10px;
        margin-top: 18px;
      }
      .codex-delete-confirm-actions button {
        min-height: 32px;
        border: 1px solid var(--swivox-border);
        border-radius: var(--border-radius-lg, 8px);
        padding: 5px 12px;
        background: var(--swivox-bg-secondary);
        color: var(--swivox-text);
        font: inherit;
        font-size: 13px;
        cursor: pointer;
      }
      .codex-delete-confirm-actions button:hover,
      .codex-delete-confirm-actions button:focus-visible {
        background: var(--swivox-bg-hover);
        outline: none;
      }
      .codex-delete-confirm-actions [data-codex-delete-confirm="true"] {
        border-color: var(--color-border-danger, #dc2626);
        background: var(--color-background-danger-solid, #dc2626);
        color: var(--color-text-danger-solid, #fff);
      }
      .swivox-modal-overlay {
        position: fixed;
        top: 0;
        left: 0;
        /*
         * overlay 自身带 zoom（见 applySwivoxZoom），而它的 inset: 0 与 100vw
         * 都按未缩放的视口算，再乘 zoom 就溢出（实测 zoom=1.2 时 100vw 得到 2072px，
         * 视口只有 1727px）。用 calc(100vw / var(--swivox-zoom)) 抵消；zoom 缺失
         * 时分母回落到 1，行为与改造前一致。
         * 内部子元素用百分比即可——它们在缩放空间里，百分比本来就对。
         * 注意：本段在 JS 模板字符串里，注释中不能出现反引号，否则会提前闭合。
         */
        width: calc(100vw / var(--swivox-zoom, 1));
        height: calc(100vh / var(--swivox-zoom, 1));
        z-index: 2147483646;
        display: flex;
        align-items: center;
        justify-content: center;
        background: var(--modal-backdrop-dim-shadow, rgba(0,0,0,.32));
        backdrop-filter: blur(1px);
        pointer-events: auto;
        -webkit-app-region: no-drag;
      }
      .swivox-modal-content {
        width: min(520px, calc(100% - 48px));
        max-height: min(680px, calc(100% - 40px));
        display: flex;
        flex-direction: column;
        overflow: hidden;
        border: 1px solid var(--swivox-border);
        border-radius: var(--border-radius-xl, 12px);
        background: var(--swivox-bg-primary);
        color: var(--swivox-text);
        font: inherit;
        font-size: 14px;
        box-shadow: var(--shadow-400, 0 16px 48px rgba(0,0,0,.2));
        pointer-events: auto;
        -webkit-app-region: no-drag;
      }
      .swivox-modal-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 16px 20px 8px;
        flex: 0 0 auto;
        -webkit-app-region: no-drag;
      }
      .swivox-modal-title { display: flex; align-items: center; gap: 8px; font-size: 18px; font-weight: 600; }
      .swivox-backend-indicator { width: 8px; height: 8px; border-radius: 999px; background: var(--swivox-text-tertiary); display: inline-block; }
      .swivox-backend-indicator[data-status="ok"] { background: var(--swivox-success); }
      .swivox-backend-indicator[data-status="failed"] { background: var(--swivox-danger); }
      .swivox-backend-indicator[data-status="checking"] { background: var(--swivox-warning); }
      .swivox-backend-indicator[data-status="degraded"] { background: var(--swivox-warning); }
      #${swivoxSidebarNavId} {
        position: relative;
        flex: 0 0 auto;
      }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-icon {
        width: 20px;
        height: 20px;
        flex: 0 0 20px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
      }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-icon svg {
        width: 19px;
        height: 19px;
        display: block;
      }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-status {
        width: 7px;
        height: 7px;
        margin-left: auto;
        border-radius: 999px;
        background: #a1a1aa;
        opacity: .9;
      }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-status[data-status="ok"] {
        background: #34d399;
        box-shadow: 0 0 7px rgba(52,211,153,.7);
      }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-status[data-status="failed"] { background: #ef4444; }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-status[data-status="checking"] { background: #fbbf24; }
      #${swivoxSidebarNavId} button[data-active="true"] {
        background: var(--swivox-bg-selected);
        color: var(--swivox-text);
      }
      .swivox-plugin-sidebar { position: relative; flex: 0 0 auto; }
      .swivox-plugin-sidebar .swivox-sidebar-nav-icon { width: 20px; height: 20px; flex: 0 0 20px; display: inline-flex; align-items: center; justify-content: center; }
      .swivox-plugin-sidebar .swivox-sidebar-nav-icon svg { width: 19px; height: 19px; }
      .swivox-plugin-sidebar-label { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; min-width: 0; }
      .swivox-plugin-sidebar button[data-active="true"] { background: var(--swivox-bg-selected); color: var(--swivox-text); }
      .swivox-plugin-market-panel { max-width: 1140px; width: 100%; margin: 0 auto; box-sizing: border-box; }
      .swivox-plugin-intro { margin: 0 0 18px; color: var(--swivox-text-secondary); font-size: 13px; line-height: 1.6; }
      .swivox-plugin-toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; margin-bottom: 14px; }
      .swivox-plugin-sources { display: flex; gap: 4px; flex: 1 1 auto; }
      .swivox-plugin-sources button { border: 1px solid var(--swivox-border); border-radius: 7px; padding: 7px 12px; color: var(--swivox-text-secondary); background: transparent; cursor: pointer; }
      .swivox-plugin-sources button[data-active="true"] { background: var(--swivox-bg-selected); color: var(--swivox-text); }
      .swivox-plugin-search { flex: 1 1 240px; min-width: 120px; border: 1px solid var(--swivox-border); border-radius: 8px; background: var(--swivox-bg-secondary); color: var(--swivox-text); padding: 9px 12px; font-size: 14px; }
      .swivox-plugin-filter { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--swivox-text-secondary); }
      .swivox-plugin-status { font-size: 12px; color: var(--swivox-text-secondary); white-space: pre-wrap; margin-bottom: 14px; }
      .swivox-plugin-results { display: flex; flex-direction: column; gap: 10px; }
      .swivox-plugin-card { display: flex; align-items: flex-start; gap: 18px; padding: 16px; border: 1px solid var(--swivox-border); border-radius: 10px; background: var(--swivox-bg-secondary); }
      .swivox-plugin-card-info { flex: 1 1 auto; min-width: 0; }
      .swivox-plugin-name { font-size: 15px; font-weight: 600; overflow-wrap: anywhere; }
      .swivox-plugin-id, .swivox-plugin-meta { color: var(--swivox-text-tertiary); font-size: 11px; margin-top: 4px; overflow-wrap: anywhere; }
      .swivox-plugin-description { color: var(--swivox-text-secondary); font-size: 13px; line-height: 1.55; margin: 7px 0; display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; }
      .swivox-plugin-tags { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 8px; }
      .swivox-plugin-tags span { border-radius: 5px; padding: 2px 6px; background: var(--swivox-bg-selected); color: var(--swivox-text-secondary); font-size: 10px; overflow-wrap: anywhere; }
      .swivox-plugin-install { flex: 0 0 auto; white-space: nowrap; }
      .swivox-plugin-market-page button:disabled { opacity: .55; cursor: default; }
      .swivox-plugin-job-message { font-size: 12px; color: var(--swivox-text-secondary); margin-top: 8px; white-space: pre-wrap; overflow-wrap: anywhere; }
      .swivox-plugin-job-message[data-status="failed"] { color: var(--swivox-danger); }
      .swivox-plugin-pagination { display: flex; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: 10px; margin-top: 20px; font-size: 12px; color: var(--swivox-text-secondary); }
      .swivox-plugin-pagination > span { margin-right: auto; }
      .swivox-plugin-empty { padding: 40px 16px; text-align: center; color: var(--swivox-text-secondary); }
      @media (max-width: 600px) { .swivox-plugin-card { flex-direction: column; gap: 10px; } .swivox-plugin-install { align-self: flex-end; } }
      /*
       * 新版导航图标栏里的 Swivox / 拓展 / 推荐内容入口：原生按钮只放图标，
       * 这里对齐它的尺寸。
       *
       * 用 [data-swivox-rail] 而不是逐个列 id——早先按 id 写，加第三个入口时
       * 漏掉了对应的选择器，那个图标容器就没有 20px 约束、撑成整个按钮宽，
       * 表现为图标偏左不居中。
       */
      [data-swivox-rail] {
        position: relative;
        flex: 0 0 auto;
        display: flex;
        align-items: center;
        justify-content: center;
      }
      [data-swivox-rail] > button {
        position: relative;
      }
      [data-swivox-rail] .swivox-rail-icon {
        width: 20px;
        height: 20px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
      }
      [data-swivox-rail] .swivox-rail-icon svg {
        width: 19px;
        height: 19px;
        display: block;
      }
      /* 图标栏是纯图标，状态点挂在按钮右上角，不占布局。 */
      #${swivoxRailNavId} .swivox-sidebar-nav-status {
        position: absolute;
        top: 2px;
        right: 2px;
        margin-left: 0;
        width: 6px;
        height: 6px;
      }
      /*
       * 我们的页面是叠加在 Codex 之上的，Codex 并不知道，所以它自己那个
       * destination 的选中态会一直留着，看起来像 rail 上同时亮两个。
       * 页面打开时给根节点打标记，用 CSS 把原生选中项压成未选中；
       * 关掉页面即移除标记，原生状态自动恢复——比改它的按钮属性稳，
       * 不会和 React 的重渲染打架。
       * 颜色取自当前主题下真实未选中项（见 syncSwivoxRailNativeSelection），
       * 主题切换时下次同步会重算。
       */
      html[data-swivox-page-open] nav[data-app-navigation-rail] [data-sidebar-destination][aria-current="page"] {
        color: var(--swivox-rail-dim, var(--swivox-text-tertiary, currentColor)) !important;
      }
      html[data-swivox-page-open] nav[data-app-navigation-rail] [data-sidebar-destination][aria-current="page"]::before {
        opacity: 0 !important;
      }
      html[data-swivox-page-open] nav[data-app-navigation-rail] [data-sidebar-destination][aria-current="page"] * {
        color: var(--swivox-rail-dim, var(--swivox-text-tertiary, currentColor)) !important;
      }
      /*
       * 页面 overlay 的 left 由 positionSwivoxPage 按图标栏右边界算好写进来。
       *
       * 关键：写进来的必须是**布局坐标**（视觉值 / zoom），因为 overlay 自己在缩放
       * 空间里布局，宽度也要用同一个空间的量。width 的 calc(100vw / zoom - left - right)
       * 把右边贴到原生内容区的右边缘；三个量都除过 zoom，缩放后才正好补齐。
       * 注意：本段在 JS 模板字符串里，注释里不能出现反引号，否则会提前闭合。
       *
       * 右/下各留一圈槽（原生是 4px）并给四角加圆角，对齐官方面板的观感：
       * 官方页面面板 [_PageSurface_] 的 rect 与我们这块 overlay 完全相同（都是
       * [52, 44, 1663, 940]），四角 12px；它外面套了一个 padding: 0 4px 4px 0 的槽，
       * 右、下能看到底色。我们原先 right/bottom 贴 0 且无圆角，于是方角并且比官方
       * 多占 4px，用户反馈「少一个圆角」就是这个。
       * 圆角值由 positionSwivoxPage 量官方面板写入，四个角一起用，不做左右区分。
       * overflow: hidden 让圆角真正裁到内容；内容都在 .swivox-modal-content 内，
       * 没有依赖溢出显示的全屏浮层，切掉是安全的。
       */
      .${swivoxPageClass} {
        position: fixed;
        top: var(--swivox-page-top, 0px);
        right: var(--swivox-page-right, 0px);
        bottom: var(--swivox-page-bottom, 0px);
        left: var(--swivox-page-left, 0px);
        width: calc(100vw / var(--swivox-zoom, 1) - var(--swivox-page-left, 0px) - var(--swivox-page-right, 0px));
        height: calc(100vh / var(--swivox-zoom, 1) - var(--swivox-page-top, 0px) - var(--swivox-page-bottom, 0px));
        border-radius: var(--swivox-page-radius, 0px);
        overflow: hidden;
        z-index: 2147483644;
        display: block;
        background: var(--swivox-bg-primary, #fff);
        pointer-events: auto;
        -webkit-app-region: no-drag;
      }
      .${swivoxPageClass} .swivox-modal-content {
        width: auto;
        height: 100%;
        max-height: none;
        border: 0;
        border-radius: 0;
        background: var(--swivox-bg-primary, #fff);
        box-shadow: none;
      }
      .${swivoxPageClass} .swivox-modal-header {
        width: 100%;
        margin: 0;
        padding: 16px 24px 10px;
      }
      .${swivoxPageClass} .swivox-modal-body {
        width: 100%;
        margin: 0;
        padding: 4px 32px 32px;
      }
      /* 两栏：左侧自己的面板（导航 / 列表），右侧内容区。观感对齐 Codex 原生页面。 */
      .${swivoxPageClass} .swivox-page-layout {
        display: flex;
        flex: 1 1 auto;
        min-height: 0;
        width: 100%;
      }
      .${swivoxPageClass} .swivox-page-nav {
        width: 260px;
        flex: 0 0 260px;
        display: flex;
        flex-direction: column;
        min-height: 0;
        border-right: 1px solid var(--swivox-border-subtle);
      }
      .${swivoxPageClass} .swivox-page-nav-header {
        flex: 0 0 auto;
        padding: 4px 16px 10px;
      }
      .${swivoxPageClass} .swivox-page-nav-title {
        font-size: 15px;
        font-weight: 600;
        color: var(--swivox-text);
      }
      .${swivoxPageClass} .swivox-page-nav-body {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        overscroll-behavior: contain;
        padding: 4px 10px 16px;
        scrollbar-width: thin;
        scrollbar-color: rgba(255,255,255,.28) transparent;
        /*
         * 左面板是导航/列表，不是内容：拖动时不该把条目文字或分组标题选蓝
         * （列表项本来就是整行可点，选中态由 data-active 表达）。
         * 右侧详情区不设，那里的描述文字要能复制。
         */
        user-select: none;
        -webkit-user-select: none;
      }
      .${swivoxPageClass} .swivox-page-main {
        flex: 1 1 auto;
        min-width: 0;
        display: flex;
        flex-direction: column;
        min-height: 0;
      }
      .swivox-page-nav-item {
        display: flex;
        align-items: center;
        gap: 8px;
        width: 100%;
        padding: 7px 10px;
        border: 0;
        border-radius: 8px;
        background: transparent;
        color: var(--swivox-text-secondary);
        font: inherit;
        font-size: 13px;
        text-align: left;
        cursor: pointer;
      }
      .swivox-page-nav-item:hover { background: var(--swivox-bg-hover); }
      .swivox-page-nav-item[data-active="true"] {
        background: var(--swivox-bg-selected);
        color: var(--swivox-text);
      }
      .swivox-page-nav-item-text { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
      .swivox-page-nav-item-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .swivox-page-nav-item-meta { font-size: 13px; color: var(--swivox-text-tertiary); }
      .swivox-page-nav-item-state {
        flex: 0 0 auto;
        width: 7px;
        height: 7px;
        border-radius: 999px;
        background: var(--swivox-text-tertiary);
      }
      .swivox-page-nav-item-state[data-state="on"] { background: #34d399; }
      /* 拓展条目：三行结构（名称 / 简介 / 作者+操作），对齐 VSCode 扩展列表的 .extension-list-item */
      .swivox-page-nav-item .swivox-extensions-item-body {
        flex: 1 1 auto;
        min-width: 0;
        display: flex;
        flex-direction: column;
        /* VSCode 的 .details 垂直居中，条目高矮不一时文字块不贴顶 */
        justify-content: center;
        overflow: hidden;
      }
      .swivox-extensions-item-header {
        display: flex;
        align-items: center;
        gap: 8px;
        min-width: 0;
      }
      .swivox-extensions-icon {
        flex: 0 0 auto;
        display: flex;
        align-items: flex-start;
        padding-top: 10px;
        color: var(--swivox-text-secondary);
      }
      .swivox-extensions-icon svg { width: 40px; height: 40px; display: block; }
      /* 市场清单给的图标：正方形等比缩放，圆角与 VSCode 的扩展图标一致 */
      .swivox-extensions-icon .swivox-extensions-icon-img {
        width: 40px;
        height: 40px;
        display: block;
        object-fit: contain;
        border-radius: 6px;
      }
      .swivox-page-nav-item[data-active="true"] .swivox-extensions-icon { color: var(--swivox-text); }
      /* 名称：VSCode 用 semiBold，且 hover 才加下划线 */
      .swivox-extensions-item-name {
        flex: 1 1 auto;
        min-width: 0;
        font-weight: 600;
        color: var(--swivox-text);
        white-space: nowrap;
        text-overflow: ellipsis;
        overflow: hidden;
      }
      .swivox-page-nav-item:hover .swivox-extensions-item-name { text-decoration: underline; }
      .swivox-extensions-item-description {
        margin-top: 2px;
        padding-right: 8px;
        color: var(--swivox-text-tertiary);
        font-size: 13px;
        line-height: normal;
        display: -webkit-box;
        -webkit-line-clamp: 2;
        -webkit-box-orient: vertical;
        overflow: hidden;
      }
      /* 底部行：作者在左、操作在右，VSCode 的 .footer 是 24px 高 */
      .swivox-extensions-item-footer {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: 6px;
        min-height: 24px;
        padding-top: 2px;
      }
      .swivox-extensions-item-publisher {
        flex: 1 1 auto;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: 13px;
        font-weight: 600;
        color: var(--swivox-text-tertiary);
      }
      .swivox-extensions-item-actions { flex: 0 0 auto; display: inline-flex; align-items: center; gap: 6px; }
      /* 行内按钮：默认低调，hover 整行时才提亮，避免列表花掉 */
      .swivox-extensions-item-button {
        padding: 2px 8px;
        border: 1px solid var(--swivox-border);
        border-radius: 6px;
        background: transparent;
        color: var(--swivox-text-secondary);
        font-size: 13px;
        line-height: 16px;
        white-space: nowrap;
      }
      .swivox-page-nav-item:hover .swivox-extensions-item-button {
        background: var(--swivox-bg-selected);
        color: var(--swivox-text);
      }
      /* 启用状态点挪到名称右侧，用 VSCode 那种 14px 徽标尺寸 */
      .swivox-page-nav-item[data-active="true"] .swivox-extensions-item-state {
        background: currentColor;
      }
      .swivox-extensions-item-state {
        flex: 0 0 auto;
        width: 7px;
        height: 7px;
        border-radius: 999px;
        background: var(--swivox-text-tertiary);
      }
      .swivox-extensions-item-state[data-state="on"] { background: #34d399; }
      .swivox-extensions-icon-badge {
        flex: 0 0 auto;
        width: 14px;
        height: 14px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border-radius: 999px;
        background: #10a37f;
        color: #fff;
        font-size: 13px;
        line-height: 1;
      }
      .swivox-page-nav-empty { padding: 8px 10px; color: var(--swivox-text-tertiary); font-size: 13px; }
      /* 拓展页：搜索框 + 分组标题 + 市场条目的「安装」按钮 */
      .swivox-page-search { padding: 0 10px 8px; }
      .swivox-page-search-input {
        width: 100%;
        box-sizing: border-box;
        padding: 6px 9px;
        border: 1px solid var(--swivox-border);
        border-radius: 8px;
        background: var(--swivox-bg-secondary);
        color: var(--swivox-text);
        font: inherit;
        font-size: 13px;
        outline: none;
      }
      .swivox-page-search-input:focus { border-color: var(--swivox-border-subtle); background: var(--swivox-bg-elevated); }
      .swivox-page-search-input::placeholder { color: var(--swivox-text-tertiary); }
      /* 左面板条目图标：市场里没有图标字段，统一用 VSCode 的默认扩展字形 */
      .swivox-page-nav-item-icon {
        flex: 0 0 auto;
        width: 22px;
        height: 22px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border-radius: 5px;
        background: var(--swivox-bg-hover);
        color: var(--swivox-text-secondary);
      }
      .swivox-page-nav-item-icon svg { width: 14px; height: 14px; display: block; }
      .swivox-page-nav-item[data-active="true"] .swivox-page-nav-item-icon {
        color: var(--swivox-text);
      }
      /* 右上角详情：形态对齐 VSCode 的扩展详情页 */
      .swivox-extensions-detail { padding: 4px 4px 24px; }
      .swivox-extensions-detail-empty {
        padding: 40px 8px;
        color: var(--swivox-text-tertiary);
        font-size: 13px;
        text-align: center;
      }
      .swivox-extensions-detail-head {
        display: flex;
        align-items: flex-start;
        gap: 14px;
        padding-bottom: 14px;
        border-bottom: 1px solid var(--swivox-border-subtle);
      }
      .swivox-extensions-detail-icon {
        flex: 0 0 auto;
        width: 64px;
        height: 64px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border-radius: 12px;
        background: var(--swivox-bg-hover);
        color: var(--swivox-text-secondary);
      }
      .swivox-extensions-detail-icon svg { width: 40px; height: 40px; display: block; }
      .swivox-extensions-detail-heading { flex: 1 1 auto; min-width: 0; }
      .swivox-extensions-detail-title {
        font-size: 19px;
        font-weight: 600;
        color: var(--swivox-text);
        line-height: 1.3;
      }
      .swivox-extensions-detail-meta {
        margin-top: 3px;
        color: var(--swivox-text-secondary);
        font-size: 13px;
      }
      .swivox-extensions-detail-sep { margin: 0 6px; color: var(--swivox-text-tertiary); }
      /* issue #2359：原 #fbbf24 在浅色主题下对比度仅 1.56:1，改为随主题走的警告色。 */
      .swivox-extensions-detail-update { margin-top: 5px; color: var(--swivox-warning); font-size: 13px; }
      .swivox-extensions-detail-actions {
        flex: 0 0 auto;
        display: inline-flex;
        align-items: center;
        gap: 8px;
      }
      .swivox-extensions-detail-button {
        padding: 5px 12px;
        border: 1px solid var(--swivox-border);
        border-radius: 8px;
        background: transparent;
        color: var(--swivox-text-secondary);
        font: inherit;
        font-size: 13px;
        cursor: pointer;
      }
      .swivox-extensions-detail-button:hover { background: var(--swivox-bg-hover); color: var(--swivox-text); }
      .swivox-extensions-detail-primary {
        background: #10a37f;
        border-color: #10a37f;
        color: #fff;
      }
      .swivox-extensions-detail-primary:hover { background: #0e8f70; color: #fff; }
      .swivox-extensions-detail-description {
        margin-top: 14px;
        color: var(--swivox-text-secondary);
        font-size: 13px;
        line-height: 1.6;
      }
      .swivox-extensions-detail-tags {
        display: flex;
        flex-wrap: wrap;
        gap: 6px;
        margin-top: 12px;
      }
      .swivox-extensions-detail-tags span {
        padding: 2px 8px;
        border-radius: 999px;
        background: var(--swivox-bg-hover);
        color: var(--swivox-text-tertiary);
        font-size: 13px;
      }
      .swivox-extensions-detail-section { margin-top: 16px; }
      .swivox-extensions-detail-section-title {
        margin-bottom: 6px;
        font-weight: 600;
        font-size: 13px;
        color: var(--swivox-text);
      }
      .swivox-extensions-detail-section ul {
        margin: 0;
        padding-left: 18px;
        color: var(--swivox-text-secondary);
        font-size: 13px;
        line-height: 1.7;
      }
      .swivox-extensions-detail-link { margin-top: 16px; font-size: 13px; }
      .swivox-extensions-detail-link a { color: var(--swivox-success); word-break: break-all; }
      .swivox-extensions-detail-error {
        margin-top: 14px;
        padding: 8px 10px;
        border-radius: 8px;
        background: var(--swivox-danger-bg);
        color: var(--swivox-danger);
        font-size: 13px;
      }
      .swivox-page-nav-group { margin-bottom: 10px; }
      .swivox-page-nav-group-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 4px 10px;
        color: var(--swivox-text-tertiary);
        font-size: 13px;
        text-transform: uppercase;
        letter-spacing: .04em;
      }
      .swivox-page-nav-group-count {
        min-width: 16px;
        padding: 0 5px;
        border-radius: 999px;
        background: var(--swivox-bg-hover);
        text-align: center;
        font-size: 13px;
      }
      .swivox-page-nav-group-tail { display: inline-flex; align-items: center; gap: 6px; }
      .swivox-page-nav-group-action {
        border: 0;
        background: transparent;
        color: var(--swivox-text-tertiary);
        font: inherit;
        font-size: 13px;
        cursor: pointer;
        padding: 0 2px;
      }
      .swivox-page-nav-group-action:hover { color: var(--swivox-text); }
      .swivox-page-nav-item-action {
        flex: 0 0 auto;
        padding: 2px 8px;
        border: 1px solid var(--swivox-border);
        border-radius: 6px;
        color: var(--swivox-text-secondary);
        font-size: 13px;
      }
      .swivox-page-nav-item:hover .swivox-page-nav-item-action {
        background: var(--swivox-bg-selected);
        color: var(--swivox-text);
      }
      .swivox-modal-close {
        border: 0;
        background: transparent;
        color: #d1d5db;
        font-size: 20px;
        cursor: pointer;
        pointer-events: auto;
        -webkit-app-region: no-drag;
      }
      .swivox-modal-body {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        overscroll-behavior: contain;
        scrollbar-gutter: stable;
        padding: 4px 20px 16px;
        scrollbar-width: thin;
        scrollbar-color: rgba(255,255,255,.28) transparent;
      }
      .swivox-modal-body::-webkit-scrollbar { width: 10px; }
      .swivox-modal-body::-webkit-scrollbar-track { background: transparent; }
      .swivox-modal-body::-webkit-scrollbar-thumb {
        border: 2px solid transparent;
        border-radius: 999px;
        background: rgba(255,255,255,.28);
        background-clip: padding-box;
      }
      .swivox-modal-body::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,.38); background-clip: padding-box; }
      .swivox-row {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 12px;
        padding: 10px 0;
        border-top: 1px solid rgba(255,255,255,.1);
      }
      .swivox-row:first-child { border-top: 0; }
      .swivox-row-title { font-weight: 550; line-height: 1.35; }
      .swivox-row-description { margin-top: 2px; color: #a1a1aa; font-size: 13px; line-height: 1.4; }
      .swivox-model-compat-warning { margin-top: 6px; color: #fbbf24; font-size: 13px; line-height: 1.45; }
      .swivox-toggle {
        width: 42px;
        height: 24px;
        border: 0;
        border-radius: 999px;
        background: #52525b;
        padding: 2px;
      }
      .swivox-toggle span {
        display: block;
        width: 20px;
        height: 20px;
        border-radius: 999px;
        background: white;
        transition: transform .12s ease;
      }
      .swivox-toggle,
      .swivox-action-button,
      .swivox-issue-button,
      .swivox-backend-status {
        flex-shrink: 0;
        align-self: center;
      }
      .swivox-toggle[data-enabled="true"] { background: #10a37f; }
      .swivox-toggle[data-enabled="true"] span { transform: translateX(18px); }
      .swivox-toggle[data-pending="true"],
      .swivox-toggle:disabled { cursor: not-allowed; opacity: .55; }
      .swivox-width-control { display: flex; align-items: center; justify-content: flex-end; gap: 8px; min-width: 176px; align-self: center; }
      .swivox-width-input {
        width: 78px;
        height: 26px;
        box-sizing: border-box;
        border: 1px solid rgba(255,255,255,.18);
        border-radius: 7px;
        background: rgba(255,255,255,.08);
        color: #f3f4f6;
        font-size: 13px;
        font-family: inherit;
        padding: 0 8px;
      }
      .swivox-width-input:disabled { opacity: .55; cursor: not-allowed; }
      .swivox-typing-effect-select {
        flex-shrink: 0;
        align-self: center;
        min-width: 112px;
        height: 32px;
        padding: 0 8px;
        font-size: 13px;
      }
      .swivox-typing-effect-select:disabled { opacity: .55; cursor: not-allowed; }
      .swivox-service-tier-control { display: grid; gap: 6px; min-width: 316px; justify-items: end; align-self: center; }
      .swivox-service-tier-status { color: #a1a1aa; font-size: 13px; line-height: 1.3; text-align: right; }
      .swivox-service-tier-status[data-status="ok"] { color: #34d399; }
      .swivox-service-tier-status[data-status="failed"] { color: #f87171; }
      .swivox-service-tier-status[data-status="unsupported"] { color: #fbbf24; }
      .swivox-service-tier-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }
      .swivox-service-tier-thread-actions { opacity: .88; align-items: center; }
      .swivox-service-tier-thread-label { color: #a1a1aa; font-size: 13px;
        line-height: 1.2;
        font-family: inherit; white-space: nowrap; }
      .swivox-service-tier-button { border: 1px solid rgba(255,255,255,.18); border-radius: 7px; background: #3f3f46; color: #f3f4f6; font-size: 13px;
        font-family: inherit; padding: 5px 8px; white-space: nowrap; }
      .swivox-service-tier-button[data-active="true"] { border-color: #10a37f; background: rgba(16,163,127,.22); color: #6ee7b7; }
      .swivox-service-tier-button:disabled { opacity: .55; cursor: not-allowed; }
      .${codexServiceTierBadgeClass} {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex: 0 0 auto;
        height: 24px;
        min-width: 54px;
        box-sizing: border-box;
        border: 1px solid rgba(148,163,184,.28);
        border-radius: 999px;
        background: rgba(148,163,184,.12);
        color: #d4d4d8;
        font-size: 13px;
        font-weight: 600;
        line-height: 1;
        font-family: inherit;
        padding: 0 8px;
        white-space: nowrap;
        cursor: pointer;
      }
      .${codexServiceTierBadgeClass}:hover { border-color: rgba(16,163,127,.44); background: rgba(16,163,127,.13); }
      .${codexServiceTierBadgeClass}[data-tier="fast"] { border-color: rgba(16,163,127,.55); background: rgba(16,163,127,.18); color: #6ee7b7; }
      .${codexServiceTierBadgeClass}[data-tier="loading"] { color: #a1a1aa; }
      .${codexServiceTierBadgeClass}[data-tier="failed"] { border-color: rgba(248,113,113,.42); background: rgba(248,113,113,.12); color: #fca5a5; }
      .${codexServiceTierBadgeClass}[data-tier="unsupported"] { border-color: rgba(251,191,36,.48); background: rgba(251,191,36,.13); color: #fbbf24; }
      .${codexServiceTierBadgeClass}[data-disabled="true"] { cursor: not-allowed; opacity: .78; }
      .swivox-about { color: #a1a1aa; line-height: 1.5; }
      .swivox-panel[hidden] { display: none; }
      .swivox-action-button,
      .swivox-issue-button { border: 1px solid rgba(255,255,255,.18); border-radius: 7px; background: #3f3f46; color: #f3f4f6; font-size: 13px;
        font-family: inherit; padding: 6px 8px; }
      .swivox-form-field {
        display: grid;
        gap: 4px;
        margin-top: 10px;
        /* issue #2359：原来是写死的 #d4d4d8，浅色主题下对比度只有 1.38:1（≈看不见）。
           标签文字改跟主题走。 */
        color: var(--swivox-text-secondary);
        font-size: 13px;
        font-family: inherit;
        text-align: left;
      }
      .swivox-form-field input {
        width: min(520px, 72vw);
        border: 1px solid rgba(255,255,255,.18);
        border-radius: 8px;
        background: #18181b;
        color: #f4f4f5;
        padding: 8px 10px;
        font: 13px ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      }
      .swivox-form-message {
        min-height: 18px;
        margin-top: 10px;
        color: #a1a1aa;
        font-size: 13px;
        font-family: inherit;
        text-align: left;
      }
      .swivox-form-message[data-status="ok"] { color: #34d399; }
      .swivox-form-message[data-status="failed"] { color: #f87171; }
      .swivox-form-message[data-status="loading"] { color: #fbbf24; }
      .swivox-backend-status { display: grid; gap: 4px; min-width: 132px; justify-items: end; }
      .swivox-backend-label { color: #a1a1aa; font-size: 13px; }
      .swivox-backend-label[data-status="ok"] { color: #34d399; }
      .swivox-backend-label[data-status="failed"] { color: #f87171; }
      .swivox-backend-label[data-status="degraded"] { color: #fbbf24; }
      .swivox-sponsor-text { color: #d1d5db; font-size: 13px; line-height: 1.55; margin: 4px 0 12px; }
      /*
       * 推荐内容：网格卡片。
       *
       * 一张卡 = 图标 + 名称/简介 + 右上箭头 + 底部优惠条。用 auto-fill + minmax
       * 让列数随宽度自适应（宽屏 4 列、窄屏递减），卡片等高对齐。
       */
      .swivox-ad-list {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(268px, 1fr));
        gap: 12px;
      }
      .swivox-ad-card {
        display: flex;
        flex-direction: column;
        gap: 12px;
        border: 1px solid rgba(255,255,255,.08);
        border-radius: 12px;
        background: rgba(255,255,255,.02);
        color: inherit;
        text-decoration: none;
        padding: 16px;
        transition: background .12s ease, border-color .12s ease;
      }
      .swivox-ad-card:hover,
      .swivox-ad-card:focus-visible {
        border-color: rgba(255,255,255,.18);
        background: rgba(255,255,255,.05);
        outline: none;
      }
      .swivox-ad-main { display: flex; align-items: flex-start; gap: 10px; min-width: 0; }
      .swivox-ad-icon {
        flex: 0 0 auto;
        width: 36px;
        height: 36px;
        border-radius: 9px;
        object-fit: contain;
        background: rgba(255,255,255,.06);
      }
      /* 清单没给图时用名称首字占位，比空一块整齐。 */
      .swivox-ad-icon-fallback {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        color: #f3f4f6;
        font-size: 15px;
        font-weight: 600;
      }
      .swivox-ad-text { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
      .swivox-ad-title { overflow: hidden; color: #f8fafc; font-size: 14px; font-weight: 600; line-height: 1.3; text-overflow: ellipsis; white-space: nowrap; }
      /* 简介不再压成一行：卡片按内容撑高，最多 3 行，超出才省略。
         nowrap 会让「提供 Claude 与 ...」这类较长简介只露前几个字。 */
      .swivox-ad-description { display: -webkit-box; overflow: hidden; color: #a1a1aa; font-size: 13px; line-height: 1.4; -webkit-box-orient: vertical; -webkit-line-clamp: 3; }
      .swivox-ad-arrow { flex: 0 0 auto; width: 14px; height: 14px; margin-top: 2px; color: #71717a; }
      .swivox-ad-arrow svg { width: 14px; height: 14px; display: block; }
      .swivox-ad-card:hover .swivox-ad-arrow,
      .swivox-ad-card:focus-visible .swivox-ad-arrow { color: #f3f4f6; }
      /* 底部优惠条：撑满卡片宽度，长文本截断。 */
      .swivox-ad-promo {
        display: block;
        overflow: hidden;
        border-radius: 8px;
        background: rgba(255,255,255,.05);
        color: #f5a97f;
        font-size: 13px;
        line-height: 1.35;
        padding: 7px 10px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .swivox-ad-empty { border: 1px dashed rgba(255,255,255,.16); border-radius: 12px; color: #9ca3af; font-size: 13px; padding: 12px; text-align: center; }
      /*
       * 注入面板的语义色板：前景/背景必须取自同一套 token，且都要能被主题切换带走。
       *
       * issue #2359（Windows 浅色主题下黑底黑字）：旧写法在中间塞了一整层
       * --token-* 回退名（--token-bg-primary / --token-text-primary /
       * --token-border / --token-list-hover-background / --token-text-default /
       * --token-bg-fog / --token-text-tertiary …）。实测 Codex 客户端（26.9xx）
       * 的产物里这些名字**一个都不存在**——真正在用的一套是 --color-token-*。
       * 于是三级回退链的中间那级永远落空，浅色主题下只靠最后一级写死的深色值
       * 兜底，前景与背景各自独立兜底时明度撞在一起，就成了黑底黑字。
       * 这里把死名删掉，直接接到确实存在、且**由 Codex 自己按主题重算**的
       * --color-token-* 上；末级兜底仅作最后保险，不再承担主题判断。
       */
      :root {
        --swivox-bg-primary: var(--color-token-bg-primary, var(--color-token-main-surface-primary, #fff));
        --swivox-bg-secondary: var(--color-token-bg-secondary, var(--swivox-bg-primary));
        --swivox-bg-elevated: var(--color-token-dropdown-background, var(--color-token-bg-secondary, var(--swivox-bg-primary)));
        --swivox-bg-hover: var(--color-token-interactive-bg-secondary-hover, var(--color-token-list-hover-background, rgba(0,0,0,.06)));
        --swivox-bg-selected: var(--color-token-interactive-bg-secondary-selected, var(--swivox-bg-hover));
        --swivox-text: var(--color-token-text-primary, var(--color-token-foreground, #171717));
        --swivox-text-secondary: var(--color-token-text-secondary, var(--swivox-text));
        --swivox-text-tertiary: var(--color-token-text-tertiary, var(--swivox-text-secondary));
        --swivox-border: var(--color-token-border-light, var(--color-token-border, var(--color-token-border-default, rgba(0,0,0,.12))));
        --swivox-border-subtle: var(--swivox-border);
        --swivox-focus: var(--color-token-focus-border, var(--color-border-focus, currentColor));
        --swivox-danger: var(--color-text-danger, #dc2626);
        --swivox-danger-bg: var(--color-background-danger-soft, rgba(220,38,38,.1));
        --swivox-success: var(--color-text-success, #15803d);
        --swivox-warning: var(--color-text-warning, #a16207);
      }
      :where(.${moreMenuClass}, .${actionTooltipClass}, .codex-delete-toast, .codex-delete-confirm-overlay, .swivox-modal-overlay, .${swivoxPageClass}) {
        color: var(--swivox-text);
        font-family: inherit;
      }
      .${moreMenuClass} {
        border-color: var(--swivox-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--swivox-bg-elevated);
        color: var(--swivox-text);
        box-shadow: var(--ui-menu-shadow, var(--shadow-300, 0 8px 24px rgba(0,0,0,.16)));
      }
      .codex-session-more-menu-item { border-radius: var(--border-radius-sm, 6px); font-family: inherit; }
      .codex-session-more-menu-item:hover,
      .codex-session-more-menu-item:focus-visible { background: var(--swivox-bg-hover); }
      .${actionButtonClass} {
        width: var(--h-token-button-composer-sm, 28px);
        height: var(--h-token-button-composer-sm, 28px);
        border-radius: var(--border-radius-lg, 8px);
        color: var(--codex-session-action-color, var(--swivox-text-tertiary));
        font-family: inherit;
      }
      .${actionButtonClass}:hover,
      .${actionButtonClass}:focus-visible {
        background: var(--codex-session-action-hover-background, var(--swivox-bg-hover));
        color: var(--codex-session-action-hover-color, var(--swivox-text));
      }
      .${sessionShareButtonClass}:hover,
      .${sessionShareButtonClass}:focus-visible {
        background: var(--swivox-bg-hover);
        color: var(--swivox-text);
      }
      .${actionTooltipClass} {
        border-color: var(--swivox-border);
        border-radius: var(--border-radius-md, 6px);
        background: var(--swivox-bg-elevated);
        color: var(--swivox-text);
        font-family: inherit;
        font-size: 13px;
        line-height: 16px;
        padding: 6px 8px;
        box-shadow: var(--tooltip-box-shadow, var(--shadow-200, 0 4px 12px rgba(0,0,0,.14)));
      }
      .codex-delete-confirm-overlay,
      .swivox-modal-overlay { background: var(--color-background-surface-under, rgba(0,0,0,.32)); backdrop-filter: blur(1px); }
      .codex-delete-confirm-content,
      .swivox-modal-content {
        border-color: var(--swivox-border);
        border-radius: var(--border-radius-xl, 12px);
        background: var(--swivox-bg-primary);
        color: var(--swivox-text);
        font-family: inherit;
        box-shadow: var(--shadow-400, 0 16px 48px rgba(0,0,0,.2));
      }
      .codex-delete-confirm-message,
      .swivox-row-description,
      .swivox-about,
      .swivox-service-tier-thread-label,
      .swivox-backend-label,
      .swivox-form-message,
      .swivox-sponsor-text { color: var(--swivox-text-secondary); }
      .codex-delete-confirm-actions button,
      .swivox-action-button,
      .swivox-issue-button,
      .swivox-service-tier-button {
        min-height: 32px;
        border: 1px solid var(--swivox-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--swivox-bg-secondary);
        color: var(--swivox-text);
        font: inherit;
        font-size: 13px;
        line-height: 18px;
        padding: 5px 10px;
      }
      .codex-delete-confirm-actions button:hover,
      .codex-delete-confirm-actions button:focus-visible,
      .swivox-action-button:hover,
      .swivox-action-button:focus-visible,
      .swivox-issue-button:hover,
      .swivox-issue-button:focus-visible,
      .swivox-service-tier-button:hover,
      .swivox-service-tier-button:focus-visible,
      .codex-delete-confirm-actions [data-codex-delete-confirm="true"] {
        border-color: var(--color-border-danger, #dc2626);
        background: var(--color-background-danger-solid, #dc2626);
        color: var(--color-text-danger-solid, #fff);
      }
      .swivox-modal-close { border-color: var(--swivox-border); color: var(--swivox-text-secondary); border-radius: var(--border-radius-lg, 8px); }
      .swivox-modal-close:hover,
      .swivox-modal-close:focus-visible { background: var(--swivox-bg-hover); color: var(--swivox-text); outline: none; }
      .${swivoxPageClass},
      .${swivoxPageClass} .swivox-modal-content {
        background: var(--swivox-bg-primary) !important;
        color: var(--swivox-text) !important;
      }
      .${swivoxPageClass} .swivox-modal-content { border-color: transparent; }
      .swivox-modal-body { scrollbar-color: var(--swivox-text-tertiary) transparent; }
      .swivox-modal-body::-webkit-scrollbar-thumb { background: color-mix(in srgb, var(--swivox-text-tertiary) 45%, transparent); background-clip: padding-box; }
      .swivox-modal-body::-webkit-scrollbar-thumb:hover { background: var(--swivox-text-tertiary); background-clip: padding-box; }
      .swivox-row { border-top-color: var(--swivox-border-subtle); }
      .swivox-toggle { background: var(--color-background-secondary-solid, var(--swivox-text-tertiary)); }
      .swivox-toggle span { background: var(--color-token-bg-primary, #fff); box-shadow: var(--switch-thumb-shadow, 0 1px 2px rgba(0,0,0,.16)); }
      .swivox-toggle[data-enabled="true"] { background: var(--color-background-primary-solid, var(--color-background-success-solid, #10a37f)); }
      .swivox-width-input,
      .swivox-typing-effect-select,
      .swivox-form-field input {
        border: 1px solid var(--swivox-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--swivox-bg-secondary);
        color: var(--swivox-text);
        font-family: inherit;
      }
      .swivox-width-input:focus,
      .swivox-typing-effect-select:focus,
      .swivox-form-field input:focus { border-color: var(--swivox-focus); outline: 2px solid color-mix(in srgb, var(--swivox-focus) 25%, transparent); outline-offset: 0; }
      .swivox-service-tier-button[data-active="true"] {
        border-color: var(--color-border-primary, var(--swivox-focus));
        background: var(--color-background-primary-soft, var(--swivox-bg-selected));
        color: var(--color-text-primary, var(--swivox-text));
      }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-status,
      .swivox-backend-indicator { box-shadow: none; }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-status[data-status="ok"],
      .swivox-backend-indicator[data-status="ok"] { background: var(--swivox-success); }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-status[data-status="failed"],
      .swivox-backend-indicator[data-status="failed"] { background: var(--swivox-danger); }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-status[data-status="checking"],
      .swivox-backend-indicator[data-status="checking"] { background: var(--swivox-warning); }
      #${swivoxSidebarNavId} .swivox-sidebar-nav-status[data-status="degraded"],
      .swivox-backend-indicator[data-status="degraded"] { background: var(--swivox-warning); }
      .${codexServiceTierBadgeClass} {
        height: 24px;
        border-color: var(--swivox-border);
        border-radius: var(--border-radius-lg, 8px);
        background: var(--swivox-bg-secondary);
        color: var(--swivox-text-secondary);
        font-family: inherit;
      }
      .${codexServiceTierBadgeClass}:hover { border-color: var(--swivox-focus); background: var(--swivox-bg-hover); }
      .${codexServiceTierBadgeClass}[data-tier="fast"] { border-color: var(--color-border-primary, var(--swivox-focus)); background: var(--color-background-primary-soft, var(--swivox-bg-selected)); color: var(--swivox-text); }
      .${codexServiceTierBadgeClass}[data-tier="failed"] { border-color: var(--color-border-danger, var(--swivox-danger)); background: var(--swivox-danger-bg); color: var(--swivox-danger); }
      .${codexServiceTierBadgeClass}[data-tier="unsupported"] { border-color: var(--color-border-warning, var(--swivox-border)); background: var(--color-background-warning-soft, var(--swivox-bg-hover)); color: var(--swivox-warning); }
      .swivox-ad-card { border-color: var(--swivox-border-subtle); background: var(--swivox-bg-secondary); }
      .swivox-ad-card:hover,
      .swivox-ad-card:focus-visible { border-color: var(--swivox-border); background: var(--swivox-bg-hover); }
      .swivox-ad-icon { background: var(--swivox-bg-hover); }
      .swivox-ad-icon-fallback { color: var(--swivox-text); }
      .swivox-ad-title { color: var(--swivox-text); }
      .swivox-ad-description { color: var(--swivox-text-secondary); }
      .swivox-ad-arrow { color: var(--swivox-text-tertiary); }
      .swivox-ad-card:hover .swivox-ad-arrow,
      .swivox-ad-card:focus-visible .swivox-ad-arrow { color: var(--swivox-text); }
      /*
       * 优惠条：暗底配橙色文字。
       *
       * 底色不能用 --swivox-danger-bg —— 它在当前主题下解析成浅粉（偏浅色主题
       * 的值），压在深色卡片上非常刺眼。改成用警告色按低透明度混出来，深浅主题
       * 都成立，也和原生「需要注意」的语义色同源。
       */
      .swivox-ad-promo {
        background: color-mix(in srgb, var(--swivox-warning) 14%, transparent);
        color: var(--swivox-warning);
      }
      .swivox-ad-empty { border-color: var(--swivox-border); color: var(--swivox-text-tertiary); }
      .swivox-form-message[data-status="ok"], .swivox-service-tier-status[data-status="ok"], .swivox-backend-label[data-status="ok"] { color: var(--swivox-success); }
      .swivox-form-message[data-status="failed"], .swivox-service-tier-status[data-status="failed"], .swivox-backend-label[data-status="failed"] { color: var(--swivox-danger); }
      .swivox-backend-label[data-status="degraded"] { color: var(--swivox-warning); }
      .swivox-form-message[data-status="loading"], .swivox-service-tier-status[data-status="unsupported"], .swivox-model-compat-warning { color: var(--swivox-warning); }
      /* Swivox 设置主页：对齐原生设置页，保持其它页面与弹窗的布局独立。 */
      .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="home"] {
        background: var(--swivox-settings-page-bg, var(--swivox-bg-primary)) !important;
      }
      .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="home"] .swivox-modal-body {
        box-sizing: border-box;
        padding: 72px 32px 64px;
      }
      .swivox-settings {
        width: 100%;
        max-width: 740px;
        min-width: 0;
        margin: 0 auto;
        container: swivox-settings / inline-size;
      }
      .swivox-settings > .swivox-modal-header {
        width: 100%;
        box-sizing: border-box;
        padding: 0;
        margin: 0 0 48px;
        gap: 16px;
      }
      .swivox-settings .swivox-modal-title {
        min-width: 0;
        font-size: 28px;
        font-weight: 600;
        line-height: 36px;
        letter-spacing: -.02em;
        overflow-wrap: anywhere;
      }
      .swivox-settings-section { margin: 0 0 48px; }
      .swivox-settings-section:last-child { margin-bottom: 0; }
      .swivox-settings-section-title {
        margin: 0 0 16px;
        color: var(--swivox-text);
        font-size: 14px;
        font-weight: 550;
        line-height: 20px;
      }
      .swivox-settings-card,
      .swivox-settings > [data-swivox-ext-menu] {
        box-sizing: border-box;
        min-width: 0;
        padding: 0 16px;
        border: 1px solid var(--swivox-border);
        border-radius: 16px;
        background: var(--swivox-settings-card-bg, var(--swivox-bg-secondary));
      }
      .swivox-settings > [data-swivox-ext-menu] { margin-top: 48px; }
      .swivox-settings .swivox-row {
        box-sizing: border-box;
        min-height: 60px;
        align-items: center;
        gap: 24px;
        padding: 10px 0;
        font-size: 14px;
        line-height: 20px;
      }
      .swivox-settings .swivox-row[hidden] { display: none; }
      .swivox-settings .swivox-row > :first-child {
        flex: 1 1 auto;
        min-width: 0;
        overflow-wrap: anywhere;
      }
      .swivox-settings .swivox-row > :not(:first-child) { max-width: 48%; }
      .swivox-settings .swivox-row-title { font-weight: 500; line-height: 20px; }
      .swivox-settings .swivox-row-description,
      .swivox-settings .swivox-about {
        margin-top: 2px;
        color: var(--swivox-text-secondary);
        font-size: 13px;
        line-height: 18px;
        overflow-wrap: anywhere;
      }
      .swivox-settings .swivox-toggle {
        flex: 0 0 32px;
        width: 32px;
        height: 20px;
        min-height: 20px;
        box-sizing: border-box;
        padding: 2px;
        background: var(--swivox-switch-off, #3a3a3a);
        cursor: pointer;
      }
      .swivox-settings .swivox-toggle span {
        width: 16px;
        height: 16px;
        background: var(--swivox-switch-thumb, #fff);
      }
      .swivox-settings .swivox-toggle[data-enabled="true"] { background: var(--swivox-switch-on, #3485ff); }
      .swivox-settings .swivox-toggle[data-enabled="true"] span { transform: translateX(12px); }
      .swivox-settings .swivox-toggle[data-pending="true"],
      .swivox-settings .swivox-toggle:disabled { cursor: not-allowed; opacity: .55; }
      .swivox-settings .swivox-width-control {
        flex: 0 1 auto;
        min-width: 0;
        flex-wrap: wrap;
        gap: 8px;
      }
      .swivox-settings .swivox-service-tier-control {
        flex: 0 1 300px;
        min-width: 0;
      }
      .swivox-settings .swivox-service-tier-status { overflow-wrap: anywhere; }
      .swivox-settings .swivox-backend-status { overflow-wrap: anywhere; text-align: right; }
      .swivox-settings .swivox-action-button,
      .swivox-settings .swivox-issue-button,
      .swivox-settings .swivox-service-tier-button,
      .swivox-settings .swivox-width-input,
      .swivox-settings .swivox-typing-effect-select {
        box-sizing: border-box;
        min-height: 32px;
        border: 1px solid var(--swivox-border);
        border-radius: 8px;
        background: var(--swivox-bg-secondary);
        color: var(--swivox-text);
        font-family: inherit;
        font-size: 13px;
        line-height: 18px;
      }
      .swivox-settings .swivox-width-input,
      .swivox-settings .swivox-typing-effect-select { height: 32px; }
      .swivox-settings .swivox-width-input { width: 72px; min-width: 48px; }
      .swivox-settings .swivox-typing-effect-select { min-width: 0; }
      .swivox-settings .swivox-typing-effect-select[hidden] { display: none; }
      .swivox-settings .swivox-dropdown-trigger {
        display: inline-flex;
        flex: 0 0 auto;
        align-items: center;
        align-self: center;
        justify-content: space-between;
        gap: 8px;
        height: 28px;
        min-width: 112px;
        box-sizing: border-box;
        border: 1px solid var(--swivox-border);
        border-radius: 8px;
        padding: 3px 10px;
        background: var(--swivox-bg-secondary);
        color: var(--swivox-text);
        font: inherit;
        font-size: 13px;
        line-height: 20px;
        cursor: pointer;
      }
      .swivox-settings .swivox-dropdown-trigger > svg {
        flex: 0 0 16px;
        width: 16px;
        height: 16px;
        color: var(--swivox-text-secondary);
      }
      .swivox-settings .swivox-dropdown-trigger:hover:not(:disabled),
      .swivox-settings .swivox-dropdown-trigger[aria-expanded="true"] { background: var(--swivox-bg-hover); }
      .swivox-settings .swivox-dropdown-trigger:disabled { cursor: not-allowed; opacity: .55; }
      /* 菜单挂在 overlay 上，避免设置卡片或滚动容器裁切。 */
      .swivox-dropdown-menu {
        position: absolute;
        z-index: 2;
        overflow-y: auto;
        overscroll-behavior: contain;
        width: 220px;
        max-width: calc(100% - 16px);
        box-sizing: border-box;
        padding: 6px;
        border: 1px solid var(--swivox-border);
        border-radius: 14px;
        background: var(--swivox-bg-elevated);
        color: var(--swivox-text);
        font-family: inherit;
        box-shadow: var(--ui-menu-shadow, var(--shadow-300, 0 8px 24px rgba(0,0,0,.16)));
      }
      .swivox-dropdown-menu[hidden] { display: none; }
      .swivox-dropdown-option {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        width: 100%;
        min-height: 28px;
        box-sizing: border-box;
        border: 0;
        border-radius: 8px;
        padding: 4px 12px;
        background: transparent;
        color: inherit;
        font: inherit;
        font-size: 14px;
        line-height: 20px;
        text-align: left;
        cursor: pointer;
      }
      .swivox-dropdown-option > svg {
        flex: 0 0 16px;
        width: 16px;
        height: 16px;
        visibility: hidden;
      }
      .swivox-dropdown-option[aria-selected="true"] > svg { visibility: visible; }
      .swivox-dropdown-option:hover,
      .swivox-dropdown-option:focus-visible {
        background: var(--swivox-bg-hover);
        outline: none;
      }
      .swivox-settings .swivox-action-button,
      .swivox-settings .swivox-issue-button { white-space: normal; overflow-wrap: anywhere; }
      .swivox-settings .swivox-action-button:hover,
      .swivox-settings .swivox-action-button:focus-visible,
      .swivox-settings .swivox-issue-button:hover,
      .swivox-settings .swivox-issue-button:focus-visible,
      .swivox-settings .swivox-service-tier-button:hover,
      .swivox-settings .swivox-service-tier-button:focus-visible {
        border-color: var(--swivox-border);
        background: var(--swivox-bg-hover);
        color: var(--swivox-text);
      }
      .swivox-settings .swivox-service-tier-button[data-active="true"] {
        border-color: var(--swivox-switch-on, #3485ff);
        background: color-mix(in srgb, var(--swivox-switch-on, #3485ff) 16%, var(--swivox-settings-card-bg, var(--swivox-bg-secondary)));
        color: var(--swivox-text);
      }
      .swivox-settings button:focus-visible,
      .swivox-settings input:focus-visible,
      .swivox-settings select:focus-visible {
        outline: 2px solid var(--swivox-switch-on, #3485ff);
        outline-offset: 3px;
      }
      .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="home"] { container: swivox-settings-page / inline-size; }
      @container swivox-settings-page (max-width: 800px) {
        .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="home"] .swivox-modal-body { padding: 48px 24px; }
      }
      @container swivox-settings-page (max-width: 480px) {
        .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="home"] .swivox-modal-body { padding: 32px 16px; }
      }
      @container swivox-settings (max-width: 560px) {
        .swivox-settings .swivox-row { gap: 16px; }
        .swivox-settings .swivox-row[data-codex-service-tier-controls="true"] { flex-wrap: wrap; }
        .swivox-settings .swivox-row[data-codex-service-tier-controls="true"] > :first-child { flex-basis: 100%; }
        .swivox-settings .swivox-row[data-codex-service-tier-controls="true"] > .swivox-service-tier-control { flex-basis: 100%; max-width: 100%; }
      }
      /* 拓展与推荐内容使用原生插件页面的导航、标题和轻量列表布局。 */
      .${swivoxPageClass} .swivox-modal-content:is([data-swivox-active-tab="extensions"], [data-swivox-active-tab="sponsor"]) {
        background: var(--swivox-settings-page-bg, var(--swivox-bg-primary)) !important;
        container: swivox-discovery-page / inline-size;
      }
      .${swivoxPageClass} .swivox-modal-content:is([data-swivox-active-tab="extensions"], [data-swivox-active-tab="sponsor"]) > .swivox-modal-header { display: none; }
      .${swivoxPageClass} .swivox-modal-content:is([data-swivox-active-tab="extensions"], [data-swivox-active-tab="sponsor"]) .swivox-modal-body {
        box-sizing: border-box;
        padding: 32px 32px 56px;
      }
      .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav {
        box-sizing: border-box;
        width: 312px;
        flex-basis: 312px;
        background: var(--swivox-nav-bg, var(--swivox-bg-secondary));
      }
      .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav-item[data-active="true"] { background: var(--swivox-nav-selected-bg, var(--swivox-bg-selected)); }
      .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav-header { padding: 16px; }
      .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav-title { font-size: 18px; line-height: 24px; }
      .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav-body { padding: 0 12px 20px; }
      .swivox-extensions-nav-entry {
        min-height: 36px;
        gap: 10px;
        box-sizing: border-box;
        padding: 8px 12px;
        font-size: 14px;
        font-weight: 400;
        line-height: 20px;
      }
      .swivox-extensions-nav-entry .swivox-page-nav-item-icon,
      .swivox-extensions-nav-entry .swivox-extensions-icon {
        flex: 0 0 20px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 20px;
        height: 20px;
        padding: 0;
        background: transparent;
      }
      .swivox-extensions-nav-entry .swivox-page-nav-item-icon svg,
      .swivox-extensions-nav-entry .swivox-extensions-icon svg,
      .swivox-extensions-nav-entry .swivox-extensions-icon-img { width: 20px; height: 20px; }
      .swivox-extensions-nav-entry .swivox-extensions-icon-img { display: block; object-fit: contain; }
      .swivox-extensions-nav-entry .swivox-extensions-item-name { font-weight: 400; }
      .swivox-extensions-nav-entry + .swivox-page-nav-group { margin-top: 24px; }
      .swivox-extensions-nav-entry:hover .swivox-extensions-item-name { text-decoration: none; }
      .swivox-discovery {
        width: 100%;
        max-width: 900px;
        min-width: 0;
        margin: 0 auto;
        padding: 0;
        container: swivox-discovery / inline-size;
      }
      .swivox-discovery-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 24px;
      }
      .swivox-discovery-header > :first-child { flex: 1 1 auto; min-width: 0; }
      .swivox-discovery-title {
        margin: 0;
        color: var(--swivox-text);
        font-size: 26px;
        font-weight: 600;
        line-height: 34px;
        letter-spacing: -.02em;
        overflow-wrap: anywhere;
      }
      .swivox-discovery-description {
        margin: 12px 0 0;
        color: var(--swivox-text-secondary);
        font-size: 14px;
        line-height: 20px;
        overflow-wrap: anywhere;
      }
      .swivox-discovery-toolbar { display: flex; flex: 0 1 auto; align-items: center; gap: 8px; min-width: 0; }
      .swivox-discovery-search {
        display: flex;
        flex: 0 1 240px;
        align-items: center;
        gap: 8px;
        width: 240px;
        max-width: 100%;
        min-width: 0;
        box-sizing: border-box;
        border: 1px solid var(--swivox-border);
        border-radius: 999px;
        padding: 8px 12px;
        background: transparent;
        color: var(--swivox-text-secondary);
      }
      .swivox-discovery-search > svg { flex: 0 0 16px; width: 16px; height: 16px; }
      .swivox-discovery-search input {
        width: 100%;
        min-width: 0;
        border: 0;
        padding: 0;
        outline: none;
        background: transparent;
        color: var(--swivox-text);
        font: inherit;
        font-size: 13px;
        line-height: 18px;
      }
      .swivox-discovery-search input::placeholder { color: var(--swivox-text-tertiary); }
      .swivox-discovery-search:focus-within {
        outline: 2px solid var(--swivox-switch-on, #3485ff);
        outline-offset: 2px;
      }
      .swivox-discovery-icon-button {
        display: inline-flex;
        flex: 0 0 32px;
        align-items: center;
        justify-content: center;
        width: 32px;
        height: 32px;
        box-sizing: border-box;
        border: 0;
        border-radius: 8px;
        padding: 0;
        background: transparent;
        color: var(--swivox-text-secondary);
        cursor: pointer;
      }
      .swivox-discovery-icon-button svg { width: 16px; height: 16px; }
      .swivox-discovery-icon-button:hover { background: var(--swivox-bg-hover); color: var(--swivox-text); }
      .swivox-discovery-icon-button:disabled { opacity: .55; cursor: not-allowed; }
      .swivox-extensions-filter { display: flex; align-items: center; gap: 8px; margin-top: 16px; }
      .swivox-extensions-filter button {
        border: 0;
        border-radius: 999px;
        padding: 8px 16px;
        background: transparent;
        color: var(--swivox-text-secondary);
        font: inherit;
        font-size: 13px;
        line-height: 20px;
        cursor: pointer;
      }
      .swivox-extensions-filter button[data-active="true"] { background: var(--swivox-bg-selected); color: var(--swivox-text); }
      .swivox-extensions-filter button:hover { background: var(--swivox-bg-hover); }
      .swivox-discovery-section-title {
        margin: 32px 0 16px;
        color: var(--swivox-text);
        font-size: 16px;
        font-weight: 550;
        line-height: 24px;
      }
      .swivox-extensions-grid,
      .swivox-discovery .swivox-ad-list { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 32px; }
      .swivox-extensions-grid > .swivox-extensions-detail-empty,
      .swivox-discovery .swivox-ad-empty { grid-column: 1 / -1; }
      .swivox-extension-card {
        display: flex;
        align-items: center;
        gap: 16px;
        min-width: 0;
        min-height: 56px;
        box-sizing: border-box;
        border: 0;
        padding: 8px 0;
        background: transparent;
      }
      .swivox-extension-card-select {
        display: flex;
        flex: 1 1 auto;
        align-items: center;
        gap: 12px;
        min-width: 0;
        border: 0;
        border-radius: 8px;
        padding: 0;
        background: transparent;
        color: var(--swivox-text);
        font: inherit;
        text-align: left;
        cursor: pointer;
      }
      .swivox-extension-card-icon {
        display: inline-flex;
        flex: 0 0 36px;
        align-items: center;
        justify-content: center;
        width: 36px;
        height: 36px;
        box-sizing: border-box;
        border: 1px solid var(--swivox-border);
        border-radius: 10px;
        color: var(--swivox-text-secondary);
      }
      .swivox-extension-card-icon svg { width: 24px; height: 24px; }
      .swivox-extension-card-icon img { width: 32px; height: 32px; object-fit: contain; border-radius: 8px; }
      .swivox-extension-card-copy { display: flex; flex: 1 1 auto; flex-direction: column; gap: 2px; min-width: 0; }
      .swivox-extension-card-title {
        overflow: hidden;
        color: var(--swivox-text);
        font-size: 14px;
        font-weight: 500;
        line-height: 20px;
        white-space: nowrap;
        text-overflow: ellipsis;
      }
      .swivox-extension-card-description {
        display: block;
        overflow: hidden;
        color: var(--swivox-text-secondary);
        font-size: 13px;
        line-height: 18px;
        white-space: nowrap;
        text-overflow: ellipsis;
      }
      .swivox-extension-card-actions { display: inline-flex; flex: 0 0 auto; align-items: center; gap: 4px; }
      .swivox-discovery .swivox-extensions-detail { max-width: 900px; margin: 0 auto; padding: 0 0 24px; }
      .swivox-discovery .swivox-extensions-detail-head { align-items: center; border-bottom: 0; gap: 20px; padding-bottom: 20px; }
      .swivox-discovery .swivox-extensions-detail-title { font-size: 26px; line-height: 34px; overflow-wrap: anywhere; }
      .swivox-discovery .swivox-extensions-detail-icon img { width: 40px; height: 40px; object-fit: contain; border-radius: 8px; }
      .swivox-discovery .swivox-extensions-detail-description { font-size: 14px; line-height: 20px; overflow-wrap: anywhere; }
      .swivox-discovery .swivox-extensions-detail-actions { flex-wrap: wrap; }
      .swivox-discovery .swivox-extensions-detail-button { min-height: 32px; box-sizing: border-box; }
      .swivox-discovery .swivox-extensions-detail-primary {
        border-color: var(--swivox-text);
        background: var(--swivox-text);
        color: var(--swivox-settings-page-bg, var(--swivox-bg-primary));
      }
      .swivox-discovery .swivox-extensions-detail-primary:hover {
        background: color-mix(in srgb, var(--swivox-text) 90%, transparent);
        color: var(--swivox-settings-page-bg, var(--swivox-bg-primary));
      }
      .swivox-discovery .swivox-extensions-detail-link a { color: var(--color-text-link, var(--color-token-text-link, var(--swivox-switch-on, #3485ff))); }
      .swivox-discovery .swivox-toggle { flex: 0 0 32px; width: 32px; height: 20px; min-height: 20px; box-sizing: border-box; padding: 2px; background: var(--swivox-switch-off, #3a3a3a); cursor: pointer; }
      .swivox-discovery .swivox-toggle span { width: 16px; height: 16px; background: var(--swivox-switch-thumb, #fff); }
      .swivox-discovery .swivox-toggle[data-enabled="true"] { background: var(--swivox-switch-on, #3485ff); }
      .swivox-discovery .swivox-toggle[data-enabled="true"] span { transform: translateX(12px); }
      .swivox-discovery .swivox-toggle[data-pending="true"],
      .swivox-discovery .swivox-toggle:disabled { cursor: not-allowed; opacity: .55; }
      .swivox-extensions-back {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        margin: 0 0 24px;
        border: 0;
        border-radius: 8px;
        padding: 4px 0;
        background: transparent;
        color: var(--swivox-text-secondary);
        font: inherit;
        font-size: 13px;
        line-height: 20px;
        cursor: pointer;
      }
      .swivox-extensions-back svg { width: 16px; height: 16px; }
      .swivox-extensions-back:hover { color: var(--swivox-text); }
      .swivox-discovery button:focus-visible,
      .swivox-discovery a:focus-visible,
      .swivox-extensions-nav-entry:focus-visible { outline: 2px solid var(--swivox-switch-on, #3485ff); outline-offset: 3px; }
      .swivox-discovery .swivox-ad-card {
        flex-direction: row;
        align-items: center;
        gap: 16px;
        min-height: 72px;
        min-width: 0;
        box-sizing: border-box;
        border: 0;
        border-radius: 8px;
        padding: 8px 0;
        background: transparent;
      }
      .swivox-discovery .swivox-ad-card:hover,
      .swivox-discovery .swivox-ad-card:focus-visible { background: transparent; }
      .swivox-discovery .swivox-ad-main { flex: 1 1 auto; align-items: center; gap: 12px; }
      .swivox-discovery .swivox-ad-icon { flex: 0 0 36px; border: 1px solid var(--swivox-border); border-radius: 10px; background: transparent; box-sizing: border-box; }
      .swivox-discovery .swivox-ad-text { gap: 4px; }
      .swivox-discovery .swivox-ad-title { font-size: 14px; font-weight: 500; line-height: 20px; }
      .swivox-discovery .swivox-ad-description { font-size: 13px; line-height: 18px; -webkit-line-clamp: 2; overflow-wrap: anywhere; }
      .swivox-discovery .swivox-ad-promo { border-radius: 0; padding: 0; background: transparent; color: var(--swivox-text-secondary); font-size: 12px; line-height: 16px; white-space: normal; overflow-wrap: anywhere; }
      .swivox-discovery .swivox-ad-arrow { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; margin: 0; color: var(--swivox-text-secondary); }
      .swivox-discovery .swivox-ad-arrow svg { width: 16px; height: 16px; }
      @container swivox-discovery-page (max-width: 900px) {
        .swivox-discovery-header { flex-wrap: wrap; gap: 20px; }
        .swivox-discovery-toolbar { flex-basis: 100%; }
        .swivox-discovery .swivox-extensions-detail-head { flex-wrap: wrap; }
        .swivox-discovery .swivox-extensions-detail-actions { flex-basis: 100%; }
      }
      @container swivox-discovery (max-width: 600px) {
        .swivox-extensions-grid,
        .swivox-discovery .swivox-ad-list { grid-template-columns: minmax(0, 1fr); }
      }
      @container swivox-discovery-page (max-width: 700px) {
        .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav { width: 180px; flex-basis: 180px; }
        .${swivoxPageClass} .swivox-modal-content:is([data-swivox-active-tab="extensions"], [data-swivox-active-tab="sponsor"]) .swivox-modal-body { padding: 24px 20px 40px; }
      }
      @container swivox-discovery-page (max-width: 480px) {
        .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav { width: 56px; flex-basis: 56px; }
        .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav-header { display: none; }
        .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav-body { padding: 12px 8px; }
        .swivox-extensions-nav-entry { justify-content: center; padding: 8px; }
        .swivox-extensions-nav-entry .swivox-page-nav-item-text,
        .swivox-extensions-nav-entry .swivox-page-nav-item-name,
        .swivox-extensions-nav-entry .swivox-extensions-item-name,
        .swivox-extensions-nav-entry .swivox-page-nav-item-state,
        .swivox-extensions-nav-entry .swivox-extensions-item-state { display: none; }
        .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav-group-head,
        .${swivoxPageClass} .swivox-modal-content[data-swivox-active-tab="extensions"] .swivox-page-nav-empty { display: none; }
        .${swivoxPageClass} .swivox-modal-content:is([data-swivox-active-tab="extensions"], [data-swivox-active-tab="sponsor"]) .swivox-modal-body { padding: 24px 16px 40px; }
      }
    `;
      let hash = 0x811c9dc5;
      for (let index = 0; index < text.length; index += 1) {
        hash ^= text.charCodeAt(index);
        hash = Math.imul(hash, 0x01000193);
      }
      installStyle.sheet = { text, version: `${codexDeleteStyleVersion}-${(hash >>> 0).toString(36)}` };
    }
    const { text, version } = installStyle.sheet;
    const existingStyle = document.getElementById(styleId);
    if (existingStyle?.dataset.codexDeleteStyleVersion === version) return;
    existingStyle?.remove();
    const style = document.createElement("style");
    style.id = styleId;
    style.dataset.codexDeleteStyleVersion = version;
    style.textContent = text;
    document.documentElement.appendChild(style);
  }

  function defaultSwivoxSettings() {
    // capacityAutoRetry* 必须写成字面量：多个测试（typing-effects-settings、whale-widget、
    // session-share-runtime…）把 defaultSwivoxSettings 切片到不带 prelude 的沙箱里执行，
    // 引用 prelude 常量会直接 ReferenceError。与 00-prelude.js 里
    // capacityAutoRetryDefaultMaxAttempts / capacityAutoRetryDefaultIntervalSeconds 的一致性
    // 由 capacity-auto-retry.test.ts 的防漂移用例守住。
    return { pluginMarketplaceUnlock: true, modelWhitelistUnlock: true, sessionDelete: true, markdownExport: true, sessionShare: true, pasteFix: false, typingEffect: "off", threadIdBadge: false, conversationView: false, whaleWidget: false, conversationViewMaxWidth: conversationViewDefaultWidth, threadScrollRestore: true, nativeMenuPlacement: true, serviceTierControls: false, petRealMouseLook: false, activityOverlayRight: false, stepwise: false, answerOutline: false, usageStatusLine: true, capacityAutoRetry: false, capacityAutoRetryMaxAttempts: 15, capacityAutoRetryIntervalSeconds: 0 };
  }

  const swivoxBackendSettingMap = {
    pluginMarketplaceUnlock: "codexAppPluginMarketplaceUnlock",
    modelWhitelistUnlock: "codexAppModelWhitelistUnlock",
    sessionDelete: "codexAppSessionDelete",
    markdownExport: "codexAppMarkdownExport",
    sessionShare: "codexAppSessionShare",
    threadIdBadge: "codexAppThreadIdBadge",
    conversationView: "codexAppConversationView",
    whaleWidget: "codexAppWhaleWidgetEnabled",
    threadScrollRestore: "codexAppThreadScrollRestore",
    nativeMenuPlacement: "codexAppNativeMenuPlacement",
    serviceTierControls: "codexAppServiceTierControls",
    petRealMouseLook: "codexAppPetRealMouseLook",
    activityOverlayRight: "codexAppActivityOverlayRight",
    stepwise: "codexAppStepwiseEnabled",
    answerOutline: "codexAppAnswerOutlineEnabled",
    usageStatusLine: "codexAppUsageStatusLine",
    capacityAutoRetry: "codexAppCapacityAutoRetry",
    pasteFix: "codexAppPasteFix",
    typingEffect: "codexAppTypingEffect",
  };
  const swivoxBackendMappedSettings = new Set(Object.keys(swivoxBackendSettingMap));

  function backendSwivoxSettings() {
    const settings = {};
    Object.entries(swivoxBackendSettingMap).forEach(([localKey, backendKey]) => {
      const value = swivoxBackendSettings[backendKey];
      if (typeof value === "boolean" || typeof value === "string" || (value && typeof value === "object" && !Array.isArray(value))) {
        settings[localKey] = value;
      }
    });
    return settings;
  }

  function swivoxSettings() {
    if (swivoxBackendSettings.enhancementsEnabled === false) {
      return {
        pluginMarketplaceUnlock: false,
        modelWhitelistUnlock: false,
        sessionDelete: false,
        markdownExport: false,
        sessionShare: false,
        pasteFix: false,
        typingEffect: "off",
        threadIdBadge: false,
        conversationView: false,
        whaleWidget: false,
        conversationViewMaxWidth: conversationViewDefaultWidth,
        threadScrollRestore: false,
        nativeMenuPlacement: false,
        serviceTierControls: false,
        petRealMouseLook: false,
        activityOverlayRight: false,
        stepwise: false,
        answerOutline: false,
        usageStatusLine: false,
        capacityAutoRetry: false,
        capacityAutoRetryMaxAttempts: 15,
        capacityAutoRetryIntervalSeconds: 0,
      };
    }
    try {
      const settings = { ...defaultSwivoxSettings(), ...JSON.parse(localStorage.getItem(swivoxSettingsKey) || "{}"), ...backendSwivoxSettings() };
      delete settings.customLayout;
      return settings;
    } catch {
      const settings = { ...defaultSwivoxSettings(), ...backendSwivoxSettings() };
      return settings;
    }
  }


  function setSwivoxSetting(key, value) {
    const backendKey = swivoxBackendSettingMap[key];
    if (backendKey) {
      if (key === "stepwise") syncStepwisePanel(value);
      if (key === "answerOutline") syncStepwisePanel(undefined, value);
      void setBackendSetting(backendKey, value).then(() => {
        if (key === "stepwise" || key === "answerOutline") {
          Promise.resolve(window.__codexStepwisePanel?.loadSettings?.()).then(() => syncStepwisePanel());
        }
        // 容量错误自动重试：开关落库后立即启停轮询，不必等下一次心跳。
        if (key === "capacityAutoRetry") syncSwivoxCapacityAutoRetry();
      }).catch(() => {
        void loadBackendSettings();
      });
      return;
    }
    let stored = {};
    try {
      stored = JSON.parse(localStorage.getItem(swivoxSettingsKey) || "{}");
    } catch {
      stored = {};
    }
    const next = { ...stored, [key]: value };
    localStorage.setItem(swivoxSettingsKey, JSON.stringify(next));
    if (key === "threadScrollRestore" && !value) {
      clearTimeout(window.__codexThreadScrollSaveTimer);
      window.__codexThreadScrollSaveTimer = null;
      window.__codexThreadScrollRestoreRevision = (window.__codexThreadScrollRestoreRevision || 0) + 1;
      window.__codexThreadScrollSyncRevision = (window.__codexThreadScrollSyncRevision || 0) + 1;
      (window.__codexThreadScrollRestoreTimers || []).forEach((timer) => clearTimeout(timer));
      window.__codexThreadScrollRestoreTimers = [];
      (window.__codexThreadScrollSyncTimers || []).forEach((timer) => clearTimeout(timer));
      window.__codexThreadScrollSyncTimers = [];
      window.__codexThreadScrollRuntime = null;
    }
    if (key === "serviceTierControls") {
      if (value) {
        void loadCodexServiceTierState();
      } else {
        removeCodexServiceTierBadges();
        refreshCodexServiceTierControls();
      }
    }
    if (key === "stepwise") syncStepwisePanel(value);
    renderSwivoxMenu();
    scan();
  }

  function syncStepwisePanel(
    enabled = swivoxSettings().stepwise,
    answerOutlineEnabled = swivoxSettings().answerOutline
  ) {
    try {
      window.__codexStepwisePanel?.syncSettings?.({
        enabled: !!enabled,
        answerOutlineEnabled: !!answerOutlineEnabled,
      });
    } catch (error) {
      sendSwivoxDiagnostic("stepwise_sync_failed", {
        errorName: error?.name || "",
        errorMessage: error?.message || String(error),
      });
    }
  }

  function normalizeConversationViewWidth(value) {
    if (value === null || value === undefined || String(value).trim() === "") return null;
    const number = Number(value);
    if (!Number.isFinite(number)) return null;
    return Math.max(conversationViewMinWidth, Math.min(conversationViewMaxAllowedWidth, Math.round(number)));
  }

  function conversationViewWidth() {
    const settingsWidth = normalizeConversationViewWidth(swivoxSettings().conversationViewMaxWidth);
    if (settingsWidth) return settingsWidth;
    const legacyWidth = normalizeConversationViewWidth(localStorage.getItem(conversationViewLegacyWidthKey));
    return legacyWidth || conversationViewDefaultWidth;
  }

  function refreshConversationViewControls() {
    const enabled = !!swivoxSettings().conversationView;
    const width = conversationViewWidth();
    document.querySelectorAll("[data-swivox-conversation-view-width]").forEach((input) => {
      input.value = String(width);
      input.disabled = !enabled;
    });
  }

  function setConversationViewWidth(value) {
    const width = normalizeConversationViewWidth(value);
    if (!width) return;
    setSwivoxSetting("conversationViewMaxWidth", width);
  }
