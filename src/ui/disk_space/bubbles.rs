//! The Bubbles tab: the largest files as packed circles, area by size and
//! color by type. Hover shows a file, click selects it for the detail strip.

use std::rc::Rc;

use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::packing::{self, Circle};
use super::parts::{self, kind_color};
use super::{CopyPath, DiskSpacePage, MoveToTrash, OpenLocation};
use crate::system::file_types::Kind;
use crate::system::format;

/// How many files get a bubble. More than this and the tail is just specks.
pub const BUBBLES: usize = 300;
/// Space between the pile and the panel's edge.
const INSET: f32 = 12.;
/// Bubbles at least this big get their name and size written inside.
const LABEL_RADIUS: f32 = 34.;
const LABEL_FONT: f32 = 12.;
const DETAIL_FONT: f32 = 11.;
/// Bubbles smaller than this aren't drawn or hit.
const MIN_RADIUS: f32 = 1.;

/// One file's bubble, before it's fitted to the panel.
pub struct Bubble {
    /// An index into the Files table's files.
    pub file: usize,
    pub circle: Circle,
    pub kind: Kind,
    pub name: SharedString,
    pub size: u64,
}

/// A bubble placed on screen.
#[derive(Clone, Copy)]
struct Placed {
    ix: usize,
    center: Point<Pixels>,
    radius: Pixels,
}

impl DiskSpacePage {
    /// The packed bubbles for the files the filter lets through, worked out
    /// once and kept until the files or the filter change.
    fn bubble_layout(&mut self, cx: &App) -> Rc<Vec<Bubble>> {
        if let Some(layout) = &self.bubbles {
            return layout.clone();
        }
        let table = self.files.read(cx).delegate();
        let files: Vec<_> = table.largest_visible(BUBBLES).into_iter().filter_map(|ix| Some((ix, table.file(ix)?))).collect();
        let radii: Vec<f64> = files.iter().map(|(_, file)| (file.size as f64).sqrt()).collect();
        let layout: Vec<Bubble> = packing::pack(&radii)
            .into_iter()
            .zip(files)
            .map(|(circle, (ix, file))| Bubble { file: ix, circle, kind: file.kind, name: parts::file_name(&file.path).into(), size: file.size })
            .collect();
        let layout = Rc::new(layout);
        self.bubbles = Some(layout.clone());
        layout
    }

    pub(super) fn render_bubbles(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let layout = self.bubble_layout(cx);
        let theme = cx.theme();
        if layout.is_empty() {
            let message = if self.files.read(cx).delegate().len() == 0 { "No files found" } else { "No files match the filter" };
            return div().size_full().flex().items_center().justify_center().text_sm().text_color(theme.muted_foreground).child(message).into_any_element();
        }

        let mut kinds: Vec<Kind> = layout.iter().map(|bubble| bubble.kind).collect();
        kinds.sort();
        kinds.dedup();
        let legend = h_flex()
            .flex_shrink_0()
            .flex_wrap()
            .gap_x_4()
            .gap_y_1()
            .px_3()
            .pt_2()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(format!("{} largest files", layout.len()))
            .children(kinds.into_iter().map(|kind| parts::with_dot(kind, kind.label().to_string(), cx)));

        let selected = self.files.read(cx).delegate().selected;
        let hovered = self.hovered;
        let page = cx.entity().downgrade();
        let menu_page = page.clone();
        let paint_layout = layout.clone();
        let bubbles = canvas(
            move |bounds, window, _| {
                let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                (hitbox, place(&layout, bounds))
            },
            move |bounds, (hitbox, placed), window, cx| {
                let layout = paint_layout;
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    for spot in &placed {
                        let bubble = &layout[spot.ix];
                        paint_bubble(spot, bubble, selected == Some(bubble.file), hovered == Some(bubble.file), window, cx);
                    }
                    if let Some(spot) = placed.iter().find(|spot| hovered == Some(layout[spot.ix].file)) {
                        window.set_cursor_style(CursorStyle::PointingHand, &hitbox);
                        paint_tooltip(spot, &layout[spot.ix], bounds, window, cx);
                    }
                });
                listen(page, hitbox, Rc::new(placed), layout, window);
            },
        )
        .size_full();

        v_flex()
            .size_full()
            .key_context("Bubbles")
            .track_focus(&self.bubbles_focus)
            .child(legend)
            .child(
                div().id("disk-bubbles").flex_1().min_h_0().child(bubbles).context_menu(move |menu: PopupMenu, _, cx| {
                    // Right-click acts on the bubble under the pointer.
                    let target = menu_page.update(cx, |page, cx| page.select_hovered(cx)).ok().flatten();
                    let none = target.is_none();
                    menu.menu_with_disabled("Open location", Box::new(OpenLocation), none)
                        .menu_with_disabled("Copy path", Box::new(CopyPath), none)
                        .separator()
                        .menu_with_disabled("Move to Trash", Box::new(MoveToTrash), none)
                }),
            )
            .into_any_element()
    }

    /// Selects the hovered bubble's file, if the pointer is on one.
    fn select_hovered(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let file = self.hovered?;
        self.select_file(Some(file), cx);
        Some(file)
    }

    fn select_file(&mut self, file: Option<usize>, cx: &mut Context<Self>) {
        self.files.update(cx, |state, cx| {
            state.delegate_mut().selected = file;
            cx.notify();
        });
        cx.notify();
    }
}

