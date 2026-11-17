(() => {
  if (window.top !== window || window.self !== window || !window.electronBridge || !/^app:\/\/\-\//i.test(window.location.href)) return;
  if (window.__swivoxUserScriptsBootstrap) return;
  window.__swivoxUserScriptsBootstrap = true;
  const load = () => {
    // 每次页面加载都读取当前文件与开关，不保留启动时的旧脚本副本。
    window.__codexSessionDeleteBridge("/user-scripts/load", {}).then((result) => {
      if (result?.status === "failed") console.warn("[Swivox] user scripts:", result.message);
    }).catch((error) => console.warn("[Swivox] user scripts:", error));
  };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", load, { once: true });
  else load();
})();
