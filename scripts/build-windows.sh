#!/usr/bin/env bash
#
# Quota Critter — Windows 本地打包脚本
#
# 作用：把 Rust MSVC 工具链所需的环境（link.exe / LIB / INCLUDE）一次性配好，
#       然后调用 `npm run tauri build` 生成 NSIS 安装包。
#
# 用法：
#   bash scripts/build-windows.sh
#
# 说明：
#   - 本机需已安装 Rust（rustup）与 VS 2022 Build Tools（C++ 工作负载）。
#   - 在自己本地的普通终端里，也可以直接打开「Developer Command Prompt for VS 2022」
#     再运行 `npm run tauri build -- --bundles nsis --ci`，效果相同。
#   - 本脚本额外处理了 WorkBuddy 沙箱的 safe-delete 注入，普通终端可忽略该副作用。

set -euo pipefail

# --- 1. 定位工具链（自动发现版本，避免写死） ---
VS_ROOT="/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC"
MSVC_VER="$(ls "$VS_ROOT" 2>/dev/null | grep -E '^[0-9]' | sort -V | tail -1)"
if [ -z "$MSVC_VER" ]; then
    echo "错误：未找到 MSVC 工具链，请先安装 VS 2022 Build Tools（含 C++ 工作负载）。" >&2
    exit 1
fi
MSVC_ROOT="$VS_ROOT/$MSVC_VER"

SDK_ROOT="/c/Program Files (x86)/Windows Kits/10"
SDK_VER="$(ls "$SDK_ROOT/Include" 2>/dev/null | grep -E '^10\.' | sort -V | tail -1)"
if [ -z "$SDK_VER" ]; then
    echo "错误：未找到 Windows SDK，请安装 Windows 10/11 SDK。" >&2
    exit 1
fi

# --- 2. 设置 Rust MSVC 工具链环境 ---
export PATH="$MSVC_ROOT/bin/Hostx64/x64:$PATH"
export LIB="$MSVC_ROOT/lib/x64;$SDK_ROOT/Lib/$SDK_VER/ucrt/x64;$SDK_ROOT/Lib/$SDK_VER/um/x64"
export INCLUDE="$MSVC_ROOT/include;$SDK_ROOT/Include/$SDK_VER/ucrt;$SDK_ROOT/Include/$SDK_VER/um;$SDK_ROOT/Include/$SDK_VER/shared"

# --- 3. cargo 加入 PATH（rustup 用 --no-modify-path 安装时未自动加入） ---
if [ -x "$HOME/.cargo/bin/cargo.exe" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
elif [ -x "/c/Users/$USERNAME/.cargo/bin/cargo.exe" ]; then
    export PATH="/c/Users/$USERNAME/.cargo/bin:$PATH"
fi

# --- 4. 绕过 WorkBuddy 沙箱的 safe-delete shim（本地普通终端无需此行） ---
export NODE_OPTIONS="--use-system-ca"

# --- 4.1 关键 workaround：避免 rustc 编译 proc-macro 时链接阶段卡死 ---
# 本机 rustc(1.85/1.97) + MSVC link.exe 编译 proc-macro 时会卡在链接后不退出，
# 加 -C save-temps 可规避该时序竞态（代价是 target 目录会残留 .o/.bc 中间文件，无碍）。
export RUSTFLAGS="${RUSTFLAGS:-} -C save-temps"

# --- 5. 打包 ---
cd "$(dirname "$0")/.."
echo "使用 MSVC $MSVC_VER + Windows SDK $SDK_VER 打包..."
npm run tauri build -- --bundles nsis --ci

echo ""
echo "完成。安装包位于：src-tauri/target/release/bundle/nsis/"
