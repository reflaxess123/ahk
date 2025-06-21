use std::ffi::{CStr, CString};
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use once_cell::sync::Lazy;
use winapi::shared::minwindef::{BOOL, HINSTANCE, LPARAM, LRESULT, WPARAM};
use winapi::shared::windef::HWND;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::libloaderapi::{GetModuleHandleW, GetProcAddress, LoadLibraryA};
use winapi::um::winuser::*;

// Типы функций из VirtualDesktopAccessor.dll
type GoToDesktopNumberFn = unsafe extern "C" fn(desktop_number: i32) -> i32;
type GetCurrentDesktopNumberFn = unsafe extern "C" fn() -> i32;
type GetDesktopCountFn = unsafe extern "C" fn() -> i32;
type CreateDesktopFn = unsafe extern "C" fn() -> i32;
type RemoveDesktopFn = unsafe extern "C" fn(remove_desktop: i32, fallback_desktop: i32) -> i32;
type IsWindowOnDesktopNumberFn = unsafe extern "C" fn(hwnd: HWND, desktop_number: i32) -> i32;

struct VirtualDesktopAccessor {
    _dll_handle: HINSTANCE,
    go_to_desktop_number: GoToDesktopNumberFn,
    get_current_desktop_number: GetCurrentDesktopNumberFn,
    get_desktop_count: GetDesktopCountFn,
    create_desktop: Option<CreateDesktopFn>,
    remove_desktop: Option<RemoveDesktopFn>,
    is_window_on_desktop_number: Option<IsWindowOnDesktopNumberFn>,
}

impl VirtualDesktopAccessor {
    fn new() -> Result<Self, String> {
        unsafe {
            let dll_path = CString::new("VirtualDesktopAccessor.dll")
                .map_err(|_| "Failed to create DLL path string")?;
            
            let dll_handle = LoadLibraryA(dll_path.as_ptr());
            if dll_handle.is_null() {
                return Err(format!("Failed to load VirtualDesktopAccessor.dll: {}", GetLastError()));
            }

            // Обязательные функции
            let go_to_desktop_number = Self::get_proc_address(dll_handle, "GoToDesktopNumber")?;
            let get_current_desktop_number = Self::get_proc_address(dll_handle, "GetCurrentDesktopNumber")?;
            let get_desktop_count = Self::get_proc_address(dll_handle, "GetDesktopCount")?;

            // Опциональные функции (только для Windows 11)
            let create_desktop = Self::get_proc_address_optional(dll_handle, "CreateDesktop");
            let remove_desktop = Self::get_proc_address_optional(dll_handle, "RemoveDesktop");
            let is_window_on_desktop_number = Self::get_proc_address_optional(dll_handle, "IsWindowOnDesktopNumber");

            println!("VirtualDesktopAccessor загружен успешно!");
            println!("Windows 11 функции: {}", 
                if create_desktop.is_some() && remove_desktop.is_some() { "доступны" } else { "недоступны" });

            Ok(VirtualDesktopAccessor {
                _dll_handle: dll_handle,
                go_to_desktop_number,
                get_current_desktop_number,
                get_desktop_count,
                create_desktop,
                remove_desktop,
                is_window_on_desktop_number,
            })
        }
    }

    unsafe fn get_proc_address<T>(dll_handle: HINSTANCE, name: &str) -> Result<T, String> {
        let name_cstr = CString::new(name).map_err(|_| format!("Invalid function name: {}", name))?;
        let proc_addr = GetProcAddress(dll_handle, name_cstr.as_ptr());
        
        if proc_addr.is_null() {
            return Err(format!("Function '{}' not found in DLL", name));
        }
        
        Ok(mem::transmute_copy(&proc_addr))
    }

    unsafe fn get_proc_address_optional<T>(dll_handle: HINSTANCE, name: &str) -> Option<T> {
        let name_cstr = CString::new(name).ok()?;
        let proc_addr = GetProcAddress(dll_handle, name_cstr.as_ptr());
        
        if proc_addr.is_null() {
            None
        } else {
            Some(mem::transmute_copy(&proc_addr))
        }
    }

    fn get_current_desktop(&self) -> i32 {
        unsafe { (self.get_current_desktop_number)() }
    }

    fn get_desktop_count(&self) -> i32 {
        unsafe { (self.get_desktop_count)() }
    }

    fn go_to_desktop(&self, desktop_number: i32) -> Result<(), String> {
        let result = unsafe { (self.go_to_desktop_number)(desktop_number) };
        if result == -1 {
            Err(format!("Failed to switch to desktop {}", desktop_number + 1))
        } else {
            Ok(())
        }
    }

    fn create_desktop(&self) -> Result<i32, String> {
        match self.create_desktop {
            Some(func) => {
                let result = unsafe { func() };
                if result == -1 {
                    Err("Failed to create desktop".to_string())
                } else {
                    Ok(result)
                }
            }
            None => Err("CreateDesktop function not available (Windows 10?)".to_string()),
        }
    }

