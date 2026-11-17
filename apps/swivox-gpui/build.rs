fn main() {
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("icons/icon.ico");
        resource.set_manifest(include_str!("windows-app-manifest.xml"));
        resource.compile().expect("compile swivox icon resource");
    }
}
