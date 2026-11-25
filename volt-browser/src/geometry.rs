use serde::{Deserialize, Serialize};

/// A place in a page's viewport, or a distance, in CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// The distance from the left.
    pub x: f64,
    /// The distance from the top.
    pub y: f64,
}

/// The box an element takes in its page's viewport, in CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// The distance of the left edge from the viewport's left.
    pub x: f64,
    /// The distance of the top edge from the viewport's top.
    pub y: f64,
    /// The width.
    pub width: f64,
    /// The height.
    pub height: f64,
}
