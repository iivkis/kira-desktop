//! Ownership-safe persistence for Niri output configuration.
//! Kira owns exactly one generated include. Other KDL, Nix files, and manual
//! edits are never rewritten. Unknown source structure fails closed.
use std::collections::HashSet;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::display_manager::Desired;

const MANAGED_NAME: &str = "kira-managed-outputs.kdl";
const RECEIPT_NAME: &str = "kira-managed-outputs.receipt";
const INCLUDE_LINE: &str = "include \"kira-managed-outputs.kdl\"";
const MAX_SOURCE: u64 = 1_048_576;

/// Determine active user config path. A custom --config override not exported
/// in NIRI_CONFIG cannot be detected, and must be handled manually.
fn root() -> Result<PathBuf, String> {
    if let Ok(value) = env::var("NIRI_CONFIG") {
        if !value.is_empty() {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err("NIRI_CONFIG должен содержать абсолютный путь".into());
            }
            return Ok(path);
        }
    }
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .ok_or("Не найден каталог конфигурации пользователя")?;
    Ok(config.join("niri/config.kdl"))
}

fn read_regular(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|e| format!("Невозможно прочитать {}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_SOURCE {
        return Err(format!("{} не является обычным доступным файлом: Nix-managed и символьные ссылки не изменяются автоматически", path.display()));
    }
    fs::read_to_string(path).map_err(|e| format!("Невозможно прочитать {}: {e}", path.display()))
}

/// Read-only traversal can follow Nix-store symlinks. Write targets never do.
fn read_source(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("Невозможно прочитать {}: {e}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE {
        return Err("Небезопасный размер или тип исходного KDL".into());
    }
    fs::read_to_string(path).map_err(|e| format!("Ошибка чтения: {e}"))
}

fn managed_dir() -> Result<PathBuf, String> {
    let base = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or("Не удалось определить XDG_CONFIG_HOME или HOME")?;
    Ok(base.join("niri"))
}

fn resolve_include(parent: &Path, raw: &str) -> Result<PathBuf, String> {
    if raw.starts_with("~/") {
        let home = env::var_os("HOME").ok_or("HOME не определён")?;
        return Ok(PathBuf::from(home).join(&raw[2..]));
    }
    let target = PathBuf::from(raw);
    Ok(if target.is_absolute() { target } else { parent.join(target) })
}

/// Inspect only simple, explicit top-level include/output lines. This check is
/// deliberately conservative: a complex or unknown output definition requires
/// manual migration, never a blind rewrite.
fn inspect(path: &Path, managed: &Path, visited: &mut HashSet<PathBuf>, depth: usize)
    -> Result<usize, String>
{
    if depth > 8 {
        return Err("Слишком глубокая цепочка include".into());
    }
    let canonical = fs::canonicalize(path)
        .map_err(|e| format!("Не удалось прочитать include {}: {e}", path.display()))?;
    if !visited.insert(canonical) {
        return Err("Вложенные include образуют цикл или повтор".into());
    }
    let content = read_source(path)?;
    let parent = path.parent().ok_or("Нет родительского каталога")?;
    let mut managed_count = 0;
    // Only simple top-level includes and output declarations are eligible for
    // automatic ownership handover; ambiguous KDL must be migrated manually.
    for line in content.lines() {
        let trim = line.trim();
        if trim.is_empty() || trim.starts_with("//") || trim.starts_with("/-") {
            continue;
        }
        if trim.starts_with("output ") || trim.starts_with("output\t") || trim.starts_with("output\"") {
            return Err(format!("В {} есть собственный блок output. Перенесите управление дисплеями в файл Kira или удалите конфликтующий блок вручную", path.display()));
        }
        if let Some(rest) = trim.strip_prefix("include ").or_else(|| trim.strip_prefix("include\t")) {
            let (optional, tail) = if let Some(tail) = rest.strip_prefix("optional=true ") {
                (true, tail)
            } else if let Some(tail) = rest.strip_prefix("optional=true\t") {
                (true, tail)
            } else {
                (false, rest)
            };
            let tail = tail.trim();
            let quoted = tail.strip_prefix('"').ok_or("Сложный include требует ручной настройки")?;
            let (name, extra) = quoted.split_once('"')
                .ok_or("Некорректный путь include")?;
            if name.is_empty() || name.contains('\\') || name.contains('\n') ||
                !extra.trim().trim_end_matches(';').trim().is_empty() {
                return Err("Сложный include требует ручной настройки".into());
            }
            let target = resolve_include(parent, name)?;
            if target == managed {
                managed_count += 1;
                continue;
            }
            if !target.exists() && optional {
                continue;
            }
            managed_count += inspect(&target, managed, visited, depth + 1)?;
        }
    }
    Ok(managed_count)
}

