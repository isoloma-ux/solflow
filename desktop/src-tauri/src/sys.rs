//! Мелкие обращения к системе, у которых на каждой платформе свой способ:
//! открыть ссылку, показать файл в проводнике, приглушить звук, узнать
//! версию системы. Всё, что различается, живёт здесь, чтобы в остальном
//! коде не было `#[cfg]` вперемешку с логикой.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

/// Запуск внешней программы без окна консоли. На Windows каждый запуск
/// консольной программы иначе поднимает чёрное окно поверх всего — при
/// импорте так мигали ffmpeg и распаковка, хотя работа шла в фоне.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Открывает ссылку в браузере (или почтовом клиенте для mailto:).
#[cfg(target_os = "macos")]
pub fn open_url(url: &str) {
    let _ = Command::new("/usr/bin/open").arg(url).spawn();
}

/// rundll32 с FileProtocolHandler — единственный способ открыть ссылку в
/// Windows, не воюя с тем, как cmd разбирает кавычки и знак «&».
#[cfg(windows)]
pub fn open_url(url: &str) {
    let _ = command("rundll32")
        .arg("url.dll,FileProtocolHandler")
        .arg(url)
        .spawn();
}

/// Показать файл в Finder или проводнике — выделенным, а не открытым.
#[cfg(target_os = "macos")]
pub fn reveal_file(path: &Path) {
    let _ = Command::new("/usr/bin/open").arg("-R").arg(path).spawn();
}

#[cfg(windows)]
pub fn reveal_file(path: &Path) {
    use std::os::windows::process::CommandExt;

    // У explorer особый разбор командной строки: ключ и путь — один
    // аргумент, а кавычки нужны только вокруг пути. Обычный .arg() взял бы
    // всё в кавычки целиком (там пробел в «Sol Flow»), explorer такого не
    // понимает и молча открывает «Документы» — из-за этого «показать файл»
    // и выглядело как будто ничего не делает.
    let _ = command("explorer")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .spawn();
}

/// Приглушить или вернуть системный звук на время записи.
#[cfg(target_os = "macos")]
pub fn set_muted(muted: bool) {
    let script = if muted {
        "set volume with output muted"
    } else {
        "set volume without output muted"
    };
    let _ = Command::new("/usr/bin/osascript").args(["-e", script]).spawn();
}

/// На Windows громкостью заведует COM-интерфейс IAudioEndpointVolume, ради
/// одной настройки его не поднимаем: сама настройка в интерфейсе скрыта.
#[cfg(windows)]
pub fn set_muted(_muted: bool) {}

/// Версия системы — для отчёта об ошибке.
#[cfg(target_os = "macos")]
pub fn os_version() -> String {
    Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

#[cfg(windows)]
pub fn os_version() -> String {
    command("cmd")
        .args(["/C", "ver"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Экран «Универсальный доступ», без которого на macOS не работает
/// автовставка. На Windows такого разрешения нет, и открывать нечего.
#[cfg(target_os = "macos")]
pub fn open_accessibility_settings() {
    let _ = Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}

#[cfg(windows)]
pub fn open_accessibility_settings() {}

/// Проигрывает готовый WAV-файл, не занимая свой аудиовыход: микрофон в
/// этот момент уже слушает другой поток.
#[cfg(target_os = "macos")]
pub fn play_wav(path: &Path) {
    let _ = Command::new("/usr/bin/afplay").arg(path).spawn();
}

#[cfg(windows)]
pub fn play_wav(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SND_ASYNC — не ждать конца звука: запись уже пошла.
    unsafe {
        PlaySoundW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

/// Название процессора — по нему в каталоге показывается ориентир по
/// скорости, и оно же уходит в отчёт об ошибке.
#[cfg(target_os = "macos")]
pub fn cpu_name() -> String {
    sysctl("machdep.cpu.brand_string")
}

/// В реестре имя процессора лежит готовой строкой: wmic из Windows 11
/// выпилен, а PowerShell ради одной строки поднимается почти секунду.
#[cfg(windows)]
pub fn cpu_name() -> String {
    command("reg")
        .args([
            "query",
            r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "/v",
            "ProcessorNameString",
        ])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .find(|l| l.contains("ProcessorNameString"))
                .and_then(|l| l.split("REG_SZ").nth(1))
                .map(|name| name.trim().to_string())
        })
        .unwrap_or_default()
}

/// Сколько всего оперативной памяти, в байтах. Ноль — не удалось узнать.
#[cfg(target_os = "macos")]
pub fn memory_bytes() -> u64 {
    sysctl("hw.memsize").parse().unwrap_or(0)
}

#[cfg(windows)]
pub fn memory_bytes() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GlobalMemoryStatusEx;
    use windows_sys::Win32::System::SystemInformation::MEMORYSTATUSEX;

    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return 0;
    }
    status.ullTotalPhys
}

#[cfg(target_os = "macos")]
fn sysctl(key: &str) -> String {
    Command::new("/usr/sbin/sysctl")
        .args(["-n", key])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Название системы для писем и подписей в интерфейсе.
pub const OS_NAME: &str = if cfg!(target_os = "macos") { "Mac" } else { "Windows" };
