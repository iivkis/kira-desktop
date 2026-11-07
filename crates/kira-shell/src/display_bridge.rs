//! Strict translation of trusted Niri output snapshots into reusable kira-ui models.
//! No commands run here. The service validates live output capabilities again on Apply.
use kira_ui::{DisplayMode, DisplayOutput};
use serde_json::Value;

fn mode(value: &Value) -> Option<DisplayMode> {
    let width = value["width"].as_u64()?;
    let height = value["height"].as_u64()?;
    let refresh = value["refresh_rate"].as_u64()?;
    if !(320..=16384).contains(&width)
        || !(200..=16384).contains(&height)
        || !(1000..=1_000_000).contains(&refresh)
    {
        return None;
    }
    Some(DisplayMode {
        id: format!("{width}x{height}@{}.{:03}", refresh / 1000, refresh % 1000),
        label: format!(
            "{width}×{height} · {}.{:03} Hz",
            refresh / 1000,
            refresh % 1000
        ),
    })
}

fn normalized_transform(source: &str) -> Option<&'static str> {
    match source {
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

fn coordinate(logical: &Value, key: &str) -> Result<i32, String> {
    let raw = logical[key]
        .as_i64()
        .ok_or_else(|| format!("Нет координаты {key}"))?;
    let value = i32::try_from(raw).map_err(|_| "Координата выходит за пределы i32")?;
    if !(-32000..=32000).contains(&value) {
        return Err("Координата за пределами Niri".into());
    }
    Ok(value)
}

pub fn decode_outputs(value: &Value) -> Result<Vec<DisplayOutput>, String> {
    let source = value.as_array().ok_or("Нет списка дисплеев")?;
    if source.is_empty() || source.len() > 8 {
        return Err("Неверное количество дисплеев".into());
    }
    let mut views = Vec::new();
    for entry in source {
        let connector = entry["connector"]
            .as_str()
            .ok_or("Не указан коннектор дисплея")?;
        if connector.is_empty() || connector.len() > 128 {
            return Err("Некорректное имя коннектора дисплея".into());
        }
        if views
            .iter()
            .any(|out: &DisplayOutput| out.connector == connector)
        {
            return Err("Повтор коннектора дисплея".into());
        }
        let modes_raw = entry["modes"].as_array().ok_or("Нет режимов дисплея")?;
        if modes_raw.len() > 80 {
            return Err("Слишком много режимов дисплея".into());
        }
        let modes = modes_raw.iter().filter_map(mode).collect::<Vec<_>>();
        if modes.is_empty() {
            return Err(format!(
                "Дисплей {connector} не сообщает поддерживаемых режимов"
            ));
        }
        let preferred = entry["current_mode"]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| modes_raw.get(index))
            .or_else(|| modes_raw.iter().find(|item| item["is_preferred"] == true))
            .or_else(|| modes_raw.first())
            .and_then(mode)
            .ok_or("Не удалось определить текущий режим")?;
        if !modes.iter().any(|item| item.id == preferred.id) {
            return Err("Неподдерживаемый режим текущего дисплея".into());
        }

        let logical = &entry["logical"];
        let enabled = !logical.is_null();
        let (scale, transform, x, y) = if enabled {
            let scale = logical["scale"]
                .as_f64()
                .ok_or("Не указан масштаб дисплея")?;
            if !scale.is_finite() || !(0.5..=4.0).contains(&scale) {
                return Err("Масштаб дисплея не поддерживается".into());
            }
            let transform = logical["transform"]
                .as_str()
                .and_then(normalized_transform)
                .ok_or("Неподдерживаемая ориентация дисплея")?;
            (
                scale,
                transform.to_owned(),
                coordinate(logical, "x")?,
                coordinate(logical, "y")?,
            )
        } else {
            (1.0, "normal".to_owned(), 0, 0)
        };
        let description = [entry["make"].as_str(), entry["model"].as_str()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        views.push(DisplayOutput {
            connector: connector.to_owned(),
            description: if description.is_empty() {
                "Неизвестный монитор".into()
            } else {
                description.chars().take(120).collect()
            },
            enabled,
            modes,
            mode: preferred.id,
            scale,
            transform,
            x,
            y,
            vrr: entry["vrr_enabled"] == true,
            vrr_supported: entry["vrr_supported"] == true,
        });
    }
    Ok(views)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn output() -> Value {
        json!([{
            "connector": "DP-1",
            "make": "Acme", "model": "Panel",
            "modes": [
                {"width": 1920, "height": 1080, "refresh_rate": 60000, "is_preferred": true},
                {"width": 1280, "height": 720, "refresh_rate": 59940}
            ],
            "current_mode": 0,
            "logical": {"x": 0, "y": 300, "scale": 1.25, "transform": "Normal"},
            "vrr_supported": false, "vrr_enabled": false
        }])
    }

    #[test]
    fn decodes_bounded_supported_modes_exactly() {
        let decoded = decode_outputs(&output()).unwrap();
        assert_eq!(decoded[0].mode, "1920x1080@60.000");
        assert_eq!(decoded[0].modes[1].id, "1280x720@59.940");
        assert_eq!(decoded[0].scale, 1.25);
        assert_eq!(decoded[0].y, 300);
    }

    #[test]
    fn unknown_orientation_fails_closed() {
        let mut value = output();
        value[0]["logical"]["transform"] = json!("Mystery");
        assert!(decode_outputs(&value).is_err());
    }

    #[test]
    fn disabled_output_does_not_invent_active_geometry() {
        let mut value = output();
        value[0]["logical"] = Value::Null;
        let decoded = decode_outputs(&value).unwrap();
        assert!(!decoded[0].enabled);
        assert_eq!(decoded[0].scale, 1.0);
    }
}