    fn remove_desktop(&self, desktop_to_remove: i32, fallback_desktop: i32) -> Result<(), String> {
        match self.remove_desktop {
            Some(func) => {
                let result = unsafe { func(desktop_to_remove, fallback_desktop) };
                if result == -1 {
                    Err(format!("Failed to remove desktop {}", desktop_to_remove + 1))
                } else {
                    Ok(())
                }
            }
            None => Err("RemoveDesktop function not available (Windows 10?)".to_string()),
        }
    }

    fn is_desktop_empty(&self, desktop_number: i32) -> bool {
        match self.is_window_on_desktop_number {
            Some(func) => {
                let mut data = DesktopCheckData {
                    desktop_number,
                    func,
                    has_windows: false,
                };

                unsafe {
                    EnumWindows(Some(check_desktop_windows_proc), 
                        &mut data as *mut DesktopCheckData as LPARAM);
                }

                !data.has_windows
            }
            None => {
                println!("Warning: Cannot check if desktop is empty (function not available)");
                false // Безопасный вариант - не удаляем
            }
        }
    }
}

struct DesktopCheckData {
    desktop_number: i32,
    func: IsWindowOnDesktopNumberFn,
    has_windows: bool,
}

unsafe extern "system" fn check_desktop_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let data = &mut *(lparam as *mut DesktopCheckData);
    
    if IsWindowVisible(hwnd) == 1 {
        let mut class_name = [0u8; 256];
        let mut window_title = [0u8; 256];
        
        GetClassNameA(hwnd, class_name.as_mut_ptr() as *mut i8, 256);
        GetWindowTextA(hwnd, window_title.as_mut_ptr() as *mut i8, 256);
        
        let class_str = CStr::from_ptr(class_name.as_ptr() as *const i8).to_string_lossy();
        let title_str = CStr::from_ptr(window_title.as_ptr() as *const i8).to_string_lossy();
        
        // Фильтруем системные окна
        if !title_str.is_empty() && 
           !class_str.contains("Shell_TrayWnd") &&
           !class_str.contains("DV2ControlHost") &&
           !class_str.contains("ForegroundStaging") &&
           !class_str.contains("ApplicationFrameHost") {
            
            let is_on_desktop = (data.func)(hwnd, data.desktop_number);
            if is_on_desktop == 1 {
                data.has_windows = true;
                return 0; // Прекращаем поиск
            }
        }
    }
    
    1 // Продолжаем перечисление
}

// Глобальные переменные для хука клавиатуры
static mut VDA_INSTANCE: Option<VirtualDesktopAccessor> = None;
static mut LAST_DESKTOP: Option<i32> = None;
static WIN_KEY_PRESSED: AtomicBool = AtomicBool::new(false);

struct HyprlandDesktopSwitcher {
    vda: VirtualDesktopAccessor,
    hook_handle: HHOOK,
}

// Процедура обработки низкоуровневого хука клавиатуры
unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let kb_struct = *(l_param as *const KBDLLHOOKSTRUCT);
        let vk_code = kb_struct.vkCode;
        let is_key_down = w_param == WM_KEYDOWN as WPARAM || w_param == WM_SYSKEYDOWN as WPARAM;
        let is_key_up = w_param == WM_KEYUP as WPARAM || w_param == WM_SYSKEYUP as WPARAM;

        // Отслеживаем состояние Win клавиш
        if vk_code == VK_LWIN as u32 || vk_code == VK_RWIN as u32 {
            WIN_KEY_PRESSED.store(is_key_down, Ordering::Relaxed);
        }

        // Если Win нажат и нажата цифра - обрабатываем
        if is_key_down && WIN_KEY_PRESSED.load(Ordering::Relaxed) {
            match vk_code {
                0x31..=0x39 => {
                    // Цифры 1-9
                    let desktop_number = (vk_code - 0x31) as i32;
                    println!("Перехватили Win + {}", desktop_number + 1);
                    
                    if let Some(ref vda) = VDA_INSTANCE {
                        let current_desktop = vda.get_current_desktop();
                        LAST_DESKTOP = Some(current_desktop);
                        
                        if let Err(e) = switch_to_desktop_static(vda, desktop_number) {
                            println!("Ошибка переключения: {}", e);
                        }
                    }
                    
                    return 1; // Блокируем передачу клавиши дальше
                }
                0x30 => {
                    // Цифра 0 - показать информацию
                    if let Some(ref vda) = VDA_INSTANCE {
                        let current = vda.get_current_desktop();
                        let total = vda.get_desktop_count();
                        println!("📊 Текущий рабочий стол: {}/{}", current + 1, total);
                    }
                    return 1; // Блокируем передачу клавиши дальше
                }
                VK_ESCAPE => {
                    println!("Получен сигнал выхода...");
                    PostQuitMessage(0);
                    return 1;
                }
                _ => {}
            }
        }
    }

    CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param)
}

