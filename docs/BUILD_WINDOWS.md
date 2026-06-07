# Сборка Windows MSI установщика

## Подготовка Windows хоста

### 1. Установка зависимостей

```powershell
# Install Node.js 22+
winget install OpenJS.NodeJS.LTS

# Install Rust
winget install Rustlang.Rustup
# После установки:
rustup target add x86_64-pc-windows-msvc

# Install NSIS (for MSI installer)
winget install NSIS.NISIS

# Install Visual Studio Build Tools (required by Rust)
winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --add Microsoft.VisualStudio.Workload.VCTools"
```

### 2. Клонирование и сборка

```powershell
cd C:\Projects
git clone https://github.com/NikolayGusev-astra/autolycus-desktop.git
cd autolycus-desktop

npm install
npm run tauri build -- --target x86_64-pc-windows-msvc
```

### 3. Результат

MSI установщик появится в:
```
src-tauri\target\x86_64-pc-windows-msvc\release\bundle\msi\Autolycus Desktop_0.3.0_x64_en-US.msi
```

## Структура установщика

- **Установка:** per-machine (для всех пользователей)
- **Путь:** `C:\Program Files\Autolycus Desktop\`
- **Языки:** English + Russian (авто-определение по системе)
- **Ярлыки:** Рабочий стол + Меню Пуск
- **Деинсталляция:** через "Удаление программ"

## Требования Windows

- Windows 10 (1809+) или Windows 11
- WebView2 Runtime (обычно предустановлен, иначе скачается автоматически)
- Visual C++ Redistributable (устанавливается с Tauri bootstrapper)
