fn shift_decimal(value: &str, places: i32) -> Option<String> {
    let value = value.trim().strip_prefix('+').unwrap_or(value.trim());
    let (mantissa, exponent) = match value.find(['e', 'E']) {
        Some(index) => (&value[..index], value[index + 1..].parse::<i32>().ok()?),
        None => (value, 0),
    };
    let decimal = mantissa.find('.').unwrap_or(mantissa.len());
    let digits = mantissa.replace('.', "");
    if digits.is_empty()
        || !digits.bytes().all(|v| v.is_ascii_digit())
        || mantissa.bytes().filter(|v| *v == b'.').count() > 1
    {
        return None;
    }
    let point = i32::try_from(decimal)
        .ok()?
        .checked_add(exponent)?
        .checked_add(places)?;
    if !(-1000..=1000).contains(&point) {
        return None;
    }
    let result = if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if point as usize >= digits.len() {
        format!("{}{}", digits, "0".repeat(point as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    };
    let trimmed = result.trim_start_matches('0');
    Some(if trimmed.is_empty() {
        "0".into()
    } else if trimmed.starts_with('.') {
        format!("0{trimmed}")
    } else {
        trimmed.into()
    })
}

pub(crate) fn fraction(percentage: &str) -> Option<f64> {
    let parsed = percentage.trim().parse::<f64>().ok()?;
    if !(parsed > 0.0 && parsed <= 100.0) {
        return None;
    }
    Some(
        shift_decimal(percentage, -2)
            .and_then(|v| v.parse().ok())
            .unwrap_or(parsed / 100.0),
    )
}

pub(crate) fn percentage(fraction: f64) -> String {
    shift_decimal(&fraction.to_string(), 2).unwrap_or_else(|| (fraction * 100.0).to_string())
}
