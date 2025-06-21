@echo off
echo Компилируем Hyprland-style Desktop Switcher...

REM Проверяем наличие Rust
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo Ошибка: Rust не установлен!
    echo Установите Rust с https://rustup.rs/
    pause
    exit /b 1
)

REM Сборка в release режиме
echo Сборка релизной версии...
cargo build --release

if %errorlevel% equ 0 (
    echo.
    echo ✓ Сборка завершена успешно!
    echo Исполняемый файл: target\release\hyprland-desktop-switcher.exe
    echo.
    echo Убедитесь, что VirtualDesktopAccessor.dll находится в папке с exe файлом
    pause
) else (
    echo.
    echo ✗ Ошибка сборки!
    pause
    exit /b 1
) 