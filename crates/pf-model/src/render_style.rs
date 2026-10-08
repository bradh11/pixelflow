//! How effects lay a target's pixels out to draw on: a group's own layout, and the render style
//! and buffer transform an effect can choose (xLights' group "layout" and an effect's
//! "Render Style" and "Transformation").

use serde::{Deserialize, Serialize};

/// The grid size a new group gets, as in xLights.
pub const DEFAULT_GRID_SIZE: u32 = 400;
/// Smallest and largest group grid sizes (xLights' "Max Grid Size" range).
pub const MIN_GRID_SIZE: u32 = 10;
pub const MAX_GRID_SIZE: u32 = 4000;

/// How effects see a target's pixels, chosen per effect (xLights' render styles).
///
/// `Default` uses the target's own layout: a prop's own buffer, a submodel's buffer style, or a
/// group's [`GroupLayout`]. The group styles lay out the group's members; on a prop or submodel
/// they draw as `Default`, and a per-model style draws as the style it names, as xLights does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema",
    schemars(description = "How the effect lays out the pixels (xLights' render styles).")
)]
#[serde(rename_all = "camelCase")]
pub enum RenderStyle {
    #[default]
    Default,
    // Each pixel where it is in the layout, on a grid of up to the group's grid size.
    PerPreview,
    // Every pixel in a row, in order.
    SingleLine,
    // The whole target as one cell.
    AsPixel,
    // Each member is a column, its pixels in order from the bottom up.
    HorizontalPerModel,
    // Each member is a row, its pixels in order from the left.
    VerticalPerModel,
    // The members' own buffers side by side.
    HorizontalStack,
    // The members' own buffers one above the other.
    VerticalStack,
    // The members' own buffers side by side, each stretched to the same size.
    HorizontalStackScaled,
    // The members' own buffers one above the other, each stretched to the same size.
    VerticalStackScaled,
    // The members' own buffers on top of each other, centered.
    OverlayCentered,
    // The members' own buffers on top of each other, each stretched to the largest.
    OverlayScaled,
    // Each member is one cell, in a row.
    SingleLineModelAsPixel,
    // Each member is one cell, where it is in the layout.
    DefaultModelAsPixel,
    // The effect draws on each member separately, on the member's own buffer.
    PerModelDefault,
    // The effect draws on each member separately, each laid out as it is in the layout.
    PerModelPerPreview,
    // The effect draws on each member separately, each as a single line.
    PerModelSingleLine,
}

impl RenderStyle {
    /// Every style, in the order xLights lists them.
    pub const ALL: [RenderStyle; 17] = [
        RenderStyle::Default,
        RenderStyle::PerPreview,
        RenderStyle::SingleLine,
        RenderStyle::AsPixel,
        RenderStyle::HorizontalStack,
        RenderStyle::VerticalStack,
        RenderStyle::HorizontalStackScaled,
        RenderStyle::VerticalStackScaled,
        RenderStyle::HorizontalPerModel,
        RenderStyle::VerticalPerModel,
        RenderStyle::OverlayCentered,
        RenderStyle::OverlayScaled,
        RenderStyle::SingleLineModelAsPixel,
        RenderStyle::DefaultModelAsPixel,
        RenderStyle::PerModelDefault,
        RenderStyle::PerModelPerPreview,
        RenderStyle::PerModelSingleLine,
    ];

    /// xLights' name for the style (`B_CHOICE_BufferStyle`).
    pub fn xlights_name(self) -> &'static str {
        match self {
            RenderStyle::Default => "Default",
            RenderStyle::PerPreview => "Per Preview",
            RenderStyle::SingleLine => "Single Line",
            RenderStyle::AsPixel => "As Pixel",
            RenderStyle::HorizontalPerModel => "Horizontal Per Model",
            RenderStyle::VerticalPerModel => "Vertical Per Model",
            RenderStyle::HorizontalStack => "Horizontal Stack",
            RenderStyle::VerticalStack => "Vertical Stack",
            RenderStyle::HorizontalStackScaled => "Horizontal Stack - Scaled",
            RenderStyle::VerticalStackScaled => "Vertical Stack - Scaled",
            RenderStyle::OverlayCentered => "Overlay - Centered",
            RenderStyle::OverlayScaled => "Overlay - Scaled",
            RenderStyle::SingleLineModelAsPixel => "Single Line Model As A Pixel",
            RenderStyle::DefaultModelAsPixel => "Default Model As A Pixel",
            RenderStyle::PerModelDefault => "Per Model Default",
            RenderStyle::PerModelPerPreview => "Per Model Per Preview",
            RenderStyle::PerModelSingleLine => "Per Model Single Line",
        }
    }

    /// The style xLights calls `name`, if PixelFlow has it. "Per Model Default Deep" is "Per
    /// Model Default": PixelFlow's groups list the props of groups inside them directly.
    pub fn from_xlights(name: &str) -> Option<Self> {
        match name.trim() {
            "" => Some(RenderStyle::Default),
            "Per Model Default Deep" => Some(RenderStyle::PerModelDefault),
            name => Self::ALL.into_iter().find(|s| s.xlights_name() == name),
        }
    }

    /// For a per-model style, the style each member draws with.
    pub fn per_model(self) -> Option<RenderStyle> {
        match self {
            RenderStyle::PerModelDefault => Some(RenderStyle::Default),
            RenderStyle::PerModelPerPreview => Some(RenderStyle::PerPreview),
            RenderStyle::PerModelSingleLine => Some(RenderStyle::SingleLine),
            _ => None,
        }
    }

    /// Whether the style lays out a group's members (on a prop or submodel it draws as the
    /// target's own layout).
    pub fn is_group_only(self) -> bool {
        !matches!(
            self,
            RenderStyle::Default | RenderStyle::PerPreview | RenderStyle::SingleLine | RenderStyle::AsPixel
        ) && self.per_model().is_none()
    }
}

