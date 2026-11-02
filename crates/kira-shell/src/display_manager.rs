//! Bounded Niri output management: fixed argv, no shell, and watchdog rollback.
//! Operations are session-scoped; persistent Niri KDL remains user-owned.
use std::process::Command;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

const CONFIRM_SECS: u64 = 15;
const MAX_OUTPUTS: usize = 8;
const MAX_MODES: usize = 80;

#[derive(Debug, Clone)]
struct Desired {
    name: String,
    enabled: bool,
    mode: String,
    scale: f64,
    transform: String,
    x: i32,
    y: i32,
    vrr: bool,
}

#[derive(Debug)]
enum Request {
    Refresh,
    Apply(serde_json::Value),
    Confirm(u64),
    Revert(u64),
}

#[derive(Debug)]
pub struct Event {
    pub state: &'static str,
    pub outputs: Option<serde_json::Value>,
    pub pending_id: Option<u64>,
    pub error: Option<String>,
}

pub struct Model {
    sender: Sender<Request>,
    receiver: Receiver<Event>,
}

impl Model {
    pub fn new() -> Self {
        let (sender, requests) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        thread::spawn(move || run(requests, events));
        Self { sender, receiver }
    }

    pub fn refresh(&self) {
        let _ = self.sender.send(Request::Refresh);
    }

    pub fn apply(&self, value: serde_json::Value) {
        let _ = self.sender.send(Request::Apply(value));
    }

    pub fn confirm(&self, id: u64) {
        let _ = self.sender.send(Request::Confirm(id));
    }

    pub fn revert(&self, id: u64) {
        let _ = self.sender.send(Request::Revert(id));
    }

    pub fn poll(&self) -> Vec<Event> {
        // Never block GTK. Limit messages consumed per frame.
        (0..6)
            .filter_map(|_| self.receiver.try_recv().ok())
            .collect()
    }
}

fn report(
    events: &Sender<Event>,
    state: &'static str,
    pending_id: Option<u64>,
    error: Option<String>,
) {
    let snapshot = query().ok();
    let _ = events.send(Event {
        state,
        outputs: snapshot,
        pending_id,
        error: error.map(|value| value.chars().take(240).collect()),
    });
}

/// The only executable is the fixed Niri CLI; all arguments are bounded and
/// validated against Niri's own snapshot, without an interpolated shell.
fn niri(args: &[&str]) -> Result<String, String> {
    let output = Command::new("niri")
        .arg("msg")
        .args(args)
        .output()
        .map_err(|error| format!("Niri IPC недоступен: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Niri отклонил команду: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(180)
                .collect::<String>()
        ));
    }
    if output.stdout.len() > 256 * 1024 {
        return Err("Ответ Niri слишком большой".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "Некорректный ответ Niri".into())
}

fn query() -> Result<serde_json::Value, String> {
    let text = niri(&["--json", "outputs"])?;
    let source: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "Niri вернул некорректный JSON".to_string())?;
    let map = source.as_object().ok_or("Niri не вернул список дисплеев")?;
    if map.len() > MAX_OUTPUTS {
        return Err("Обнаружено слишком много дисплеев для безопасного редактирования".into());
    }
    let mut outputs = Vec::new();
    for (connector, value) in map {
        if connector.len() > 128 || !value.is_object() {
            return Err("Некорректное описание дисплея".into());
        }
        let modes = value
            .get("modes")
            .and_then(serde_json::Value::as_array)
            .ok_or("Niri не сообщает доступные режимы")?;
        if modes.len() > MAX_MODES {
            return Err("Слишком много режимов дисплея".into());
        }
        let mut out = value.clone();
        out["connector"] = serde_json::Value::String(connector.clone());
        outputs.push(out);
    }
    outputs.sort_by(|a, b| a["connector"].as_str().cmp(&b["connector"].as_str()));
    Ok(serde_json::Value::Array(outputs))
}

fn mode_id(mode: &serde_json::Value) -> Option<String> {
    let width = mode["width"].as_u64()?;
    let height = mode["height"].as_u64()?;
    let refresh = mode["refresh_rate"].as_u64()?;
    if !(320..=16384).contains(&width)
        || !(200..=16384).contains(&height)
        || !(1000..=1_000_000).contains(&refresh)
    {
        return None;
    }
    Some(format!(
        "{width}x{height}@{}.{:03}",
        refresh / 1000,
        refresh % 1000
    ))
}

fn valid_transform(value: &str) -> bool {
    matches!(
        value,
        "normal" | "90" | "180" | "270" | "flipped" | "flipped-90" | "flipped-180" | "flipped-270"
    )
}

fn normalized_transform(value: &str) -> Option<&'static str> {
    match value {
        "Normal" | "normal" => Some("normal"),
        "90" => Some("90"),
        "180" => Some("180"),
        "270" => Some("270"),
        "Flipped" | "flipped" => Some("flipped"),
        "Flipped90" | "flipped-90" => Some("flipped-90"),
        "Flipped180" | "flipped-180" => Some("flipped-180"),
        "Flipped270" | "flipped-270" => Some("flipped-270"),
        _ => None,
    }
}

