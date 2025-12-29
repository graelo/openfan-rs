//! Thermal curve data - mutable via API
//!
//! Stored in `{data_dir}/thermal_curves.toml`
//!
//! Thermal curves define temperature-to-PWM mappings for dynamic fan control.
//! Linear interpolation is used between defined points.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single point on a thermal curve mapping temperature to PWM.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    /// Temperature in Celsius
    pub temp_c: f32,
    /// PWM percentage (0-100)
    pub pwm: u8,
}

impl CurvePoint {
    /// Create a new curve point.
    pub fn new(temp_c: f32, pwm: u8) -> Self {
        Self { temp_c, pwm }
    }
}

/// A thermal curve defining temperature-to-PWM mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalCurve {
    /// Human-readable curve name
    pub name: String,
    /// Points defining the curve (must be sorted by temperature, ascending)
    pub points: Vec<CurvePoint>,
    /// Optional description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ThermalCurve {
    /// Create a new thermal curve with the given name and points.
    ///
    /// Points should be sorted by temperature in ascending order.
    pub fn new(name: impl Into<String>, points: Vec<CurvePoint>) -> Self {
        Self {
            name: name.into(),
            points,
            description: None,
        }
    }

    /// Create a new thermal curve with description.
    pub fn with_description(
        name: impl Into<String>,
        points: Vec<CurvePoint>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            points,
            description: Some(description.into()),
        }
    }

    /// Interpolate PWM value for a given temperature.
    ///
    /// - Returns the minimum PWM if temperature is below the first point
    /// - Returns the maximum PWM if temperature is above the last point
    /// - Linearly interpolates between surrounding points otherwise
    ///
    /// # Panics
    ///
    /// Panics if the curve has no points (validation should prevent this).
    pub fn interpolate(&self, temp: f32) -> u8 {
        if self.points.is_empty() {
            return 0;
        }

        // Clamp below minimum temperature
        if temp <= self.points[0].temp_c {
            return self.points[0].pwm;
        }

        // Clamp above maximum temperature
        let last = self.points.last().unwrap();
        if temp >= last.temp_c {
            return last.pwm;
        }

        // Find surrounding points and interpolate
        for window in self.points.windows(2) {
            let (p1, p2) = (&window[0], &window[1]);
            if temp >= p1.temp_c && temp <= p2.temp_c {
                let ratio = (temp - p1.temp_c) / (p2.temp_c - p1.temp_c);
                let pwm = p1.pwm as f32 + ratio * (p2.pwm as f32 - p1.pwm as f32);
                return pwm.round().clamp(0.0, 100.0) as u8;
            }
        }

        // Fallback (shouldn't reach here if points are properly sorted)
        last.pwm
    }

    /// Validate the thermal curve.
    ///
    /// Returns Ok if valid, or an error message if invalid.
    pub fn validate(&self) -> Result<(), String> {
        // Must have at least 2 points
        if self.points.len() < 2 {
            return Err("Thermal curve must have at least 2 points".to_string());
        }

        // Check temperature ordering (must be ascending)
        for window in self.points.windows(2) {
            if window[0].temp_c >= window[1].temp_c {
                return Err(format!(
                    "Points must be in ascending temperature order: {} >= {}",
                    window[0].temp_c, window[1].temp_c
                ));
            }
        }

        // Check PWM values
        for point in &self.points {
            if point.pwm > 100 {
                return Err(format!(
                    "PWM value {} exceeds maximum of 100 at temperature {}",
                    point.pwm, point.temp_c
                ));
            }
        }

        // Check temperature range
        for point in &self.points {
            if point.temp_c < -50.0 || point.temp_c > 150.0 {
                return Err(format!(
                    "Temperature {} is outside valid range (-50 to 150)",
                    point.temp_c
                ));
            }
        }

        Ok(())
    }

    /// Sort points by temperature (ascending).
    pub fn sort_points(&mut self) {
        self.points
            .sort_by(|a, b| a.temp_c.partial_cmp(&b.temp_c).unwrap());
    }
}