/// Fits the pile inside `bounds` and drops bubbles too small to see.
fn place(layout: &[Bubble], bounds: Bounds<Pixels>) -> Vec<Placed> {
    let circles: Vec<Circle> = layout.iter().map(|bubble| bubble.circle).collect();
    let width = f64::from(f32::from(bounds.size.width) - INSET * 2.);
    let height = f64::from(f32::from(bounds.size.height) - INSET * 2.);
    let origin = bounds.origin + point(px(INSET), px(INSET));
    packing::fit(&circles, width, height)
        .into_iter()
        .enumerate()
        .filter(|(_, circle)| circle.r as f32 >= MIN_RADIUS)
        .map(|(ix, circle)| Placed { ix, center: origin + point(px(circle.x as f32), px(circle.y as f32)), radius: px(circle.r as f32) })
        .collect()
}

/// The bubble under `position`, as a Files table index.
fn hit(placed: &[Placed], layout: &[Bubble], position: Point<Pixels>) -> Option<usize> {
    placed
        .iter()
        .find(|spot| {
            let (dx, dy) = (f32::from(position.x - spot.center.x), f32::from(position.y - spot.center.y));
            dx * dx + dy * dy <= f32::from(spot.radius).powi(2)
        })
        .map(|spot| layout[spot.ix].file)
}

/// Hover follows the pointer, a left click selects, and a click on empty
/// space clears the selection.
fn listen(page: WeakEntity<DiskSpacePage>, hitbox: Hitbox, placed: Rc<Vec<Placed>>, layout: Rc<Vec<Bubble>>, window: &mut Window) {
    {
        let (page, hitbox, placed, layout) = (page.clone(), hitbox.clone(), placed.clone(), layout.clone());
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            let over = if hitbox.is_hovered(window) { hit(&placed, &layout, event.position) } else { None };
            _ = page.update(cx, |page, cx| {
                if page.hovered != over {
                    page.hovered = over;
                    cx.notify();
                }
            });
        });
    }
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
            return;
        }
        let over = hit(&placed, &layout, event.position);
        _ = page.update(cx, |page, cx| {
            if event.button == MouseButton::Left {
                page.bubbles_focus.focus(window, cx);
                page.select_file(over, cx);
            } else if over.is_some() {
                page.select_file(over, cx);
            }
        });
    });
}

