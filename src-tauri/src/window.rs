use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// Monitor rectangles and stored positions use physical pixels (including negative coordinates).
#[derive(Clone, Copy)]
pub struct DisplayRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub fn restore_position(
    saved: Option<Position>,
    size: (u32, u32),
    displays: &[DisplayRect],
    primary: DisplayRect,
) -> Position {
    if let Some(p) = saved {
        if displays.iter().any(|d| {
            i64::from(p.x) >= i64::from(d.x)
                && i64::from(p.y) >= i64::from(d.y)
                && i64::from(p.x) + i64::from(size.0) <= i64::from(d.x) + i64::from(d.width)
                && i64::from(p.y) + i64::from(size.1) <= i64::from(d.y) + i64::from(d.height)
        }) {
            return p;
        }
    }
    Position {
        x: (i64::from(primary.x) + (i64::from(primary.width) - i64::from(size.0) - 32).max(0))
            as i32,
        y: (i64::from(primary.y) + (i64::from(primary.height) - i64::from(size.1) - 64).max(0))
            as i32,
    }
}

pub fn aspect_size(width: f64, canvas: (u32, u32)) -> Size {
    let ratio = f64::from(canvas.1) / f64::from(canvas.0);
    // Bound both axes; ultra-tall skins cannot create an inaccessible giant window.
    let width = width.clamp(64.0, 800.0).min(800.0 / ratio);
    Size {
        width,
        height: width * ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_valid_negative_monitor_position_and_recovers_missing_display() {
        let main = DisplayRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let other = DisplayRect { x: -1920, ..main };
        let p = Position { x: -400, y: 100 };
        assert_eq!(
            restore_position(Some(p), (240, 240), &[main, other], main),
            p
        );
        let restored = restore_position(Some(p), (240, 240), &[main], main);
        assert!(restored.x >= 0 && restored.x + 240 <= 1920);
        assert!(restored.y >= 0 && restored.y + 240 <= 1080);
    }
    #[test]
    fn avoids_overflow_and_preserves_aspect_ratio() {
        let main = DisplayRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let p = Position {
            x: i32::MAX,
            y: i32::MIN,
        };
        assert_ne!(restore_position(Some(p), (240, 240), &[main], main), p);
        assert_eq!(
            aspect_size(240.0, (200, 100)),
            Size {
                width: 240.0,
                height: 120.0
            }
        );
        assert!(aspect_size(800.0, (1, 100)).height <= 800.0);
    }
}
