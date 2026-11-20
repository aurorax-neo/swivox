//! macOS 钥匙串纯原生静默读写（覆盖 Codex 与 Antigravity 等全部钥匙串场景）：
//! 1. 生产代码完全不调用 `/usr/bin/security` 子进程，彻底消除 `security 想要使用钥匙串中的机密信息` 弹窗。
//! 2. 所有读取、更新、删除、新建均在 `SecKeychainSetUserInteractionAllowed(false)` 保护下于进程内完成，
//!    即使遇到外部进程创建的受限 ACL 条目也会立即静默返回而非阻塞弹窗。
//! 3. 当 `allow_any_application = true` 时，通过原生 `SecAccessCreate` + `SecACLSetContents(..., NULL, ..., 0)`
//!    直接构造系统级无限制访问列表（等价 `-A`），供 `Antigravity`（`go-keyring`）与 `Codex` 免弹窗读取，且无任何长度上限。

/// 交互模式参数加双引号；服务名/账户名里的 `"` 与 `\` 需要转义（保留供单元测试校验转义规则）。
fn quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 构造交互模式命令行（仅保留供单元测试校验转义与明文隔离行为，生产路径不调用 `security` 子进程）。
pub fn add_generic_password_command(
    service: &str,
    account: &str,
    secret: &[u8],
    update_existing: bool,
    allow_any_application: bool,
) -> anyhow::Result<String> {
    validate_service_and_account(service, account)?;
    let mut command = String::from("add-generic-password");
    if update_existing {
        command.push_str(" -U");
    }
    command.push_str(&format!(" -s {} -a {}", quote(service), quote(account)));
    command.push_str(&format!(" -X {}", hex(secret)));
    if allow_any_application {
        command.push_str(" -A");
    }
    command.push('\n');
    Ok(command)
}

fn validate_service_and_account(service: &str, account: &str) -> anyhow::Result<()> {
    for value in [service, account] {
        anyhow::ensure!(
            !value.is_empty() && !value.contains(['\n', '\r', '\0']),
            "钥匙串服务名或账户名包含非法字符或为空"
        );
    }
    Ok(())
}