fn paint_bubble(spot: &Placed, bubble: &Bubble, selected: bool, hovered: bool, window: &mut Window, cx: &mut App) {
    let color = kind_color(bubble.kind, cx);
    let foreground = cx.theme().foreground;
    let r = spot.radius;
    let bounds = Bounds::new(spot.center - point(r, r), size(r * 2., r * 2.));
    let (border, border_color) = if selected { (px(2.), foreground) } else { (px(1.), color.opacity(0.9)) };
    window.paint_quad(fill(bounds, color.opacity(if hovered { 0.7 } else { 0.4 })).corner_radii(r).border_widths(border).border_color(border_color));

    if f32::from(r) < LABEL_RADIUS {
        return;
    }
    let width = r * 1.6;
    let gap = px(2.);
    let top = spot.center.y - px(LABEL_FONT + DETAIL_FONT) / 2. - gap;
    paint_text(&bubble.name, spot.center.x, top, px(LABEL_FONT), foreground, width, window, cx);
    paint_text(&format::bytes(bubble.size), spot.center.x, top + px(LABEL_FONT) + gap * 2., px(DETAIL_FONT), foreground.opacity(0.75), width, window, cx);
}

/// The hovered file's name, size and type in a box just above its bubble,
/// or below it when there's no room above.
fn paint_tooltip(spot: &Placed, bubble: &Bubble, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let theme = cx.theme();
    let (background, border, foreground, muted, radius) = (theme.popover, theme.border, theme.popover_foreground, theme.muted_foreground, theme.radius);
    let name = shape(&bubble.name, px(LABEL_FONT), foreground, window);
    let detail = shape(&format!("{} · {}", format::bytes(bubble.size), bubble.kind.label()), px(DETAIL_FONT), muted, window);
    let padding = px(8.);
    let line = px(LABEL_FONT * 1.4);
    let box_size = size(name.width().max(detail.width()) + padding * 2., line * 2. + padding * 2.);

    let above = spot.center.y - spot.radius - px(6.) - box_size.height;
    let y = if above >= bounds.top() { above } else { (spot.center.y + spot.radius + px(6.)).min(bounds.bottom() - box_size.height) };
    let x = (spot.center.x - box_size.width / 2.).clamp(bounds.left(), (bounds.right() - box_size.width).max(bounds.left()));
    let frame = Bounds::new(point(x, y), box_size);
    window.paint_quad(fill(frame, background).corner_radii(radius).border_widths(px(1.)).border_color(border));
    _ = name.paint(frame.origin + point(padding, padding), line, TextAlign::Left, None, window, cx);
    _ = detail.paint(frame.origin + point(padding, padding + line), line, TextAlign::Left, None, window, cx);
}

fn shape(text: &str, font_size: Pixels, color: Hsla, window: &mut Window) -> ShapedLine {
    let run = TextRun { len: text.len(), font: window.text_style().font(), color, background_color: None, underline: None, strikethrough: None };
    window.text_system().shape_line(SharedString::from(text.to_string()), font_size, &[run], None)
}

/// Draws one line centered on `center_x`, cut short with "…" to fit
/// `max_width`. Nothing is drawn when even a few letters won't fit.
#[allow(clippy::too_many_arguments)]
fn paint_text(text: &str, center_x: Pixels, top: Pixels, font_size: Pixels, color: Hsla, max_width: Pixels, window: &mut Window, cx: &mut App) {
    let mut line = shape(text, font_size, color, window);
    let mut chars = text.chars().count();
    while line.width() > max_width {
        // Shrink in proportion to the overflow, then measure again.
        chars = ((chars as f32 * f32::from(max_width) / f32::from(line.width())) as usize).min(chars.saturating_sub(1));
        if chars < 4 {
            return;
        }
        let cut: String = text.chars().take(chars - 1).chain(std::iter::once('…')).collect();
        line = shape(&cut, font_size, color, window);
    }
    _ = line.paint(point(center_x - line.width() / 2., top), font_size * 1.3, TextAlign::Left, None, window, cx);
}