/// How a group lays its pixels out for effects that use its own layout (xLights' group
/// "layout"; the default render style on the group).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema",
    schemars(description = "How effects lay out the group (xLights' layouts).")
)]
#[serde(rename_all = "camelCase")]
pub enum GroupLayout {
    // Each pixel where it is in the layout, on a grid with up to the grid size cells along its
    // longer side, cut down to the group's pixels ("Minimal Grid").
    #[default]
    MinimalGrid,
    // Each pixel where it is in the layout, on a grid covering the whole layout area ("Grid as
    // per preview").
    Grid,
    HorizontalPerModel,
    VerticalPerModel,
    HorizontalStack,
    VerticalStack,
    HorizontalStackScaled,
    VerticalStackScaled,
    SingleLine,
    OverlayCentered,
    OverlayScaled,
    SingleLineModelAsPixel,
    DefaultModelAsPixel,
    PerModelDefault,
}

impl GroupLayout {
    /// Every layout, in the order xLights lists them.
    pub const ALL: [GroupLayout; 14] = [
        GroupLayout::Grid,
        GroupLayout::MinimalGrid,
        GroupLayout::HorizontalStack,
        GroupLayout::VerticalStack,
        GroupLayout::HorizontalStackScaled,
        GroupLayout::VerticalStackScaled,
        GroupLayout::HorizontalPerModel,
        GroupLayout::VerticalPerModel,
        GroupLayout::SingleLine,
        GroupLayout::OverlayCentered,
        GroupLayout::OverlayScaled,
        GroupLayout::SingleLineModelAsPixel,
        GroupLayout::DefaultModelAsPixel,
        GroupLayout::PerModelDefault,
    ];

    /// The render style an effect with the default style draws with on the group (`None`: the
    /// group's grid).
    pub fn style(self) -> Option<RenderStyle> {
        match self {
            GroupLayout::MinimalGrid | GroupLayout::Grid => None,
            GroupLayout::HorizontalPerModel => Some(RenderStyle::HorizontalPerModel),
            GroupLayout::VerticalPerModel => Some(RenderStyle::VerticalPerModel),
            GroupLayout::HorizontalStack => Some(RenderStyle::HorizontalStack),
            GroupLayout::VerticalStack => Some(RenderStyle::VerticalStack),
            GroupLayout::HorizontalStackScaled => Some(RenderStyle::HorizontalStackScaled),
            GroupLayout::VerticalStackScaled => Some(RenderStyle::VerticalStackScaled),
            GroupLayout::SingleLine => Some(RenderStyle::SingleLine),
            GroupLayout::OverlayCentered => Some(RenderStyle::OverlayCentered),
            GroupLayout::OverlayScaled => Some(RenderStyle::OverlayScaled),
            GroupLayout::SingleLineModelAsPixel => Some(RenderStyle::SingleLineModelAsPixel),
            GroupLayout::DefaultModelAsPixel => Some(RenderStyle::DefaultModelAsPixel),
            GroupLayout::PerModelDefault => Some(RenderStyle::PerModelDefault),
        }
    }

    /// The layout an xLights group's `layout` attribute names, if PixelFlow has it.
    pub fn from_xlights(layout: &str) -> Option<Self> {
        match layout.trim() {
            "" | "minimalGrid" => Some(GroupLayout::MinimalGrid),
            "grid" => Some(GroupLayout::Grid),
            "horizontal" => Some(GroupLayout::HorizontalPerModel),
            "vertical" => Some(GroupLayout::VerticalPerModel),
            "perModelDefault" => Some(GroupLayout::PerModelDefault),
            name => Self::ALL
                .into_iter()
                .find(|l| l.style().is_some_and(|s| s.xlights_name() == name)),
        }
    }
}