fn generated(desired: &[Desired]) -> Result<String, String> {
    if desired.is_empty() || desired.len() > 8 || !desired.iter().any(|d| d.enabled) {
        return Err("Некорректная конфигурация дисплеев".into());
    }
    let mut text = String::from("// KIRA MANAGED: edit via Kira Settings; do not edit manually.\n// To take back control, remove this include from config.kdl.\n");
    let mut names = HashSet::new();
    for out in desired {
        if !names.insert(&out.name) {
            return Err("Повтор определения одного дисплея".into());
        }
        if out.name.is_empty() || out.name.len() > 128 ||
            !out.name.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.')) ||
            !out.mode.bytes().all(|c| c.is_ascii_digit() || matches!(c, b'x' | b'@' | b'.')) ||
            !out.scale.is_finite() || !(0.5..=4.0).contains(&out.scale) ||
            !matches!(out.transform.as_str(), "normal" | "90" | "180" | "270" | "flipped" |
                "flipped-90" | "flipped-180" | "flipped-270") {
            return Err("Отказано: неподдерживаемое значение для KDL".into());
        }
        text.push_str(&format!("output \"{}\" {{\n", out.name));
        if !out.enabled {
            text.push_str("    off\n");
        } else {
            text.push_str(&format!(
                "    mode \"{}\"\n    scale {}\n    transform \"{}\"\n    position x={} y={}\n",
                out.mode, out.scale, out.transform, out.x, out.y
            ));
            if out.vrr {
                text.push_str("    variable-refresh-rate\n");
            }
        }
        text.push_str("}\n\n");
    }
    Ok(text)
}

/// Atomic replace in the same directory. Not even a pre-existing symlink is
/// overwritten. Preserve permissions of an existing user-owned KDL.
fn atomic(path: &Path, contents: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() =>
            return Err("Целевой файл не является обычным файлом".into()),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound =>
            return Err(format!("Невозможно проверить файл: {error}")),
        _ => {}
    }
    let parent = path.parent().ok_or("Нет каталога для сохранения")?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_nanos();
    let temp = parent.join(format!(".kira-{}-{stamp}-{}.tmp",
        std::process::id(), path.file_name().and_then(|n| n.to_str()).unwrap_or("config")));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)
        .map_err(|e| format!("Не удалось создать временный файл: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = if path.exists() {
            fs::metadata(path).map_err(|e| e.to_string())?.permissions()
        } else {
            fs::Permissions::from_mode(0o600)
        };
        if let Err(e) = file.set_permissions(permissions) {
            let _ = fs::remove_file(&temp);
            return Err(format!("Не удалось назначить права: {e}"));
        }
    }
    let result = file.write_all(contents.as_bytes()).and_then(|_| file.sync_all());
    drop(file);
    if let Err(e) = result {
        let _ = fs::remove_file(&temp);
        return Err(format!("Ошибка записи файла: {e}"));
    }
    if let Err(e) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(format!("Не удалось атомарно заменить файл: {e}"));
    }
    Ok(())
}

fn validate_config(path: &Path) -> Result<(), String> {
    let result = std::process::Command::new("niri").arg("validate")
        .arg("--config").arg(path).output()
        .map_err(|e| format!("Niri validate недоступен: {e}"))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(format!("Niri отверг конфигурацию: {}",
            String::from_utf8_lossy(&result.stderr).chars().take(240).collect::<String>()))
    }
}

fn rollback_file(path: &Path, previous: Option<&str>) {
    if let Some(value) = previous {
        let _ = atomic(path, value);
    } else {
        let _ = fs::remove_file(path);
    }
}