fn baseline(outputs: &serde_json::Value) -> Result<Vec<Desired>, String> {
    let mut result = Vec::new();
    for output in outputs.as_array().ok_or("Неверные данные дисплеев")? {
        let name = output["connector"]
            .as_str()
            .ok_or("Нет идентификатора дисплея")?;
        let logical = &output["logical"];
        let current = output["current_mode"]
            .as_u64()
            .and_then(|i| output["modes"].as_array()?.get(usize::try_from(i).ok()?));
        let fallback = output["modes"].as_array().and_then(|modes| {
            modes
                .iter()
                .find(|m| m["is_preferred"] == true)
                .or(modes.first())
        });
        let mode = current
            .or(fallback)
            .and_then(mode_id)
            .ok_or("Невозможно определить режим дисплея")?;
        let enabled = !logical.is_null();
        let scale = logical["scale"].as_f64().unwrap_or(1.0);
        let transform = logical["transform"]
            .as_str()
            .and_then(normalized_transform)
            .unwrap_or("normal")
            .to_string();
        result.push(Desired {
            name: name.to_string(),
            enabled,
            mode,
            scale,
            transform,
            x: logical["x"].as_i64().unwrap_or(0) as i32,
            y: logical["y"].as_i64().unwrap_or(0) as i32,
            vrr: output["vrr_enabled"] == true,
        });
    }
    Ok(result)
}

fn validate(
    value: &serde_json::Value,
    current: &serde_json::Value,
) -> Result<Vec<Desired>, String> {
    let requested = value.as_array().ok_or("Ожидался массив дисплеев")?;
    let existing = current.as_array().ok_or("Не удалось прочитать дисплеи")?;
    if requested.is_empty() || requested.len() != existing.len() || requested.len() > MAX_OUTPUTS {
        return Err("Список подключённых дисплеев изменился. Обновите его.".into());
    }
    let mut result = Vec::new();
    for input in requested {
        let name = input["connector"]
            .as_str()
            .ok_or("Нет идентификатора дисплея")?;
        let host = existing
            .iter()
            .find(|out| out["connector"] == name)
            .ok_or("Дисплей был отключён. Обновите список.")?;
        if result.iter().any(|out: &Desired| out.name == name) {
            return Err("Повтор дисплея".into());
        }
        let enabled = input["enabled"]
            .as_bool()
            .ok_or("Некорректное состояние дисплея")?;
        let mode = input["mode"].as_str().ok_or("Не указан режим")?;
        if !host["modes"].as_array().is_some_and(|modes| {
            modes
                .iter()
                .any(|item| mode_id(item).as_deref() == Some(mode))
        }) {
            return Err("Режим не поддерживается дисплеем".into());
        }
        let scale = input["scale"].as_f64().ok_or("Некорректный масштаб")?;
        if !scale.is_finite() || !(0.5..=4.0).contains(&scale) {
            return Err("Масштаб должен быть от 50% до 400%".into());
        }
        let transform = input["transform"].as_str().ok_or("Некорректный поворот")?;
        if !valid_transform(transform) {
            return Err("Неподдерживаемый поворот".into());
        }
        let x = input["x"].as_i64().ok_or("Неверная координата X")?;
        let y = input["y"].as_i64().ok_or("Неверная координата Y")?;
        if !(-32000..=32000).contains(&x) || !(-32000..=32000).contains(&y) {
            return Err("Координаты за пределами рабочего стола".into());
        }
        let vrr = input["vrr"].as_bool().ok_or("Некорректный VRR")?;
        if vrr && host["vrr_supported"] != true {
            return Err("Монитор не поддерживает VRR".into());
        }
        result.push(Desired {
            name: name.to_string(),
            enabled,
            mode: mode.to_string(),
            scale,
            transform: transform.to_string(),
            x: x as i32,
            y: y as i32,
            vrr,
        });
    }
    if !result.iter().any(|out| out.enabled) {
        return Err("Нельзя отключить все дисплеи".into());
    }
    Ok(result)
}

fn command(name: &str, suffix: &[&str]) -> Result<(), String> {
    let mut args = vec!["output", name];
    args.extend_from_slice(suffix);
    niri(&args).map(|_| ())
}

/// Enable first, adjust while active, disable last. On a partial failure the
/// caller replays the original snapshot. Only advertised modes are allowed.
fn execute(desired: &[Desired]) -> Result<(), String> {
    for out in desired.iter().filter(|out| out.enabled) {
        command(&out.name, &["on"])?;
    }
    for out in desired.iter().filter(|out| out.enabled) {
        command(&out.name, &["mode", &out.mode])?;
        command(&out.name, &["scale", &out.scale.to_string()])?;
        command(&out.name, &["transform", &out.transform])?;
        command(
            &out.name,
            &["position", "set", &out.x.to_string(), &out.y.to_string()],
        )?;
        command(&out.name, &["vrr", if out.vrr { "on" } else { "off" }])?;
    }
    for out in desired.iter().filter(|out| !out.enabled) {
        command(&out.name, &["off"])?;
    }
    Ok(())
}