/// Thermal curve data stored in thermal_curves.toml
///
/// Maps curve names to their definitions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThermalCurveData {
    /// Curve name to curve definition mapping
    #[serde(default)]
    pub curves: HashMap<String, ThermalCurve>,
}

impl ThermalCurveData {
    /// Create default thermal curves.
    pub fn with_defaults() -> Self {
        let mut data = Self::default();

        // Balanced curve
        data.insert(
            "Balanced".to_string(),
            ThermalCurve::with_description(
                "Balanced",
                vec![
                    CurvePoint::new(30.0, 25),
                    CurvePoint::new(50.0, 50),
                    CurvePoint::new(70.0, 80),
                    CurvePoint::new(85.0, 100),
                ],
                "Standard curve for balanced performance",
            ),
        );

        // Silent curve
        data.insert(
            "Silent".to_string(),
            ThermalCurve::with_description(
                "Silent",
                vec![
                    CurvePoint::new(40.0, 20),
                    CurvePoint::new(60.0, 40),
                    CurvePoint::new(80.0, 70),
                    CurvePoint::new(90.0, 100),
                ],
                "Low noise curve for quiet operation",
            ),
        );

        // Aggressive curve
        data.insert(
            "Aggressive".to_string(),
            ThermalCurve::with_description(
                "Aggressive",
                vec![
                    CurvePoint::new(30.0, 40),
                    CurvePoint::new(50.0, 70),
                    CurvePoint::new(65.0, 90),
                    CurvePoint::new(75.0, 100),
                ],
                "High cooling curve for maximum performance",
            ),
        );

        data
    }

    /// Get a curve by name.
    pub fn get(&self, name: &str) -> Option<&ThermalCurve> {
        self.curves.get(name)
    }

    /// Get a mutable reference to a curve by name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut ThermalCurve> {
        self.curves.get_mut(name)
    }

    /// Insert a curve.
    pub fn insert(&mut self, name: String, curve: ThermalCurve) {
        self.curves.insert(name, curve);
    }

    /// Remove a curve by name.
    pub fn remove(&mut self, name: &str) -> Option<ThermalCurve> {
        self.curves.remove(name)
    }

    /// Check if a curve exists.
    pub fn contains(&self, name: &str) -> bool {
        self.curves.contains_key(name)
    }

    /// Get all curve names.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.curves.keys()
    }

    /// Parse ThermalCurveData from TOML string.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Serialize ThermalCurveData to TOML string.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