/// 严格静默读取钥匙串密码：若条目不存在或当前进程不在其免弹窗 ACL 中，立即返回 `None`，绝不弹出系统密码框。
pub fn find_generic_password_silent(service: &str, account: &str) -> Option<Vec<u8>> {
    if validate_service_and_account(service, account).is_err() {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        return imp::find_generic_password_silent_native(service, account);
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// 静默检测指定钥匙串条目是否存在（不解密密码数据，绝不弹窗）。
pub fn has_generic_password_silent(service: &str, account: &str) -> bool {
    if validate_service_and_account(service, account).is_err() {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        return imp::has_generic_password_silent_native(service, account);
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// 静默删除指定服务名与账户名的通用密码条目（不存在时视为成功，不调用 `security` 子进程）。
pub fn delete_generic_password(service: &str, account: &str) -> anyhow::Result<()> {
    validate_service_and_account(service, account)?;
    #[cfg(target_os = "macos")]
    {
        imp::delete_generic_password_native(service, account);
    }
    Ok(())
}

/// 纯原生静默写入（或更新）一条通用密码，支持任意长度载荷且不暴露于进程参数、不调用 `security` 子进程。
pub fn add_generic_password(
    service: &str,
    account: &str,
    secret: &[u8],
    update_existing: bool,
    allow_any_application: bool,
) -> anyhow::Result<()> {
    validate_service_and_account(service, account)?;
    #[cfg(target_os = "macos")]
    {
        imp::add_generic_password_native(
            service,
            account,
            secret,
            update_existing,
            allow_any_application,
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (secret, update_existing, allow_any_application);
        anyhow::bail!("macOS Keychain 仅在 macOS 平台可用")
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, c_void};
    use std::sync::Mutex;

    const ERR_SEC_SUCCESS: i32 = 0;
    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_SEC_GENERIC_PASSWORD_ITEM_CLASS: u32 = u32::from_be_bytes(*b"genp");
    const K_SEC_ACCOUNT_ITEM_ATTR: u32 = u32::from_be_bytes(*b"acct");
    const K_SEC_SERVICE_ITEM_ATTR: u32 = u32::from_be_bytes(*b"svce");
    const K_SEC_LABEL_ITEM_ATTR: u32 = u32::from_be_bytes(*b"labl");

    /// `SecKeychainSetUserInteractionAllowed` 为进程级全局状态，使用互斥锁串行保护。
    static KEYCHAIN_UI_LOCK: Mutex<()> = Mutex::new(());

    #[repr(C)]
    struct SecKeychainAttribute {
        tag: u32,
        length: u32,
        data: *const c_void,
    }

    #[repr(C)]
    struct SecKeychainAttributeList {
        count: u32,
        attr: *const SecKeychainAttribute,
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithBytes(
            alloc: *const c_void,
            bytes: *const u8,
            num_bytes: isize,
            encoding: u32,
            is_external_representation: u8,
        ) -> *const c_void;
        fn CFArrayGetCount(the_array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(the_array: *const c_void, idx: isize) -> *mut c_void;
        fn CFRelease(cf: *const c_void);
    }

    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecKeychainGetUserInteractionAllowed(state: *mut u8) -> i32;
        fn SecKeychainSetUserInteractionAllowed(state: u8) -> i32;

        fn SecKeychainFindGenericPassword(
            keychain_or_array: *const c_void,
            service_name_length: u32,
            service_name: *const c_char,
            account_name_length: u32,
            account_name: *const c_char,
            password_length: *mut u32,
            password_data: *mut *mut c_void,
            item_ref: *mut *mut c_void,
        ) -> i32;

        fn SecKeychainItemFreeContent(
            attr_list: *const SecKeychainAttributeList,
            data: *mut c_void,
        ) -> i32;

        fn SecKeychainItemModifyAttributesAndData(
            item_ref: *mut c_void,
            attr_list: *const SecKeychainAttributeList,
            length: u32,
            data: *const c_void,
        ) -> i32;

        fn SecKeychainItemDelete(item_ref: *mut c_void) -> i32;

        fn SecAccessCreate(
            descriptor: *const c_void,
            trusted_list: *const c_void,
            access_ref: *mut *mut c_void,
        ) -> i32;

        fn SecAccessCopyACLList(
            access_ref: *const c_void,
            acl_list: *mut *const c_void,
        ) -> i32;

        fn SecACLCopyContents(
            acl: *mut c_void,
            application_list: *mut *const c_void,
            description: *mut *const c_void,
            prompt_selector: *mut u16,
        ) -> i32;

        fn SecACLSetContents(
            acl: *mut c_void,
            application_list: *const c_void,
            description: *const c_void,
            prompt_selector: u16,
        ) -> i32;

        fn SecKeychainItemCreateFromContent(
            item_class: u32,
            attr_list: *const SecKeychainAttributeList,
            length: u32,
            data: *const c_void,
            keychain_ref: *mut c_void,
            initial_access: *mut c_void,
            item_ref: *mut *mut c_void,
        ) -> i32;
    }

    fn with_no_ui<T>(f: impl FnOnce() -> T) -> T {
        let _guard = KEYCHAIN_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            let mut prev: u8 = 1;
            let _ = SecKeychainGetUserInteractionAllowed(&mut prev);
            let _ = SecKeychainSetUserInteractionAllowed(0);
            let out = f();
            let _ = SecKeychainSetUserInteractionAllowed(prev);
            out
        }
    }

    pub(super) fn has_generic_password_silent_native(service: &str, account: &str) -> bool {
        with_no_ui(|| unsafe {
            let mut item_ref: *mut c_void = std::ptr::null_mut();
            let status = SecKeychainFindGenericPassword(
                std::ptr::null(),
                service.len() as u32,
                service.as_ptr().cast(),
                account.len() as u32,
                account.as_ptr().cast(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut item_ref,
            );
            if !item_ref.is_null() {
                CFRelease(item_ref);
            }
            status == ERR_SEC_SUCCESS
        })
    }

    pub(super) fn find_generic_password_silent_native(
        service: &str,
        account: &str,
    ) -> Option<Vec<u8>> {
        with_no_ui(|| unsafe {
            let mut pw_len: u32 = 0;
            let mut pw_data: *mut c_void = std::ptr::null_mut();
            let status = SecKeychainFindGenericPassword(
                std::ptr::null(),
                service.len() as u32,
                service.as_ptr().cast(),
                account.len() as u32,
                account.as_ptr().cast(),
                &mut pw_len,
                &mut pw_data,
                std::ptr::null_mut(),
            );
            if status != ERR_SEC_SUCCESS || pw_data.is_null() {
                return None;
            }
            let bytes = if pw_len == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(pw_data.cast::<u8>(), pw_len as usize).to_vec()
            };
            let _ = SecKeychainItemFreeContent(std::ptr::null(), pw_data);
            Some(bytes)
        })
    }

    pub(super) fn delete_generic_password_native(service: &str, account: &str) {
        with_no_ui(|| unsafe {
            loop {
                let mut item_ref: *mut c_void = std::ptr::null_mut();
                let status = SecKeychainFindGenericPassword(
                    std::ptr::null(),
                    service.len() as u32,
                    service.as_ptr().cast(),
                    account.len() as u32,
                    account.as_ptr().cast(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut item_ref,
                );
                if status == ERR_SEC_SUCCESS && !item_ref.is_null() {
                    let del_status = SecKeychainItemDelete(item_ref);
                    CFRelease(item_ref);
                    if del_status != ERR_SEC_SUCCESS {
                        break;
                    }
                } else {
                    if !item_ref.is_null() {
                        CFRelease(item_ref);
                    }
                    break;
                }
            }
        });
    }

    /// 在内存中构造等价于 `security add-generic-password -A` 的无限制 `SecAccessRef`：
    /// 将各常规 ACL 条目的 `applicationList` 设为 `NULL`（允许任意应用免弹窗读取/修改），且全过程零弹窗。
    unsafe fn create_allow_all_access(service: &str) -> anyhow::Result<*mut c_void> {
        let descriptor_cf = unsafe {
            CFStringCreateWithBytes(
                std::ptr::null(),
                service.as_ptr(),
                service.len() as isize,
                K_CF_STRING_ENCODING_UTF8,
                0,
            )
        };
        anyhow::ensure!(!descriptor_cf.is_null(), "创建钥匙串 CFString 描述符失败");

        let mut access_ref: *mut c_void = std::ptr::null_mut();
        let create_status =
            unsafe { SecAccessCreate(descriptor_cf, std::ptr::null(), &mut access_ref) };
        if create_status != ERR_SEC_SUCCESS || access_ref.is_null() {
            unsafe { CFRelease(descriptor_cf) };
            anyhow::bail!("SecAccessCreate 失败 (OSStatus={create_status})");
        }

        let mut acl_list: *const c_void = std::ptr::null();
        if unsafe { SecAccessCopyACLList(access_ref, &mut acl_list) } == ERR_SEC_SUCCESS
            && !acl_list.is_null()
        {
            let count = unsafe { CFArrayGetCount(acl_list) };
            for idx in 0..count {
                let acl = unsafe { CFArrayGetValueAtIndex(acl_list, idx) };
                if acl.is_null() {
                    continue;
                }
                let mut app_list: *const c_void = std::ptr::null();
                let mut desc: *const c_void = std::ptr::null();
                let mut prompt_selector: u16 = 0;
                if unsafe {
                    SecACLCopyContents(acl, &mut app_list, &mut desc, &mut prompt_selector)
                } == ERR_SEC_SUCCESS
                {
                    let effective_desc = if desc.is_null() { descriptor_cf } else { desc };
                    // applicationList 传 NULL 表示允许任意应用免确认访问（即 `-A`）
                    let _ = unsafe {
                        SecACLSetContents(acl, std::ptr::null(), effective_desc, 0)
                    };
                    if !app_list.is_null() {
                        unsafe { CFRelease(app_list) };
                    }
                    if !desc.is_null() {
                        unsafe { CFRelease(desc) };
                    }
                }
            }
            unsafe { CFRelease(acl_list) };
        }

        unsafe { CFRelease(descriptor_cf) };
        Ok(access_ref)
    }

    pub(super) fn add_generic_password_native(
        service: &str,
        account: &str,
        secret: &[u8],
        update_existing: bool,
        allow_any_application: bool,
    ) -> anyhow::Result<()> {
        with_no_ui(|| unsafe {
            if !allow_any_application {
                let mut item_ref: *mut c_void = std::ptr::null_mut();
                let find_status = SecKeychainFindGenericPassword(
                    std::ptr::null(),
                    service.len() as u32,
                    service.as_ptr().cast(),
                    account.len() as u32,
                    account.as_ptr().cast(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut item_ref,
                );
                if find_status == ERR_SEC_SUCCESS && !item_ref.is_null() {
                    if !update_existing {
                        CFRelease(item_ref);
                        anyhow::bail!("钥匙串条目已存在 ({service}/{account})");
                    }
                    let mod_status = SecKeychainItemModifyAttributesAndData(
                        item_ref,
                        std::ptr::null(),
                        secret.len() as u32,
                        secret.as_ptr().cast(),
                    );
                    CFRelease(item_ref);
                    if mod_status == ERR_SEC_SUCCESS {
                        return Ok(());
                    }
                } else if !item_ref.is_null() {
                    CFRelease(item_ref);
                } else if find_status != ERR_SEC_ITEM_NOT_FOUND && find_status != ERR_SEC_SUCCESS {
                    anyhow::bail!("查询钥匙串条目失败 (OSStatus={find_status})");
                }
            }

            if allow_any_application || update_existing {
                // 先在进程内静默移除同名旧条目，确保重建条目携带最新 ACL
                let mut existing_ref: *mut c_void = std::ptr::null_mut();
                if SecKeychainFindGenericPassword(
                    std::ptr::null(),
                    service.len() as u32,
                    service.as_ptr().cast(),
                    account.len() as u32,
                    account.as_ptr().cast(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut existing_ref,
                ) == ERR_SEC_SUCCESS
                    && !existing_ref.is_null()
                {
                    let _ = SecKeychainItemDelete(existing_ref);
                    CFRelease(existing_ref);
                }
            }

            let attrs = [
                SecKeychainAttribute {
                    tag: K_SEC_SERVICE_ITEM_ATTR,
                    length: service.len() as u32,
                    data: service.as_ptr().cast(),
                },
                SecKeychainAttribute {
                    tag: K_SEC_ACCOUNT_ITEM_ATTR,
                    length: account.len() as u32,
                    data: account.as_ptr().cast(),
                },
                SecKeychainAttribute {
                    tag: K_SEC_LABEL_ITEM_ATTR,
                    length: service.len() as u32,
                    data: service.as_ptr().cast(),
                },
            ];
            let attr_list = SecKeychainAttributeList {
                count: attrs.len() as u32,
                attr: attrs.as_ptr(),
            };

            let access_ref = if allow_any_application {
                create_allow_all_access(service)?
            } else {
                std::ptr::null_mut()
            };

            let mut created_item: *mut c_void = std::ptr::null_mut();
            let create_status = SecKeychainItemCreateFromContent(
                K_SEC_GENERIC_PASSWORD_ITEM_CLASS,
                &attr_list,
                secret.len() as u32,
                secret.as_ptr().cast(),
                std::ptr::null_mut(),
                access_ref,
                &mut created_item,
            );

            if !created_item.is_null() {
                CFRelease(created_item);
            }
            if !access_ref.is_null() {
                CFRelease(access_ref);
            }

            anyhow::ensure!(
                create_status == ERR_SEC_SUCCESS,
                "写入钥匙串失败 (OSStatus={create_status})"
            );
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_hex_encodes_secret_and_quotes_names() {
        let command =
            add_generic_password_command("Codex Auth", "cli|ab\"c", b"{\"t\":1}", true, false)
                .unwrap();
        assert!(command.starts_with("add-generic-password -U -s \"Codex Auth\" -a \"cli|ab\\\"c\""));
        assert!(command.contains(" -X 7b2274223a317d"));
        assert!(!command.contains("{\"t\""));
        assert!(!command.contains(" -A"));
        assert!(add_generic_password_command("a\nb", "c", b"x", false, false).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn pure_native_keychain_roundtrips_large_payload_and_grants_allow_all_acl() {
        let service = format!("swivox-ephemeral-test-{}", std::process::id());
        let account = "large-payload-check";
        let large_secret = format!(
            "{{\"prefix\":\"go-keyring-base64:\",\"pad\":\"{}\"}}",
            "A".repeat(6200)
        );

        delete_generic_password(&service, account).unwrap();
        add_generic_password(&service, account, large_secret.as_bytes(), true, true).unwrap();

        // 1. 进程内严格静默读取（绝不弹窗）
        let silent_bytes = find_generic_password_silent(&service, account).unwrap();
        assert_eq!(silent_bytes, large_secret.as_bytes());

        // 2. 验证外部进程（如 Antigravity 的 language_server / go-keyring 调用 `/usr/bin/security`）同样拥有免弹窗 `-A` 读取权限
        let output = std::process::Command::new("/usr/bin/security")
            .args(["find-generic-password", "-s", &service, "-a", account, "-w"])
            .output()
            .unwrap();
        delete_generic_password(&service, account).unwrap();
        assert!(!has_generic_password_silent(&service, account));

        assert!(output.status.success());
        let read_back = String::from_utf8(output.stdout).unwrap();
        assert_eq!(read_back.trim_end_matches('\n'), large_secret);
    }
}
