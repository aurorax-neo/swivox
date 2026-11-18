#!/usr/bin/env bash
set -euo pipefail

# Swivox Linux .deb 打包脚本
# 用法: sudo bash scripts/installer/linux/build-deb.sh [VERSION]
#       或分步执行下面的各个函数

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GPUI_DIR="$REPO_DIR/apps/swivox-gpui"
BUILD_DIR="$REPO_DIR/.linux-build"
DEB_DIR="$BUILD_DIR/deb"
PACKAGE_NAME="swivox"
# 默认取 workspace 版本，保证与 GPUI 二进制一致。
VERSION="${1:-$(sed -n 's/^version = "\(.*\)"$/\1/p' "$REPO_DIR/Cargo.toml" | head -n 1)}"
ARCH="$(dpkg --print-architecture)"

echo "=== Swivox Linux .deb 构建 ==="
echo "版本: $VERSION"
echo "架构: $ARCH"
echo "仓库: $REPO_DIR"

# ── 1. 安装系统依赖（需要 sudo）───────────────────────────────
install_system_deps() {
    echo ">>> 安装系统依赖..."
    sudo apt-get update -qq
    sudo apt-get install -y --no-install-recommends \
        build-essential \
        curl \
        git \
        pkg-config \
        libssl-dev \
        libsqlite3-dev \
        libdbus-1-dev \
        libglib2.0-dev \
        libgtk-3-dev \
        libayatana-appindicator3-dev \
        libxkbcommon-dev \
        libxkbcommon-x11-dev \
        libwayland-dev \
        libvulkan-dev \
        libx11-xcb-dev \
        libxcb1-dev \
        libfontconfig-dev \
        libfreetype-dev \
        libasound2-dev \
        libz3-dev
    echo ">>> 系统依赖安装完成"
}

# ── 2. 安装 Rust toolchain（无需 sudo）───────────────────────
install_rust() {
    if command -v cargo &>/dev/null; then
        echo ">>> Rust 已安装: $(cargo --version)"
        return
    fi
    # 不再自动执行 `curl | sh`：远程脚本未经校验，且本脚本常以 sudo 运行。
    echo "!!! 未找到 cargo。请先按 https://rustup.rs 的说明自行安装 Rust（stable），然后重新运行本脚本。" >&2
    exit 1
}

# ── 3. 构建 GPUI 原生二进制（纯 Rust） ──────────────────────────────────────
build_rust() {
    echo ">>> 构建 Rust 二进制..."
    export PATH="$HOME/.cargo/bin:$PATH"
    cargo build --release -p swivox-gpui --bin swivox
    echo ">>> Rust 构建完成"
}

# ── 4. 打包 .deb ─────────────────────────────────────────────
package_deb() {
    echo ">>> 打包 .deb..."
    local stage="$DEB_DIR/usr"
    local deb_name="${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"
    local deb_path="$REPO_DIR/dist/linux/$deb_name"

    mkdir -p "$BUILD_DIR"
    mkdir -p "$(dirname "$deb_path")"

    # Clean previous build
    rm -rf "$DEB_DIR"
    mkdir -p "$DEB_DIR"

    # ── 目录结构 ──
    mkdir -p "$stage/bin"
    mkdir -p "$stage/share/applications"
    mkdir -p "$stage/share/pixmaps"
    mkdir -p "$stage/share/doc/$PACKAGE_NAME"
    mkdir -p "$DEB_DIR/DEBIAN"

    # ── 复制二进制 ──
    cp "$REPO_DIR/target/release/swivox" "$stage/bin/"
    chmod 755 "$stage/bin/swivox"

    # ── 复制图标（GPUI 应用图标，1024×1024，放 pixmaps 由桌面环境按需缩放）──
    cp "$GPUI_DIR/icons/icon.png" "$stage/share/pixmaps/${PACKAGE_NAME}.png"

    # ── Desktop 文件 ──
    cat > "$stage/share/applications/${PACKAGE_NAME}.desktop" <<'EOF'
[Desktop Entry]
Name=Swivox
Comment=Configure and launch the official Codex desktop app
Exec=swivox %u
Icon=swivox
Type=Application
Categories=Development;Utility;
Terminal=false
StartupNotify=true
MimeType=x-scheme-handler/swivox;
EOF

    # ── 复制文档 ──
    cp "$REPO_DIR/README.md"      "$stage/share/doc/${PACKAGE_NAME}/"
    cp "$REPO_DIR/CHANGELOG.md"   "$stage/share/doc/${PACKAGE_NAME}/"
    cp "$REPO_DIR/LICENSE"        "$stage/share/doc/${PACKAGE_NAME}/"

    # ── control 文件 ──
    local installed_size
    installed_size=$(du -sk "$stage" | awk '{print $1}')
    cat > "$DEB_DIR/DEBIAN/control" <<EOF
Package: ${PACKAGE_NAME}
Version: ${VERSION}
Section: misc
Priority: optional
Architecture: ${ARCH}
Maintainer: aurorax-neo <https://github.com/aurorax-neo/swivox>
Installed-Size: ${installed_size}
Depends: libc6 (>= 2.34), libgcc-s1 (>= 4.2), libstdc++6 (>= 13), libdbus-1-3, libglib2.0-0t64, libgtk-3-0t64, libayatana-appindicator3-1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libvulkan1, libxcb1, libfontconfig1, libfreetype6, libssl3t64 (>= 3.0.0), libz3-4, libasound2t64

Recommends: codex
Description: Swivox configures and launches OpenAI Codex / ChatGPT desktop apps.
 It provides provider switching, protocol conversion, session management and UI enhancements
 via Chromium DevTools Protocol and a local helper service, without modifying the official
 app.asar or writing patches to the installation directory.
 .
 Includes one application, swivox, for provider and model configuration,
 sessions, enhancements and launching the official desktop app from its window.
EOF

    # ── postinst 脚本 ──
    cat > "$DEB_DIR/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ -d "/usr/share/icons/hicolor" ]; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor/ 2>/dev/null || true
fi
if [ -d "/usr/share/applications" ]; then
    update-desktop-database -q /usr/share/applications/ 2>/dev/null || true
fi
exit 0
EOF
    chmod 755 "$DEB_DIR/DEBIAN/postinst"

    # ── prerm 脚本 ──
    cat > "$DEB_DIR/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ -d "/usr/share/icons/hicolor" ]; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor/ 2>/dev/null || true
fi
exit 0
EOF
    chmod 755 "$DEB_DIR/DEBIAN/prerm"

    # ── 创建 deb ──
    # --root-owner-group：包内文件属主统一为 root:root，避免继承构建者的 UID/GID。
    dpkg-deb --root-owner-group --build "$DEB_DIR" "$deb_path"
    echo ">>> .deb 构建完成: $deb_path"
}

# ── 主流程 ─────────────────────────────────────────────────
main() {
    install_system_deps
    install_rust
    build_rust
    package_deb

    echo ""
    echo "=== 构建完成 ==="
    local deb_name="${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"
    echo "Deb 包路径: $REPO_DIR/dist/linux/$deb_name"
    echo ""
    echo "安装命令: sudo dpkg -i $REPO_DIR/dist/linux/$deb_name"
}

main "$@"
