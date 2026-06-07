# Совместимость с Astra Linux

## Поддерживаемые версии

| Версия Astra Linux | Основа | Статус | Примечания |
|---|---|---|---|
| 1.8 (fly) | Debian 12 (bookworm) | ✅ Поддерживается | libwebkit2gtk-4.1-0, libssl3 |
| 1.7 (smolensk) | Debian 11 (bullseye) | ✅ Поддерживается | libwebkit2gtk-4.0-37, libssl1.1 |
| 1.6 (orel) | Debian 10 (buster) | ⚠️ Не тестировалось | Может потребоваться обновление зависимостей |

## Зависимости

### Обязательные (включены в .deb depends)

```
libwebkit2gtk-4.0-37 | libwebkit2gtk-4.1-0  # Web rendering engine
libssl3 | libssl1.1                            # TLS/SSL
libgtk-3-0                                      # GTK3 UI toolkit
librsvg2-2                                      # SVG rendering
```

### Опциональные (для полной функциональности)

```
libayatana-appindicator3-1   # System tray indicator
xdg-utils                    # Desktop integration
python3 >= 3.9               # Backend (Hermes Agent)
```

## Репозитории Astra Linux

### Astra Linux 1.8 (fly)

```bash
# /etc/apt/sources.list.d/astra.list
deb https://dl.astralinux.ru/astra/fly/1.8/main contrib non-free
deb https://dl.astralinux.ru/astra/fly/1.8/contrib main contrib
deb https://dl.astralinux.ru/astra/fly/1.8/non-free main non-free
```

### Astra Linux 1.7 (smolensk)

```bash
# /etc/apt/sources.list.d/astra.list
deb https://dl.astralinux.ru/astra/smolensk/stable/main contrib non-free
```

## Установка

### Вариант 1: Через скрипт (рекомендуется)

```bash
# Скачайте .deb и install-astra.sh из Releases
chmod +x install-astra.sh
sudo bash install-astra.sh
```

### Вариант 2: Ручная установка

```bash
# Установите зависимости
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.0-37 libgtk-3-0 librsvg2-2 libssl3

# Установите пакет
sudo dpkg -i autolycus-desktop_0.3.0_amd64.deb
# Если ошибки зависимостей:
sudo apt-get install -f -y
```

### Вариант 3: Из репозитория (когда будет настроен)

```bash
sudo apt-get install autolycus-desktop
```

## Известные проблемы

### 1. Нет репозиториев Astra в sources.list

**Симптом:** `E: Unable to locate package libwebkit2gtk-4.0-37`

**Решение:** Добавьте репозитории Astra Linux (см. выше) и выполните `sudo apt-get update`

### 2. Конфликт версий libssl

**Симптом:** `dpkg: dependency problems prevent configuration`

**Решение:** .deb поддерживает обе версии (`libssl3 | libssl1.1`). Если проблема сохраняется:
```bash
sudo apt-get install -f -y
```

### 3. Wayland vs X11

**Симптом:** Приложение не запускается под Wayland

**Решение:** Tauri использует WebKitGTK который работает через X11. Для Wayland:
```bash
# Запуск через XWayland
GDK_BACKEND=x11 autolycus-desktop
```

### 4. Безопасность Astra Linux (PARSEC/МСВС)

**Симптом:** Приложение блокируется системой безопасности

**Решение:** Добавьте исключение в политики безопасности или запускайте в контейнере.

## Сборка из исходников на Astra Linux

```bash
# Установите зависимости для сборки
sudo apt-get install -y \
    libwebkit2gtk-4.0-dev \
    libssl-dev \
    libgtk-3-dev \
    librsvg2-dev \
    pkg-config \
    build-essential

# Установите Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Установите Node.js 22+
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
sudo apt-get install -y nodejs

# Сборка
git clone https://github.com/NikolayGusev-astra/autolycus-desktop.git
cd autolycus-desktop
npm install
npm run tauri build
```

Результат: `src-tauri/target/release/bundle/deb/autolycus-desktop_0.3.0_amd64.deb`
