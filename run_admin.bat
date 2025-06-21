@echo off
echo Запуск Hyprland-style Desktop Switcher...
echo.
echo ⚠️ ВНИМАНИЕ: Программа будет запущена от имени администратора
echo для работы с низкоуровневыми хуками клавиатуры
echo.
echo После запуска программа будет перехватывать Win + цифры
echo Windows НЕ БУДЕТ открывать/переключать приложения в панели задач
echo.
pause

REM Проверяем наличие исполняемого файла
if not exist "target\release\hyprland-desktop-switcher.exe" (
    echo Ошибка: Исполняемый файл не найден!
    echo Выполните сначала: cargo build --release
    pause
    exit /b 1
)

REM Проверяем наличие DLL
if not exist "target\release\VirtualDesktopAccessor.dll" (
    echo Ошибка: VirtualDesktopAccessor.dll не найден!
    echo Скопируйте файл в папку target\release\
    pause
    exit /b 1
)

echo Запуск от имени администратора...
powershell -Command "Start-Process 'target\release\hyprland-desktop-switcher.exe' -Verb RunAs"

echo.
echo Программа запущена! Проверьте новое окно с правами администратора.
echo.
echo Горячие клавиши:
echo Win + 1-9: переключение на рабочие столы
echo Win + 0: показать информацию
echo Esc: выход
echo.
pause 