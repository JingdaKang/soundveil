//! Which side of the overlay a pointer is on, so that a drag can resize from there. Points are in
//! logical pixels from the top left of the window.

/// The sides of the overlay a drag moves. A corner is two sides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResizeEdges(u8);

impl ResizeEdges {
    pub const NONE: Self = Self(0);
    pub const LEFT: Self = Self(1);
    pub const RIGHT: Self = Self(2);
    pub const BOTTOM: Self = Self(4);
    pub const TOP: Self = Self(8);

    /// The value that stands for these sides, in the interface.
    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 15)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for ResizeEdges {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeZones {
    /// How far in from the window's edge a drag resizes.
    pub band: f32,
    /// How far along an edge still counts as the corner.
    pub corner: f32,
    /// The square at the bottom right, where the visible grip sits.
    pub grip: f32,
}

impl ResizeZones {
    /// The band is narrow enough to leave the overlay's top right buttons clear of it.
    pub const OVERLAY: Self = Self {
        band: 12.0,
        corner: 28.0,
        grip: 32.0,
    };

    /// `NONE` means the point is not on the border, so the click belongs to the content.
    pub fn edges(&self, x: f32, y: f32, width: f32, height: f32) -> ResizeEdges {
        if x < 0.0 || y < 0.0 || x >= width || y >= height {
            return ResizeEdges::NONE;
        }
        if x >= width - self.grip && y >= height - self.grip {
            return ResizeEdges::RIGHT | ResizeEdges::BOTTOM;
        }
        let side = |low, high, band: f32, first, last| {
            if low < band {
                first
            } else if high < band {
                last
            } else {
                ResizeEdges::NONE
            }
        };
        let (from_right, from_bottom) = (width - x, height - y);
        let across = side(
            x,
            from_right,
            self.band,
            ResizeEdges::LEFT,
            ResizeEdges::RIGHT,
        );
        let along = side(
            y,
            from_bottom,
            self.band,
            ResizeEdges::TOP,
            ResizeEdges::BOTTOM,
        );
        if across != ResizeEdges::NONE && along == ResizeEdges::NONE {
            across
                | side(
                    y,
                    from_bottom,
                    self.corner,
                    ResizeEdges::TOP,
                    ResizeEdges::BOTTOM,
                )
        } else if along != ResizeEdges::NONE && across == ResizeEdges::NONE {
            along
                | side(
                    x,
                    from_right,
                    self.corner,
                    ResizeEdges::LEFT,
                    ResizeEdges::RIGHT,
                )
        } else {
            across | along
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ResizeEdges as Edges;

    const ZONES: ResizeZones = ResizeZones {
        band: 12.0,
        corner: 24.0,
        grip: 30.0,
    };

    fn edges(x: f32, y: f32) -> ResizeEdges {
        ZONES.edges(x, y, 800.0, 200.0)
    }

    #[test]
    fn the_middle_belongs_to_the_content() {
        assert_eq!(edges(400.0, 100.0), Edges::NONE);
        assert_eq!(edges(12.0, 100.0), Edges::NONE);
    }

    #[test]
    fn each_edge_resizes_its_own_side() {
        assert_eq!(edges(3.0, 100.0), Edges::LEFT);
        assert_eq!(edges(797.0, 100.0), Edges::RIGHT);
        assert_eq!(edges(400.0, 197.0), Edges::BOTTOM);
        assert_eq!(edges(400.0, 3.0), Edges::TOP);
    }

    #[test]
    fn points_near_a_corner_move_both_sides() {
        assert_eq!(edges(3.0, 180.0), Edges::LEFT | Edges::BOTTOM);
        assert_eq!(edges(20.0, 3.0), Edges::LEFT | Edges::TOP);
        assert_eq!(edges(780.0, 3.0), Edges::RIGHT | Edges::TOP);
        assert_eq!(edges(3.0, 3.0), Edges::LEFT | Edges::TOP);
    }

    // The overlay's buttons sit just inside the top right corner and must stay clickable.
    #[test]
    fn only_the_bottom_right_has_a_square_grip() {
        assert_eq!(edges(780.0, 180.0), Edges::RIGHT | Edges::BOTTOM);
        assert_eq!(edges(780.0, 20.0), Edges::NONE);
        assert_eq!(edges(20.0, 180.0), Edges::NONE);
    }

    #[test]
    fn points_outside_the_window_are_not_edges() {
        assert_eq!(edges(-1.0, 100.0), Edges::NONE);
        assert_eq!(edges(800.0, 100.0), Edges::NONE);
        assert_eq!(edges(400.0, 200.0), Edges::NONE);
    }

    #[test]
    fn the_overlay_buttons_are_clear_of_every_zone() {
        // Their bar starts 13 from the top and right, and is about 160 wide and 28 tall.
        for x in (627..787).step_by(5) {
            for y in (13..41).step_by(4) {
                let edges = ResizeZones::OVERLAY.edges(x as f32, y as f32, 800.0, 156.0);
                assert_eq!(edges, Edges::NONE, "{x},{y}");
            }
        }
    }

    #[test]
    fn sides_survive_the_trip_through_the_interface() {
        let corner = Edges::LEFT | Edges::TOP;
        assert_eq!(Edges::from_bits(corner.bits()), corner);
        assert!(corner.contains(Edges::LEFT));
        assert!(!corner.contains(Edges::RIGHT));
    }
}
