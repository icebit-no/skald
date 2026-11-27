//! Drawing in the pill's own points on top of GPUI's window primitives, with one alpha over everything
//! drawn, for a state that is fading.

use std::sync::Arc;

use gpui_kit::{
    App, BoxShadow, Corners, Font, FontFeatures, FontWeight, Hsla, PathBuilder, PathStyle, Pixels,
    Point, RenderImage, Rgba, SharedString, StrokeOptions, TextAlign, TextRun, Window, bounds,
    fill, linear_color_stop, linear_gradient, point, px, size,
};

pub(crate) struct Painter<'a> {
    pub window: &'a mut Window,
    pub cx: &'a mut App,
    /// The pill's top left corner in the window.
    origin: Point<Pixels>,
    pub alpha: f32,
}

/// Text to draw: a family, a size, a weight and a colour.
#[derive(Clone, Copy)]
pub(crate) struct Type {
    pub family: &'static str,
    pub size: f32,
    pub weight: FontWeight,
    /// Lining figures of one width, for the time.
    pub figures: bool,
}

impl<'a> Painter<'a> {
    pub fn new(window: &'a mut Window, cx: &'a mut App, origin: Point<Pixels>) -> Self {
        Self {
            window,
            cx,
            origin,
            alpha: 1.0,
        }
    }

    fn at(&self, x: f32, y: f32) -> Point<Pixels> {
        point(self.origin.x + px(x), self.origin.y + px(y))
    }

