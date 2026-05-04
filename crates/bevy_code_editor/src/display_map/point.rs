//! Point types for different coordinate spaces in the display map

use super::Point;

/// A point in buffer coordinate space (raw text)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferPoint(pub Point);

impl BufferPoint {
    pub const ZERO: BufferPoint = BufferPoint(Point::ZERO);

    pub fn new(row: u32, column: u32) -> Self {
        Self(Point::new(row, column))
    }

    pub fn row(&self) -> u32 {
        self.0.row
    }

    pub fn column(&self) -> u32 {
        self.0.column
    }
}

impl From<Point> for BufferPoint {
    fn from(point: Point) -> Self {
        Self(point)
    }
}

impl From<BufferPoint> for Point {
    fn from(point: BufferPoint) -> Self {
        point.0
    }
}

/// A point in fold coordinate space (after hiding folded regions)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FoldPoint(pub Point);

impl FoldPoint {
    pub const ZERO: FoldPoint = FoldPoint(Point::ZERO);

    pub fn new(row: u32, column: u32) -> Self {
        Self(Point::new(row, column))
    }

    pub fn row(&self) -> u32 {
        self.0.row
    }

    pub fn column(&self) -> u32 {
        self.0.column
    }
}

impl From<Point> for FoldPoint {
    fn from(point: Point) -> Self {
        Self(point)
    }
}

impl From<FoldPoint> for Point {
    fn from(point: FoldPoint) -> Self {
        point.0
    }
}

/// A point in wrap coordinate space (after soft wrapping)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WrapPoint(pub Point);

impl WrapPoint {
    pub const ZERO: WrapPoint = WrapPoint(Point::ZERO);

    pub fn new(row: u32, column: u32) -> Self {
        Self(Point::new(row, column))
    }

    pub fn row(&self) -> u32 {
        self.0.row
    }

    pub fn column(&self) -> u32 {
        self.0.column
    }
}

impl From<Point> for WrapPoint {
    fn from(point: Point) -> Self {
        Self(point)
    }
}

impl From<WrapPoint> for Point {
    fn from(point: WrapPoint) -> Self {
        point.0
    }
}

/// A point in display coordinate space (final screen position)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DisplayPoint(pub Point);

impl DisplayPoint {
    pub const ZERO: DisplayPoint = DisplayPoint(Point::ZERO);

    pub fn new(row: u32, column: u32) -> Self {
        Self(Point::new(row, column))
    }

    pub fn row(&self) -> u32 {
        self.0.row
    }

    pub fn column(&self) -> u32 {
        self.0.column
    }
}

impl From<Point> for DisplayPoint {
    fn from(point: Point) -> Self {
        Self(point)
    }
}

impl From<DisplayPoint> for Point {
    fn from(point: DisplayPoint) -> Self {
        point.0
    }
}