fn revert(before: &[Desired], events: &Sender<Event>) {
    let result = execute(before);
    report(
        events,
        "reverted",
        None,
        result
            .err()
            .map(|error| format!("Не удалось полностью восстановить конфигурацию: {error}")),
    );
}

fn run(requests: Receiver<Request>, events: Sender<Event>) {
    let mut next_id = 0_u64;
    let mut pending: Option<(u64, Vec<Desired>, Instant)> = None;
    loop {
        let received = match pending.as_ref() {
            Some((_, _, deadline)) => {
                requests.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            }
            None => requests.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        let message = match received {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => {
                if let Some((_, before, _)) = pending.take() {
                    revert(&before, &events);
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => {
                if let Some((_, before, _)) = pending.take() {
                    revert(&before, &events);
                }
                break;
            }
        };
        match message {
            Request::Refresh => {
                if pending.is_none() {
                    match query() {
                        Ok(outputs) => {
                            let _ = events.send(Event {
                                state: "ready",
                                outputs: Some(outputs),
                                pending_id: None,
                                error: None,
                            });
                        }
                        Err(error) => report(&events, "error", None, Some(error)),
                    }
                }
            }
            Request::Apply(value) => {
                if pending.is_some() {
                    report(
                        &events,
                        "error",
                        pending.as_ref().map(|p| p.0),
                        Some("Сначала подтвердите или отмените текущую конфигурацию".into()),
                    );
                    continue;
                }
                let result = query().and_then(|outputs| {
                    let target = validate(&value, &outputs)?;
                    let before = baseline(&outputs)?;
                    Ok((target, before))
                });
                match result {
                    Err(error) => report(&events, "error", None, Some(error)),
                    Ok((target, before)) => {
                        if let Err(error) = execute(&target) {
                            revert(&before, &events);
                            report(&events, "error", None, Some(error));
                        } else {
                            next_id = next_id.wrapping_add(1).max(1);
                            let id = next_id;
                            pending = Some((
                                id,
                                before,
                                Instant::now() + Duration::from_secs(CONFIRM_SECS),
                            ));
                            report(&events, "pending", Some(id), None);
                        }
                    }
                }
            }
            Request::Confirm(id) => {
                if pending.as_ref().is_some_and(|p| p.0 == id) {
                    pending.take();
                    report(&events, "confirmed", None, None);
                }
            }
            Request::Revert(id) => {
                if pending.as_ref().is_some_and(|p| p.0 == id) {
                    if let Some((_, before, _)) = pending.take() {
                        revert(&before, &events);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn outputs() -> serde_json::Value {
        json!([
            {"connector":"eDP-1", "logical":{"x":0,"y":0,"scale":1.0,"transform":"Normal"},
             "current_mode":0, "modes":[{"width":1920,"height":1080,"refresh_rate":60000,"is_preferred":true}],
             "vrr_supported":false,"vrr_enabled":false},
            {"connector":"HDMI-A-1", "logical":{"x":1920,"y":0,"scale":1.0,"transform":"Normal"},
             "current_mode":0,"modes":[{"width":2560,"height":1440,"refresh_rate":144000,"is_preferred":true}],
             "vrr_supported":true,"vrr_enabled":false}
        ])
    }

    fn plan() -> serde_json::Value {
        json!([
            {"connector":"eDP-1","enabled":true,"mode":"1920x1080@60.000",
             "scale":1.0,"transform":"normal","x":0,"y":0,"vrr":false},
            {"connector":"HDMI-A-1","enabled":true,"mode":"2560x1440@144.000",
             "scale":1.25,"transform":"90","x":1920,"y":0,"vrr":true}
        ])
    }

    #[test]
    fn mode_refresh_uses_exact_millihertz() {
        assert_eq!(
            mode_id(&json!({"width":2560,"height":1440,"refresh_rate":143912})),
            Some("2560x1440@143.912".into())
        );
    }

    #[test]
    fn validates_only_connected_advertised_modes_and_keeps_a_lit_screen() {
        assert!(validate(&plan(), &outputs()).is_ok());
        let mut off = plan();
        off[0]["enabled"] = json!(false);
        off[1]["enabled"] = json!(false);
        assert!(validate(&off, &outputs()).is_err());
        let mut unknown = plan();
        unknown[0]["mode"] = json!("8000x8000@500.000");
        assert!(validate(&unknown, &outputs()).is_err());
        let mut duplicate = plan();
        duplicate[1]["connector"] = json!("eDP-1");
        assert!(validate(&duplicate, &outputs()).is_err());
        let mut vrr = plan();
        vrr[0]["vrr"] = json!(true);
        assert!(validate(&vrr, &outputs()).is_err());
        let mut injection = plan();
        injection[1]["transform"] = json!("normal; rm -rf /");
        assert!(validate(&injection, &outputs()).is_err());
    }

    #[test]
    fn restores_actual_logical_coordinates_and_transform() {
        let saved = baseline(&outputs()).unwrap();
        assert_eq!(saved[1].x, 1920);
        assert_eq!(saved[0].transform, "normal");
        assert!(saved.iter().all(|item| item.enabled));
    }
}
