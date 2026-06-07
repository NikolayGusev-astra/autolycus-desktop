#!/bin/bash
# Autolycus Desktop installer for Astra Linux 1.7 / 1.8
# Usage: sudo bash install-astra.sh

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

# ── Check root ──
if [[ $EUID -ne 0 ]]; then
    log_error "Запустите с sudo: sudo bash $0"
fi

# ── Detect Astra version —
detect_astra() {
    if [[ -f /etc/astra/version ]]; then
        ASTRA_VER=$(cat /etc/astra/version | head -1)
        log_info "Astra Linux: $ASTRA_VER"
    elif [[ -f /etc/debian_version ]]; then
        DEB_VER=$(cat /etc/debian_version)
        log_info "Debian-based: $DEB_VER"
        ASTRA_VER="debian"
    else
        log_error "Не удалось определить версию Astra Linux"
    fi
}

# ── Check architecture ──
ARCH=$(dpkg --print-architecture)
if [[ "$ARCH" != "amd64" ]]; then
    log_error "Поддерживается только amd64, обнаружен: $ARCH"
fi
log_info "Архитектура: $ARCH"

# ── Check repositories ──
check_repos() {
    if ! grep -q "astralinux" /etc/apt/sources.list /etc/apt/sources.list.d/*.list 2>/dev/null; then
        log_warn "Репозитории Astra Linux не найдены в sources.list"
        log_warn "Добавьте репозитории:"
        echo ""
        echo '  # Astra Linux 1.8 (fly)'
        echo '  deb https://dl.astralinux.ru/astra/fly/1.8/main contrib non-free'
        echo '  deb https://dl.astralinux.ru/astra/fly/1.8/contrib main contrib'
        echo '  deb https://dl.astralinux.ru/astra/fly/1.8/non-free main non-free'
        echo ""
        echo '  # Astra Linux 1.7 (smolensk)'
        echo '  deb https://dl.astralinux.ru/astra/smolensk/stable/main contrib non-free'
        echo ""
        read -p "Продолжить без репозиториев Astra? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 0
        fi
    else
        log_info "Репозитории Astra Linux найдены"
    fi
}

# ── Install dependencies ──
install_deps() {
    log_info "Установка зависимостей..."
    apt-get update -qq

    # Core Tauri dependencies
    local deps=(
        "libwebkit2gtk-4.0-37"
        "libgtk-3-0"
        "librsvg2-2"
        "libssl3"
        "libayatana-appindicator3-1"
        "xdg-utils"
    )

    # Try Debian 12 (bookworm) names if Astra 1.8
    local deps_alt=(
        "libwebkit2gtk-4.1-0"
    )

    for dep in "${deps[@]}"; do
        if dpkg -l "$dep" &>/dev/null; then
            log_info "  $dep — уже установлен"
        elif apt-cache show "$dep" &>/dev/null 2>&1; then
            log_info "  Установка $dep..."
            apt-get install -y -qq "$dep" || true
        else
            # Try alternative package names
            case "$dep" in
                "libwebkit2gtk-4.0-37")
                    if apt-cache show "libwebkit2gtk-4.1-0" &>/dev/null 2>&1; then
                        log_info "  Установка libwebkit2gtk-4.1-0 (alt)..."
                        apt-get install -y -qq "libwebkit2gtk-4.1-0" || true
                    else
                        log_warn "  $dep — не найден в репозиториях, попробуйте вручную"
                    fi
                    ;;
                "libssl3")
                    if dpkg -l "libssl1.1" &>/dev/null; then
                        log_info "  libssl1.1 — уже установлен"
                    else
                        log_warn "  $dep — не найден, может потребоваться libssl1.1"
                    fi
                    ;;
                *)
                    log_warn "  $dep — не найден в репозиториях"
                    ;;
            esac
        fi
    done
}

# ── Install .deb package ──
install_deb() {
    local deb_file=""

    # Find .deb in script directory
    local script_dir
    script_dir="$(cd "$(dirname "$0")" && pwd)"

    deb_file=$(find "$script_dir" -maxdepth 1 -name "*.deb" 2>/dev/null | head -1)

    if [[ -z "$deb_file" ]]; then
        log_error "Файл .deb не найден в $(pwd). Скачайте .deb файл и поместите рядом с этим скриптом."
    fi

    log_info "Установка: $deb_file"
    dpkg -i "$deb_file" || {
        log_warn "dpkg обнаружил проблемы, попытка исправления..."
        apt-get install -f -y -qq
    }

    # Verify
    if dpkg -l "com.autolycus.desktop" &>/dev/null; then
        log_info "Autolycus Desktop установлен успешно!"
    elif dpkg -l | grep -qi "autolycus"; then
        log_info "Autolycus Desktop установлен успешно!"
    else
        log_warn "Не удалось проверить установку. Проверьте в меню приложений."
    fi
}

# ── Main ──
detect_astra
check_repos
install_deps
install_deb

log_info ""
log_info "Готово! Запустите из меню приложений или:"
log_info "  autolycus-desktop"
