fn main() {
    #[cfg(windows)]
    {
        // gpui 底层已内置嵌入 gpui.manifest.xml（含 PerMonitorV2、Common-Controls 6.0 与 asInvoker），
        // 此处仅嵌入应用图标，避免 MSVC link.exe 报 CVT1100 重复 MANIFEST 资源错误。
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("icons/icon.ico");
        resource.compile().expect("compile swivox icon resource");
    }
}