/// Turns or flips the buffer an effect draws on (xLights' buffer "Transformation").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema",
    schemars(description = "Turns or flips the effect's layout.")
)]
#[serde(rename_all = "camelCase")]
pub enum BufferTransform {
    #[default]
    None,
    RotateCw90,
    RotateCcw90,
    Rotate180,
    FlipVertical,
    FlipHorizontal,
    RotateCw90FlipHorizontal,
    RotateCcw90FlipHorizontal,
}

impl BufferTransform {
    /// xLights' name for the transform (`B_CHOICE_BufferTransform`).
    pub fn from_xlights(name: &str) -> Option<Self> {
        Some(match name.trim() {
            "" | "None" => BufferTransform::None,
            "Rotate CW 90" => BufferTransform::RotateCw90,
            "Rotate CC 90" => BufferTransform::RotateCcw90,
            "Rotate 180" => BufferTransform::Rotate180,
            "Flip Vertical" => BufferTransform::FlipVertical,
            "Flip Horizontal" => BufferTransform::FlipHorizontal,
            "Rotate CW 90 Flip Horizontal" => BufferTransform::RotateCw90FlipHorizontal,
            "Rotate CC 90 Flip Horizontal" => BufferTransform::RotateCcw90FlipHorizontal,
            _ => return None,
        })
    }

    /// Whether the buffer's width and height swap.
    pub fn turns(self) -> bool {
        matches!(
            self,
            BufferTransform::RotateCw90
                | BufferTransform::RotateCcw90
                | BufferTransform::RotateCw90FlipHorizontal
                | BufferTransform::RotateCcw90FlipHorizontal
        )
    }

    /// Where the cell at (`x`, `y`) of a `width` × `height` buffer goes (as xLights'
    /// `Model::ApplyTransform` moves it); the new buffer is `height` × `width` when [`turns`].
    /// On a 2 × 2 grid this moves a position on the unit square.
    ///
    /// [`turns`]: BufferTransform::turns
    pub fn apply(self, x: f64, y: f64, width: f64, height: f64) -> (f64, f64) {
        match self {
            BufferTransform::None => (x, y),
            BufferTransform::Rotate180 => (width - x - 1.0, height - y - 1.0),
            BufferTransform::FlipVertical => (x, height - y - 1.0),
            BufferTransform::FlipHorizontal => (width - x - 1.0, y),
            BufferTransform::RotateCw90 => (height - y - 1.0, x),
            BufferTransform::RotateCcw90 => (y, width - x - 1.0),
            // Turned, then flipped top to bottom in the turned buffer (`width` high).
            BufferTransform::RotateCw90FlipHorizontal => (height - y - 1.0, width - x - 1.0),
            BufferTransform::RotateCcw90FlipHorizontal => (y, x),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xlights_names_round_trip() {
        for style in RenderStyle::ALL {
            assert_eq!(RenderStyle::from_xlights(style.xlights_name()), Some(style));
        }
        assert_eq!(
            RenderStyle::from_xlights("Per Model Default Deep"),
            Some(RenderStyle::PerModelDefault)
        );
        assert_eq!(RenderStyle::from_xlights("Horizontal Per Strand"), None);
    }

    #[test]
    fn group_layouts_read_xlights_values() {
        assert_eq!(
            GroupLayout::from_xlights("minimalGrid"),
            Some(GroupLayout::MinimalGrid)
        );
        assert_eq!(GroupLayout::from_xlights(""), Some(GroupLayout::MinimalGrid));
        assert_eq!(GroupLayout::from_xlights("grid"), Some(GroupLayout::Grid));
        assert_eq!(
            GroupLayout::from_xlights("horizontal"),
            Some(GroupLayout::HorizontalPerModel)
        );
        assert_eq!(
            GroupLayout::from_xlights("vertical"),
            Some(GroupLayout::VerticalPerModel)
        );
        assert_eq!(
            GroupLayout::from_xlights("perModelDefault"),
            Some(GroupLayout::PerModelDefault)
        );
        assert_eq!(
            GroupLayout::from_xlights("Overlay - Scaled"),
            Some(GroupLayout::OverlayScaled)
        );
        assert_eq!(GroupLayout::from_xlights("Vertical Per Model/Strand"), None);
    }

    #[test]
    fn transforms_move_cells_as_xlights_does() {
        // A 4 × 2 buffer; the cell at (1, 0).
        let at = |t: BufferTransform| t.apply(1.0, 0.0, 4.0, 2.0);
        assert_eq!(at(BufferTransform::Rotate180), (2.0, 1.0));
        assert_eq!(at(BufferTransform::FlipVertical), (1.0, 1.0));
        assert_eq!(at(BufferTransform::FlipHorizontal), (2.0, 0.0));
        assert_eq!(at(BufferTransform::RotateCw90), (1.0, 1.0));
        assert_eq!(at(BufferTransform::RotateCcw90), (0.0, 2.0));
        assert_eq!(at(BufferTransform::RotateCw90FlipHorizontal), (1.0, 2.0));
        assert_eq!(at(BufferTransform::RotateCcw90FlipHorizontal), (0.0, 1.0));
    }
}
