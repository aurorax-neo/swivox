fn main() {
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../swivox-gpui/icons/icon.ico");
        resource.set_manifest(include_str!(
            "../swivox-gpui/windows-app-manifest.xml"
        ));
        resource.compile().expect("compile launcher icon resource");
    }
}