    fn color(&self, color: Rgba) -> Hsla {
        let mut faded = color;
        faded.a *= self.alpha;
        faded.into()
    }

    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: Rgba) {
        self.rounded(x, y, width, height, 0.0, color);
    }

    pub fn rounded(&mut self, x: f32, y: f32, width: f32, height: f32, radius: f32, color: Rgba) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let area = bounds(self.at(x, y), size(px(width), px(height)));
        let quad = fill(area, self.color(color)).corner_radii(px(radius));
        self.window.paint_quad(quad);
    }

    pub fn circle(&mut self, x: f32, y: f32, radius: f32, color: Rgba) {
        self.rounded(
            x - radius,
            y - radius,
            2.0 * radius,
            2.0 * radius,
            radius,
            color,
        );
    }

    /// A rectangle shading from `from` at its left to `to` at its right.
    pub fn shade_across(&mut self, x: f32, y: f32, width: f32, height: f32, from: Rgba, to: Rgba) {
        let area = bounds(self.at(x, y), size(px(width), px(height)));
        let shade = linear_gradient(
            90.0,
            linear_color_stop(self.color(from), 0.0),
            linear_color_stop(self.color(to), 1.0),
        );
        self.window.paint_quad(fill(area, shade));
    }

    /// A rounded rectangle with a border inside its edge.
    pub fn framed(
        &mut self,
        (x, y, width, height): (f32, f32, f32, f32),
        radius: f32,
        background: Rgba,
        border: (f32, Rgba),
    ) {
        let area = bounds(self.at(x, y), size(px(width), px(height)));
        let quad = fill(area, self.color(background))
            .corner_radii(px(radius))
            .border_widths(px(border.0))
            .border_color(self.color(border.1));
        self.window.paint_quad(quad);
    }

    pub fn shadows(
        &mut self,
        (x, y, width, height): (f32, f32, f32, f32),
        radius: f32,
        shadows: &[(f32, f32, Rgba)],
    ) {
        let area = bounds(self.at(x, y), size(px(width), px(height)));
        let shadows: Vec<BoxShadow> = shadows
            .iter()
            .map(|&(blur, drop, color)| BoxShadow {
                color: self.color(color),
                offset: point(px(0.0), px(drop)),
                blur_radius: px(blur),
                spread_radius: px(0.0),
                inset: false,
            })
            .collect();
        self.window
            .paint_drop_shadows(area, Corners::all(px(radius)), &shadows);
    }

    /// Open polylines with butt ends and mitred joins.
    pub fn stroke(&mut self, lines: &[&[(f32, f32)]], width: f32, offset: (f32, f32), color: Rgba) {
        let options = StrokeOptions::default()
            .with_line_width(width)
            .with_miter_limit(6.0);
        let mut path = PathBuilder::stroke(px(width)).with_style(PathStyle::Stroke(options));
        for line in lines {
            let Some(&(x0, y0)) = line.first() else {
                continue;
            };
            path.move_to(self.at(x0 + offset.0, y0 + offset.1));
            for &(x, y) in &line[1..] {
                path.line_to(self.at(x + offset.0, y + offset.1));
            }
        }
        if let Ok(path) = path.build() {
            self.window.paint_path(path, self.color(color));
        }
    }

    /// Closed shapes, each filled.
    pub fn fill(&mut self, shapes: &[&[(f32, f32)]], color: Rgba) {
        for shape in shapes {
            let Some(&(x0, y0)) = shape.first() else {
                continue;
            };
            let mut path = PathBuilder::fill();
            path.move_to(self.at(x0, y0));
            for &(x, y) in &shape[1..] {
                path.line_to(self.at(x, y));
            }
            path.close();
            if let Ok(path) = path.build() {
                self.window.paint_path(path, self.color(color));
            }
        }
    }

    /// A closed shape filled with a shade from `top` to `bottom` over its height.
    pub fn fill_shaded(&mut self, shape: &[(f32, f32)], top: Rgba, bottom: Rgba) {
        let Some(&(x0, y0)) = shape.first() else {
            return;
        };
        let mut path = PathBuilder::fill();
        path.move_to(self.at(x0, y0));
        for &(x, y) in &shape[1..] {
            path.line_to(self.at(x, y));
        }
        path.close();
        let shade = linear_gradient(
            180.0,
            linear_color_stop(self.color(top), 0.0),
            linear_color_stop(self.color(bottom), 1.0),
        );
        if let Ok(path) = path.build() {
            self.window.paint_path(path, shade);
        }
    }

    pub fn image(&mut self, (x, y, width, height): (f32, f32, f32, f32), image: Arc<RenderImage>) {
        let area = bounds(self.at(x, y), size(px(width), px(height)));
        // An image the atlas cannot take is left out; the rest of the pill still draws.
        let _ = self
            .window
            .paint_image(area, area, Corners::default(), image, 0, false);
    }

    /// Draws only what falls between `left` and `right` while `draw` runs.
    pub fn clip_across(&mut self, left: f32, right: f32, draw: impl FnOnce(&mut Painter<'_>)) {
        if right <= left {
            return;
        }
        let area = bounds(self.at(left, -1000.0), size(px(right - left), px(3000.0)));
        let mask = gpui_kit::ContentMask { bounds: area };
        // The window is borrowed by the mask's scope, so the painter is rebuilt inside it.
        let (origin, alpha) = (self.origin, self.alpha);
        let cx = &mut *self.cx;
        self.window.with_content_mask(Some(mask), |window| {
            let mut inner = Painter {
                window,
                cx,
                origin,
                alpha,
            };
            draw(&mut inner);
        });
    }

    fn font(look: Type) -> Font {
        let mut font = gpui_kit::font(look.family);
        font.weight = look.weight;
        if look.figures {
            font.features = FontFeatures(Arc::new(vec![("lnum".into(), 1), ("tnum".into(), 1)]));
        }
        font
    }

    pub fn measure(&mut self, text: &str, look: Type) -> f32 {
        self.shape(text, look, Rgba::default()).width.as_f32()
    }

    fn shape(&mut self, text: &str, look: Type, color: Rgba) -> gpui_kit::ShapedLine {
        let run = TextRun {
            len: text.len(),
            font: Self::font(look),
            color: self.color(color),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        self.window.text_system().shape_line(
            SharedString::from(text.to_owned()),
            px(look.size),
            &[run],
            None,
        )
    }

    /// Text with its baseline at `baseline`, starting at `x`. Returns its width.
    pub fn text(&mut self, text: &str, look: Type, x: f32, baseline: f32, color: Rgba) -> f32 {
        let line = self.shape(text, look, color);
        let top = baseline - line.ascent.as_f32();
        let height = line.ascent + line.descent;
        let _ = line.paint(
            self.at(x, top),
            height,
            TextAlign::Left,
            None,
            self.window,
            self.cx,
        );
        line.width.as_f32()
    }
}