/// Parse points from CLI format: "30:25,50:50,70:80,85:100"
pub fn parse_points(input: &str) -> Result<Vec<CurvePoint>, String> {
    let mut points = Vec::new();

    for pair in input.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }

        let parts: Vec<&str> = pair.split(':').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid point format '{}': expected 'temp:pwm'",
                pair
            ));
        }

        let temp_c: f32 = parts[0]
            .trim()
            .parse()
            .map_err(|_| format!("Invalid temperature '{}': must be a number", parts[0]))?;

        let pwm: u8 = parts[1]
            .trim()
            .parse()
            .map_err(|_| format!("Invalid PWM '{}': must be 0-100", parts[1]))?;

        if pwm > 100 {
            return Err(format!("PWM value {} exceeds maximum of 100", pwm));
        }

        points.push(CurvePoint::new(temp_c, pwm));
    }

    // Sort by temperature
    points.sort_by(|a, b| a.temp_c.partial_cmp(&b.temp_c).unwrap());

    if points.len() < 2 {
        return Err("At least 2 points are required".to_string());
    }

    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_curve_point_creation() {
        let point = CurvePoint::new(50.0, 60);
        assert_eq!(point.temp_c, 50.0);
        assert_eq!(point.pwm, 60);
    }

    #[test]
    fn test_thermal_curve_creation() {
        let curve = ThermalCurve::new(
            "Test",
            vec![CurvePoint::new(30.0, 25), CurvePoint::new(80.0, 100)],
        );
        assert_eq!(curve.name, "Test");
        assert_eq!(curve.points.len(), 2);
        assert!(curve.description.is_none());

        let curve_with_desc = ThermalCurve::with_description(
            "Test",
            vec![CurvePoint::new(30.0, 25), CurvePoint::new(80.0, 100)],
            "A test curve",
        );
        assert_eq!(curve_with_desc.description, Some("A test curve".to_string()));
    }

    #[test]
    fn test_interpolation_basic() {
        let curve = ThermalCurve::new(
            "Test",
            vec![
                CurvePoint::new(30.0, 20),
                CurvePoint::new(50.0, 50),
                CurvePoint::new(80.0, 100),
            ],
        );

        // Exact points
        assert_eq!(curve.interpolate(30.0), 20);
        assert_eq!(curve.interpolate(50.0), 50);
        assert_eq!(curve.interpolate(80.0), 100);

        // Below minimum
        assert_eq!(curve.interpolate(20.0), 20);

        // Above maximum
        assert_eq!(curve.interpolate(90.0), 100);

        // Midpoint interpolation: (30+50)/2 = 40, (20+50)/2 = 35
        assert_eq!(curve.interpolate(40.0), 35);

        // Another interpolation: 65 is midpoint of 50-80, PWM should be midpoint of 50-100 = 75
        assert_eq!(curve.interpolate(65.0), 75);
    }

    #[test]
    fn test_interpolation_edge_cases() {
        let curve = ThermalCurve::new(
            "Test",
            vec![CurvePoint::new(0.0, 0), CurvePoint::new(100.0, 100)],
        );

        // Linear interpolation
        assert_eq!(curve.interpolate(25.0), 25);
        assert_eq!(curve.interpolate(50.0), 50);
        assert_eq!(curve.interpolate(75.0), 75);
    }

    #[test]
    fn test_interpolation_empty_curve() {
        let curve = ThermalCurve::new("Empty", vec![]);
        assert_eq!(curve.interpolate(50.0), 0);
    }

    #[test]
    fn test_validation_valid_curve() {
        let curve = ThermalCurve::new(
            "Valid",
            vec![
                CurvePoint::new(30.0, 25),
                CurvePoint::new(50.0, 50),
                CurvePoint::new(80.0, 100),
            ],
        );
        assert!(curve.validate().is_ok());
    }

    #[test]
    fn test_validation_too_few_points() {
        let curve = ThermalCurve::new("Invalid", vec![CurvePoint::new(50.0, 50)]);
        let result = curve.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 2 points"));
    }

    #[test]
    fn test_validation_unsorted_temperatures() {
        let curve = ThermalCurve::new(
            "Invalid",
            vec![CurvePoint::new(80.0, 100), CurvePoint::new(30.0, 25)],
        );
        let result = curve.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ascending"));
    }

    #[test]
    fn test_validation_pwm_out_of_range() {
        let curve = ThermalCurve::new(
            "Invalid",
            vec![CurvePoint::new(30.0, 25), CurvePoint::new(80.0, 150)],
        );
        let result = curve.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds maximum"));
    }

    #[test]
    fn test_validation_temperature_out_of_range() {
        let curve = ThermalCurve::new(
            "Invalid",
            vec![CurvePoint::new(-100.0, 25), CurvePoint::new(80.0, 100)],
        );
        let result = curve.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside valid range"));
    }

    #[test]
    fn test_sort_points() {
        let mut curve = ThermalCurve::new(
            "Unsorted",
            vec![
                CurvePoint::new(80.0, 100),
                CurvePoint::new(30.0, 25),
                CurvePoint::new(50.0, 50),
            ],
        );
        curve.sort_points();

        assert_eq!(curve.points[0].temp_c, 30.0);
        assert_eq!(curve.points[1].temp_c, 50.0);
        assert_eq!(curve.points[2].temp_c, 80.0);
    }

    #[test]
    fn test_thermal_curve_data_operations() {
        let mut data = ThermalCurveData::default();

        let curve = ThermalCurve::new(
            "Test",
            vec![CurvePoint::new(30.0, 25), CurvePoint::new(80.0, 100)],
        );
        data.insert("Test".to_string(), curve);

        assert!(data.contains("Test"));
        assert_eq!(data.get("Test").unwrap().points.len(), 2);

        let removed = data.remove("Test");
        assert!(removed.is_some());
        assert!(!data.contains("Test"));
    }

    #[test]
    fn test_default_curves() {
        let data = ThermalCurveData::with_defaults();

        assert!(data.contains("Balanced"));
        assert!(data.contains("Silent"));
        assert!(data.contains("Aggressive"));

        let balanced = data.get("Balanced").unwrap();
        assert!(balanced.validate().is_ok());
        assert_eq!(balanced.points.len(), 4);
    }

    #[test]
    fn test_serialization() {
        let mut data = ThermalCurveData::default();
        data.insert(
            "Test".to_string(),
            ThermalCurve::with_description(
                "Test",
                vec![
                    CurvePoint::new(30.0, 25),
                    CurvePoint::new(50.0, 50),
                    CurvePoint::new(80.0, 100),
                ],
                "A test curve",
            ),
        );

        let toml_str = data.to_toml().unwrap();
        assert!(toml_str.contains("[curves.Test]"));
        assert!(toml_str.contains("name = \"Test\""));
        assert!(toml_str.contains("description = \"A test curve\""));
    }

    #[test]
    fn test_deserialization() {
        let toml_str = r#"
            [curves.Balanced]
            name = "Balanced"
            description = "Standard curve"
            points = [
                { temp_c = 30.0, pwm = 25 },
                { temp_c = 50.0, pwm = 50 },
                { temp_c = 80.0, pwm = 100 },
            ]
        "#;

        let data = ThermalCurveData::from_toml(toml_str).unwrap();
        assert!(data.contains("Balanced"));

        let curve = data.get("Balanced").unwrap();
        assert_eq!(curve.name, "Balanced");
        assert_eq!(curve.description, Some("Standard curve".to_string()));
        assert_eq!(curve.points.len(), 3);
        assert_eq!(curve.points[0].temp_c, 30.0);
        assert_eq!(curve.points[0].pwm, 25);
    }

    #[test]
    fn test_roundtrip() {
        let original = ThermalCurveData::with_defaults();
        let toml_str = original.to_toml().unwrap();
        let restored = ThermalCurveData::from_toml(&toml_str).unwrap();

        assert_eq!(original.curves.len(), restored.curves.len());
        for name in original.names() {
            let orig_curve = original.get(name).unwrap();
            let restored_curve = restored.get(name).unwrap();
            assert_eq!(orig_curve.name, restored_curve.name);
            assert_eq!(orig_curve.points.len(), restored_curve.points.len());
            assert_eq!(orig_curve.description, restored_curve.description);
        }
    }

    #[test]
    fn test_parse_points_valid() {
        let points = parse_points("30:25,50:50,70:80,85:100").unwrap();
        assert_eq!(points.len(), 4);
        assert_eq!(points[0].temp_c, 30.0);
        assert_eq!(points[0].pwm, 25);
        assert_eq!(points[3].temp_c, 85.0);
        assert_eq!(points[3].pwm, 100);
    }

    #[test]
    fn test_parse_points_with_spaces() {
        let points = parse_points("30 : 25 , 50 : 50").unwrap();
        assert_eq!(points.len(), 2);
    }

    #[test]
    fn test_parse_points_unsorted_input() {
        // Input is unsorted but parse_points should sort it
        let points = parse_points("80:100,30:25,50:50").unwrap();
        assert_eq!(points[0].temp_c, 30.0);
        assert_eq!(points[1].temp_c, 50.0);
        assert_eq!(points[2].temp_c, 80.0);
    }

    #[test]
    fn test_parse_points_invalid_format() {
        assert!(parse_points("30-25,50:50").is_err());
        assert!(parse_points("30:25:50").is_err());
    }

    #[test]
    fn test_parse_points_invalid_temperature() {
        assert!(parse_points("abc:25,50:50").is_err());
    }

    #[test]
    fn test_parse_points_invalid_pwm() {
        assert!(parse_points("30:abc,50:50").is_err());
        assert!(parse_points("30:150,50:50").is_err()); // PWM > 100
    }

    #[test]
    fn test_parse_points_too_few() {
        assert!(parse_points("30:25").is_err());
    }
}
