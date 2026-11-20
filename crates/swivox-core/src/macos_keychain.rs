//! macOS 钥匙串静默读写：
//! 1. 读取统一通过 `SecKeychainSetUserInteractionAllowed(false)` + `SecKeychainFindGenericPassword` 在进程内完成，
//!    从根源杜绝后台轮询调用 `/usr/bin/security -w` 触发系统密码授权弹窗。
//! 2. 写入通过 `-X 00 -A` 短指令建立真实系统级 `-A`（Always Allow）ACL 后，再用 `SecKeychainItemModifyAttributesAndData`
//!    直接覆写任意长度密文，既不暴露进程参数，又彻底规避 `security -i` 的 4096 字节行缓冲截断。

/// 交互模式参数加双引号；服务名/账户名里的 `"` 与 `\` 需要转义。
fn quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 构造交互模式命令行（保留供单元测试校验转义与明文隔离行为）。
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

/// 删除指定服务名与账户名的通用密码条目（不存在时静默视为成功）。
pub fn delete_generic_password(service: &str, account: &str) -> anyhow::Result<()> {
    validate_service_and_account(service, account)?;
    #[cfg(target_os = "macos")]
    {
        imp::delete_generic_password_native(service, account);
    }
    Ok(())
}

/// 写入（或更新）一条通用密码，支持任意长度载荷（无 4096 字节上限）且不暴露于进程参数。
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
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    use std::sync::Mutex;

    use anyhow::Context as _;

    use super::add_generic_password_command;

    const ERR_SEC_SUCCESS: i32 = 0;

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
        let deleted_native = with_no_ui(|| unsafe {
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
                return del_status == ERR_SEC_SUCCESS;
            }
            if !item_ref.is_null() {
                CFRelease(item_ref);
            }
            false
        });

        if !deleted_native {
            let _ = Command::new("/usr/bin/security")
                .args(["delete-generic-password", "-s", service, "-a", account])
                .output();
        }
    }

    fn seed_item_via_security_cli(
        service: &str,
        account: &str,
        placeholder: &[u8],
        update_existing: bool,
        allow_any_application: bool,
    ) -> anyhow::Result<()> {
        let command = add_generic_password_command(
            service,
            account,
            placeholder,
            update_existing,
            allow_any_application,
        )?;
        let mut child = Command::new("/usr/bin/security")
            .arg("-i")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("启动 /usr/bin/security 失败")?;
        {
            let mut stdin = child.stdin.take().context("无法写入 security 标准输入")?;
            stdin.write_all(command.as_bytes())?;
        }
        let output = child.wait_with_output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::ensure!(
            output.status.success() && !stderr.to_ascii_lowercase().contains("error"),
            "初始化钥匙串条目失败：{}",
            stderr.trim()
        );
        Ok(())
    }

    pub(super) fn add_generic_password_native(
        service: &str,
        account: &str,
        secret: &[u8],
        update_existing: bool,
        allow_any_application: bool,
    ) -> anyhow::Result<()> {
        // 1. 若旧条目已具备免弹窗修改权限且无需升级 -A，直接原地静默更新任意长度密文
        if update_existing && !allow_any_application {
            let updated_in_place = with_no_ui(|| unsafe {
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
                    let mod_status = SecKeychainItemModifyAttributesAndData(
                        item_ref,
                        std::ptr::null(),
                        secret.len() as u32,
                        secret.as_ptr().cast(),
                    );
                    CFRelease(item_ref);
                    return mod_status == ERR_SEC_SUCCESS;
                }
                if !item_ref.is_null() {
                    CFRelease(item_ref);
                }
                false
            });
            if updated_in_place {
                return Ok(());
            }
        }

        // 2. 需要建立真实系统级 `-A`（Always Allow，允许 `/usr/bin/security`、Antigravity 与本程序免弹窗访问）时，
        //    先删除同名旧条目（不触发解密弹窗），再用 1 字节占位符通过 `security -i` 建立 `-A` ACL（指令仅约 70 字节，绝不触碰 4096 字节上限）。
        if allow_any_application || update_existing {
            delete_generic_password_native(service, account);
        }
        seed_item_via_security_cli(service, account, &[0u8], true, allow_any_application)?;

        // 3. 在已具备 ACL 的条目上，通过原生 `SecKeychainItemModifyAttributesAndData` 静默写入完整密文（无长度上限）
        let mod_status = with_no_ui(|| unsafe {
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
            if find_status != ERR_SEC_SUCCESS || item_ref.is_null() {
                if !item_ref.is_null() {
                    CFRelease(item_ref);
                }
                return find_status;
            }
            let status = SecKeychainItemModifyAttributesAndData(
                item_ref,
                std::ptr::null(),
                secret.len() as u32,
                secret.as_ptr().cast(),
            );
            CFRelease(item_ref);
            status
        });

        anyhow::ensure!(
            mod_status == ERR_SEC_SUCCESS,
            "写入钥匙串完整密文失败 (OSStatus={mod_status})"
        );
        Ok(())
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
    fn native_keychain_roundtrips_large_payload_silently_and_allows_security_cli() {
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

        // 2. `/usr/bin/security` 子进程同样拥有 -A 免弹窗读取权限（模拟 language_server / go-keyring 读取）
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