fn switch_to_desktop_static(vda: &VirtualDesktopAccessor, target_desktop: i32) -> Result<(), String> {
    let current_desktop = vda.get_current_desktop();
    let desktop_count = vda.get_desktop_count();

    println!("Переключение с рабочего стола {} на {}", current_desktop + 1, target_desktop + 1);

    // Создаем рабочие столы если нужно (только для Windows 11)
    if target_desktop >= desktop_count {
        if vda.create_desktop.is_some() {
            let desktops_to_create = target_desktop - desktop_count + 1;
            println!("Создаем {} новых рабочих столов", desktops_to_create);
            
            for _ in 0..desktops_to_create {
                vda.create_desktop()?;
            }
        } else {
            return Err(format!("Рабочий стол {} не существует, а создание недоступно", target_desktop + 1));
        }
    }

    // Переключаемся на целевой рабочий стол
    vda.go_to_desktop(target_desktop)?;

    // Проверяем, нужно ли удалить предыдущий рабочий стол
    unsafe {
        if let Some(last) = LAST_DESKTOP {
            if last != target_desktop && last > 0 {
                // Задержка для стабильности
                thread::sleep(Duration::from_millis(300));
                check_and_remove_empty_desktop_static(vda, last)?;
            }
        }
    }

    Ok(())
}

fn check_and_remove_empty_desktop_static(vda: &VirtualDesktopAccessor, desktop_number: i32) -> Result<(), String> {
    let desktop_count = vda.get_desktop_count();
    
    // Не удаляем единственный рабочий стол
    if desktop_count <= 1 {
        println!("Не удаляем - единственный рабочий стол");
        return Ok(());
    }

    // Не удаляем первый рабочий стол
    if desktop_number == 0 {
        println!("Не удаляем - это первый рабочий стол");
        return Ok(());
    }

    // Проверяем, пуст ли рабочий стол
    let is_empty = vda.is_desktop_empty(desktop_number);
    println!("Рабочий стол {} - {}", desktop_number + 1, if is_empty { "ПУСТ" } else { "НЕ ПУСТ" });

    if is_empty {
        let fallback_desktop = if desktop_number > 0 { desktop_number - 1 } else { 0 };
        match vda.remove_desktop(desktop_number, fallback_desktop) {
            Ok(()) => println!("✓ Удален пустой рабочий стол {}", desktop_number + 1),
            Err(e) => println!("✗ Ошибка удаления рабочего стола {}: {}", desktop_number + 1, e),
        }
    }

    Ok(())
}

impl HyprlandDesktopSwitcher {
    fn new() -> Result<Self, String> {
        let vda = VirtualDesktopAccessor::new()?;
        
        // Устанавливаем глобальную ссылку на VDA
        unsafe {
            VDA_INSTANCE = Some(vda);
            let vda_ref = VDA_INSTANCE.as_ref().unwrap();
        
            // Устанавливаем низкоуровневый хук клавиатуры
            let hook_handle = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(low_level_keyboard_proc),
                GetModuleHandleW(ptr::null()),
                0,
            );

            if hook_handle.is_null() {
                return Err(format!("Не удалось установить хук клавиатуры: {}", GetLastError()));
            }

            Ok(HyprlandDesktopSwitcher {
                vda: std::ptr::read(vda_ref), // Копируем VDA
                hook_handle,
            })
        }
    }

    fn run(&self) -> Result<(), String> {
        println!("🚀 Hyprland-style Desktop Switcher запущен!");
        println!("Текущий рабочий стол: {}/{}", 
            self.vda.get_current_desktop() + 1, 
            self.vda.get_desktop_count());
        println!("\nГорячие клавиши:");
        println!("Win + 1-9: переключение на рабочие столы");
        println!("Win + 0: показать информацию");
        println!("Esc: выход");
        println!("\n⚠️ ВАЖНО: Win клавиши теперь перехватываются на системном уровне!");
        println!("Windows больше НЕ БУДЕТ обрабатывать Win + цифры для панели задач");

        unsafe {
            let mut msg: MSG = mem::zeroed();

            // Основной цикл сообщений
            loop {
                let result = GetMessageW(&mut msg, ptr::null_mut(), 0, 0);
                
                if result == -1 {
                    return Err(format!("Ошибка получения сообщения: {}", GetLastError()));
                } else if result == 0 {
                    // WM_QUIT получено
                    println!("Получено сообщение выхода");
                    break;
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        Ok(())
    }
    
    // Очистка ресурсов
    fn cleanup(&self) {
        unsafe {
            if !self.hook_handle.is_null() {
                UnhookWindowsHookEx(self.hook_handle);
                println!("Хук клавиатуры удален");
            }
        }
    }
}

fn main() {
    println!("Инициализация Hyprland-style Desktop Switcher...");
    
    match HyprlandDesktopSwitcher::new() {
        Ok(switcher) => {
            if let Err(e) = switcher.run() {
                eprintln!("Ошибка выполнения: {}", e);
            }
            
            // Очищаем ресурсы при выходе
            switcher.cleanup();
        }
        Err(e) => {
            eprintln!("Ошибка инициализации: {}", e);
            eprintln!("Убедитесь, что VirtualDesktopAccessor.dll находится в папке с программой");
            eprintln!("Также убедитесь, что программа запущена от имени администратора");
        }
    }
} 