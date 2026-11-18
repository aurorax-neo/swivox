//! macOS 钥匙串写入：密钥不经过命令行参数。
//!
//! `/usr/bin/security add-generic-password -w <secret>` 会把令牌暴露在进程 argv 中，
//! 本机任意进程用 `ps` 即可读到。这里改用 `security -i` 交互模式，通过 stdin 传入命令，
//! 并用 `-X <hex>` 传十六进制编码的密码，避免引号/换行转义问题。

use std::io::Write as _;
use std::process::{Command, Stdio};

use anyhow::Context;

/// 交互模式参数加双引号；服务名/账户名里的 `"` 与 `\` 需要转义。
fn quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 构造交互模式命令行（单独抽出便于测试，不含任何明文密钥）。
pub fn add_generic_password_command(
    service: &str,
    account: &str,
    secret: &[u8],
    update_existing: bool,
    allow_any_application: bool,
) -> anyhow::Result<String> {
    for value in [service, account] {
        anyhow::ensure!(
            !value.contains(['\n', '\r', '\0']),
            "钥匙串服务名或账户名包含非法字符"
        );
    }
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

/// 写入（或更新）一条通用密码。失败时返回错误，调用方不得当作成功。
pub fn add_generic_password(
    service: &str,
    account: &str,
    secret: &[u8],
    update_existing: bool,
    allow_any_application: bool,
) -> anyhow::Result<()> {
    let command = add_generic_password_command(
        service,
        account,
        secret,
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
    // `security -i` 对单条命令失败仍可能返回 0，需要同时检查 stderr。
    anyhow::ensure!(
        output.status.success() && !stderr.to_ascii_lowercase().contains("error"),
        "写入钥匙串失败：{}",
        stderr.trim()
    );
    Ok(())
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
}
