use rust_decimal::Decimal;

use crate::ConfigError;

pub fn grid_prices(min: f64, max: f64, step: f64) -> Result<Vec<Decimal>, ConfigError> {
    if step <= 0.0 || min >= max {
        return Err(ConfigError::Validation("invalid grid bounds".into()));
    }
    let mut out = Vec::new();
    let mut v = min;
    while v <= max + 1e-9 {
        out.push(Decimal::try_from(v).map_err(|_| {
            ConfigError::Validation(format!("grid value not decimal: {v}"))
        })?);
        v += step;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_99_levels() {
        let g = grid_prices(0.01, 0.99, 0.01).unwrap();
        assert_eq!(g.len(), 99);
    }
}