/// The GUI writes only this opt-in file. The first successful save appends one
/// explicit include *only* if config.kdl is a regular user-owned file and has
/// no output blocks. Nix-generated roots are never changed.
pub fn save(desired: &[Desired]) -> Result<(), String> {
    let config = root()?;
    let base = read_source(&config)?;
    let dir = managed_dir()?;
    let fragment = dir.join(MANAGED_NAME);
    let receipt = dir.join(RECEIPT_NAME);
    let mut seen = HashSet::new();
    let count = inspect(&config, &fragment, &mut seen, 0)?;
    if count > 1 {
        return Err("Управляемый include Kira подключён несколько раз".into());
    }
    // An unmanaged fragment or a changed file since the last GUI save is a
    // real conflict, not an invitation to overwrite someone's changes.
    let existing = |path: &Path| -> Result<Option<String>, String> {
        match fs::symlink_metadata(path) {
            Ok(_) => read_regular(path).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("Не удалось проверить файл: {error}")),
        }
    };
    let previous_fragment = existing(&fragment)?;
    let previous_receipt = existing(&receipt)?;
    if previous_fragment.as_deref() != previous_receipt.as_deref() {
        return Err("Конфликт версий: файл дисплеев изменён вручную или не принадлежит Kira. Сначала разрешите конфликт в KDL".into());
    }
    let new_fragment = generated(desired)?;
    let new_root = if count == 0 {
        // A Nix-owned config is commonly a symlink into /nix/store.
        // It can opt in by adding an include in the Nix declaration.
        let metadata = fs::symlink_metadata(&config)
            .map_err(|e| format!("Невозможно проверить владельца config.kdl: {e}"))?;
        if !metadata.file_type().is_file() {
            return Err(format!("config.kdl управляется извне. Добавьте в Nix/Home Manager: include \"{}\"; затем повторите сохранение", fragment.display()));
        }
        let reference = if config.parent() == Some(dir.as_path()) {
            INCLUDE_LINE.to_string()
        } else {
            format!("include \"{}\"", fragment.display())
        };
        if reference.contains('\\') || reference.matches('"').count() != 2 {
            return Err("Путь к управляемому слою требует ручного подключения".into());
        }
        format!("{}\n// KIRA MANAGED OUTPUTS (explicit opt-in)\n{}\n",
            base.trim_end_matches('\n'), reference)
    } else {
        base.clone()
    };
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Не удалось создать каталог Kira: {e}"))?;
    }
    if read_source(&config)? != base {
        return Err("config.kdl изменён во время сохранения. Повторите действие.".into());
    }
    atomic(&fragment, &new_fragment)?;
    if count == 0 {
        if let Err(e) = atomic(&config, &new_root) {
            rollback_file(&fragment, previous_fragment.as_deref());
            return Err(e);
        }
    }
    if let Err(error) = validate_config(&config) {
        rollback_file(&fragment, previous_fragment.as_deref());
        if count == 0 { rollback_file(&config, Some(&base)); }
        return Err(error);
    }
    // Receipt is the previous exact managed content, not a mutable timestamp.
    // On a partial disk write failure, the next attempt detects a conflict.
    atomic(&receipt, &new_fragment)?;
    Ok(())
}

pub fn status() -> &'static str {
    let Ok(config) = root() else { return "manual"; };
    let Ok(dir) = managed_dir() else { return "manual"; };
    let fragment = dir.join(MANAGED_NAME);
    let receipt = dir.join(RECEIPT_NAME);
    let Ok(base) = read_source(&config) else { return "manual"; };
    if !base.lines().any(|line| line.trim().starts_with("include ") &&
        line.contains(MANAGED_NAME)) {
        return "manual";
    }
    match (read_regular(&fragment), read_regular(&receipt)) {
        (Ok(a), Ok(b)) if a == b => "managed",
        (Err(_), Err(_)) => "uninitialized",
        _ => "conflict",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Desired> {
        vec![Desired { name: "DP-1".into(), enabled: true,
            mode: "2560x1440@144.000".into(), scale: 1.25,
            transform: "normal".into(), x: 0, y: 0, vrr: true }]
    }

    #[test]
    fn generated_kdl_is_valid_shape_and_inert_injection_is_rejected() {
        let text = generated(&sample()).unwrap();
        assert!(text.contains("output \"DP-1\" {"));
        assert!(text.contains("position x=0 y=0"));
        assert!(text.contains("variable-refresh-rate"));
        let mut hostile = sample();
        hostile[0].name = "DP-1\"; spawn \"sh".into();
        assert!(generated(&hostile).is_err());
    }

    #[test]
    fn detects_conflicting_declarations_and_supports_optional_nix_include() {
        let temp = std::env::temp_dir().join(format!(
            "kira-kdl-test-{}-{}", std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&temp).unwrap();
        let root = temp.join("config.kdl");
        let managed = temp.join(MANAGED_NAME);
        fs::write(&root, format!("include optional=true \"{}\"\n", managed.display())).unwrap();
        let mut seen = HashSet::new();
        assert_eq!(inspect(&root, &managed, &mut seen, 0).unwrap(), 1);
        fs::write(&root, format!("output \"DP-1\" {{ off }}\ninclude optional=true \"{}\"\n", managed.display())).unwrap();
        let mut seen = HashSet::new();
        assert!(inspect(&root, &managed, &mut seen, 0).is_err());
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn disallows_managed_file_overwrites_through_symlinks() {
        let temp = std::env::temp_dir().join(format!(
            "kira-atomic-test-{}-{}", std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&temp).unwrap();
        let target = temp.join("source.kdl");
        let link = temp.join(MANAGED_NAME);
        fs::write(&target, "example").unwrap();
        #[cfg(unix)] {
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert!(atomic(&link, "unsafe").is_err());
            assert_eq!(fs::read_to_string(&target).unwrap(), "example");
        }
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn disabled_outputs_are_explicit() {
        let mut desired = sample();
        desired[0].enabled = false;
        let mut second = sample()[0].clone();
        second.name = "HDMI-A-1".into();
        desired.push(second);
        assert!(generated(&desired).is_ok());
        assert!(generated(&desired).unwrap().contains("    off\n"));
    }
}
