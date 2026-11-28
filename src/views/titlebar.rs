#![cfg_attr(target_os = "macos", allow(dead_code))]

use crate::components::icon;
use crate::scale::px;
use crate::theme::*;
use gpui::{
    AnyElement, Bounds, BoxShadow, CursorStyle, Decorations, FontWeight, HitboxBehavior,
    MouseButton, Pixels, Point, ResizeEdge, SharedString, Size, Window, WindowControlArea, black,
    canvas, div, point, prelude::*, rgb, svg, transparent_black, white,
};

const CLOSE_HOVER_BG: u32 = 0xc42b1c;
const RESIZE_INSET: f32 = 10.;
const WINDOW_RADIUS: f32 = 10.;

/// Wraps the application root with a custom titlebar (and client-side resize frame on Linux).
/// On macOS this is a no-op so the native macOS titlebar remains untouched.
pub fn wrap_window_chrome(
    workspace: Option<SharedString>,
    content: AnyElement,
    window: &mut Window,
) -> AnyElement {
    if cfg!(target_os = "macos") {
        return content;
    }
    wrap_client_chrome(workspace, content, window)
}

fn wrap_client_chrome(
    workspace: Option<SharedString>,
    content: AnyElement,
    window: &mut Window,
) -> AnyElement {
    let decorations = window.window_decorations();
    let fullscreen = window.is_fullscreen();
    let show_titlebar = !fullscreen
        && (cfg!(target_os = "windows") || matches!(decorations, Decorations::Client { .. }));

    let inner = div()
        .size_full()
        .flex()
        .flex_col()
        .bg(rgb(PAGE))
        .when(show_titlebar, |d| d.child(titlebar(workspace, window)))
        .child(div().flex_1().min_h_0().w_full().flex().child(content));

    match decorations {
        Decorations::Server => inner.into_any_element(),
        Decorations::Client { tiling } => {
            if fullscreen {
                window.set_client_inset(gpui::px(0.));
                return inner.into_any_element();
            }
            let inset = gpui::px(RESIZE_INSET);
            let rounding = gpui::px(WINDOW_RADIUS);
            window.set_client_inset(inset);

            let frame = inner
                .cursor(CursorStyle::Arrow)
                .overflow_hidden()
                .border_color(rgb(BORDER_STRONG))
                .when(!(tiling.top || tiling.left), |d| d.rounded_tl(rounding))
                .when(!(tiling.top || tiling.right), |d| d.rounded_tr(rounding))
                .when(!(tiling.bottom || tiling.left), |d| d.rounded_bl(rounding))
                .when(!(tiling.bottom || tiling.right), |d| d.rounded_br(rounding))
                .when(!tiling.top, |d| d.border_t_1())
                .when(!tiling.bottom, |d| d.border_b_1())
                .when(!tiling.left, |d| d.border_l_1())
                .when(!tiling.right, |d| d.border_r_1())
                .when(!tiling.is_tiled(), |d| {
                    d.shadow(vec![BoxShadow {
                        color: black().opacity(0.14),
                        offset: point(gpui::px(0.), gpui::px(2.)),
                        blur_radius: gpui::px(10.),
                        spread_radius: gpui::px(0.),
                        inset: false,
                    }])
                })
                .on_mouse_move(|_, _, cx| cx.stop_propagation());

            div()
                .id("window-backdrop")
                .size_full()
                .bg(transparent_black())
                .when(!tiling.top, |d| d.pt(inset))
                .when(!tiling.bottom, |d| d.pb(inset))
                .when(!tiling.left, |d| d.pl(inset))
                .when(!tiling.right, |d| d.pr(inset))
                .child(resize_cursor_overlay(inset))
                .on_mouse_move(|_, window, _| window.refresh())
                .on_mouse_down(MouseButton::Left, move |ev, window, _| {
                    let bounds = window.window_bounds().get_bounds().size;
                    if let Some(edge) = resize_edge(ev.position, inset, bounds) {
                        window.start_window_resize(edge);
                    }
                })
                .child(frame)
                .into_any_element()
        }
    }
}

fn titlebar(workspace: Option<SharedString>, window: &Window) -> impl IntoElement {
    let active = window.is_window_active();
    let maximized = window.is_maximized();
    let controls = window.window_controls();
    let workspace = workspace.filter(|w| !w.is_empty());

    let drag_area = div()
        .id("titlebar-drag")
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(12.))
        .window_control_area(WindowControlArea::Drag)
        .when(!active, |d| d.opacity(0.6))
        .child(icon("logo", 15., TEXT))
        .child(
            div()
                .flex_none()
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT))
                .child("BetterOdoo"),
        )
        .when_some(workspace, |d, ws| {
            d.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(BORDER_STRONG))
                    .child("·"),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(ws),
            )
        });

    #[cfg(not(target_os = "windows"))]
    let drag_area = drag_area
        .on_mouse_down(MouseButton::Left, |ev, window, _| {
            if ev.click_count >= 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
        })
        .on_mouse_down(MouseButton::Right, |ev, window, _| {
            window.show_window_menu(ev.position);
        });

    div()
        .id("custom-titlebar")
        .flex_none()
        .w_full()
        .h(px(36.))
        .flex()
        .items_center()
        .bg(rgb(SIDEBAR))
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(drag_area)
        .child(
            div()
                .flex_none()
                .h_full()
                .flex()
                .items_center()
                .when(!active, |d| d.opacity(0.65))
                .when(controls.minimize, |d| {
                    d.child(control_button(
                        "window-min",
                        "window-minimize",
                        WindowControlArea::Min,
                        false,
                        |window| window.minimize_window(),
                    ))
                })
                .when(controls.maximize, |d| {
                    d.child(control_button(
                        "window-max",
                        if maximized {
                            "window-restore"
                        } else {
                            "window-maximize"
                        },
                        WindowControlArea::Max,
                        false,
                        |window| window.zoom_window(),
                    ))
                })
                .child(control_button(
                    "window-close",
                    "window-close",
                    WindowControlArea::Close,
                    true,
                    |window| window.remove_window(),
                )),
        )
}

fn control_button(
    id: &'static str,
    glyph: &'static str,
    area: WindowControlArea,
    danger: bool,
    on_linux_click: fn(&mut Window),
) -> impl IntoElement {
    let btn = div()
        .id(id)
        .flex_none()
        .w(px(44.))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(rgb(TEXT_SOFT))
        .window_control_area(area)
        .when(danger, |d| {
            d.hover(|s| s.bg(rgb(CLOSE_HOVER_BG)).text_color(white()))
        })
        .when(!danger, |d| {
            d.hover(|s| s.bg(rgb(HOVER)).text_color(rgb(TEXT)))
        })
        .child(
            svg()
                .path(format!("icons/{glyph}.svg"))
                .size(px(13.))
                .flex_none(),
        );

    // On Windows, `WindowControlArea` maps to native HTMINBUTTON/HTMAXBUTTON/HTCLOSE hit tests
    // (giving native maximize/restore toggle, Windows 11 Snap Layouts on hover, and WM_CLOSE).
    // Consuming mouse events here would suppress that native handling, so only attach GPUI click
    // handlers on non-Windows platforms (Linux Wayland/X11).
    #[cfg(target_os = "windows")]
    {
        let _ = on_linux_click;
        btn
    }

    #[cfg(not(target_os = "windows"))]
    {
        btn.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, _| on_linux_click(window))
    }
}

fn resize_cursor_overlay(inset: Pixels) -> impl IntoElement {
    canvas(
        |_bounds, window, _| {
            window.insert_hitbox(
                Bounds::new(
                    point(gpui::px(0.), gpui::px(0.)),
                    window.window_bounds().get_bounds().size,
                ),
                HitboxBehavior::Normal,
            )
        },
        move |_bounds, hitbox, window, _| {
            let mouse = window.mouse_position();
            let size = window.window_bounds().get_bounds().size;
            let Some(edge) = resize_edge(mouse, inset, size) else {
                return;
            };
            window.set_cursor_style(
                match edge {
                    ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
                    ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
                    ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                        CursorStyle::ResizeUpLeftDownRight
                    }
                    ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                        CursorStyle::ResizeUpRightDownLeft
                    }
                },
                &hitbox,
            );
        },
    )
    .size_full()
    .absolute()
}

pub(crate) fn resize_edge(
    pos: Point<Pixels>,
    inset: Pixels,
    size: Size<Pixels>,
) -> Option<ResizeEdge> {
    let edge = if pos.y < inset && pos.x < inset {
        ResizeEdge::TopLeft
    } else if pos.y < inset && pos.x > size.width - inset {
        ResizeEdge::TopRight
    } else if pos.y < inset {
        ResizeEdge::Top
    } else if pos.y > size.height - inset && pos.x < inset {
        ResizeEdge::BottomLeft
    } else if pos.y > size.height - inset && pos.x > size.width - inset {
        ResizeEdge::BottomRight
    } else if pos.y > size.height - inset {
        ResizeEdge::Bottom
    } else if pos.x < inset {
        ResizeEdge::Left
    } else if pos.x > size.width - inset {
        ResizeEdge::Right
    } else {
        return None;
    };
    Some(edge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_resize_edges_and_interior() {
        let size = Size {
            width: gpui::px(1000.),
            height: gpui::px(700.),
        };
        let inset = gpui::px(10.);

        assert_eq!(
            resize_edge(point(gpui::px(4.), gpui::px(4.)), inset, size),
            Some(ResizeEdge::TopLeft)
        );
        assert_eq!(
            resize_edge(point(gpui::px(995.), gpui::px(4.)), inset, size),
            Some(ResizeEdge::TopRight)
        );
        assert_eq!(
            resize_edge(point(gpui::px(500.), gpui::px(3.)), inset, size),
            Some(ResizeEdge::Top)
        );
        assert_eq!(
            resize_edge(point(gpui::px(4.), gpui::px(695.)), inset, size),
            Some(ResizeEdge::BottomLeft)
        );
        assert_eq!(
            resize_edge(point(gpui::px(995.), gpui::px(695.)), inset, size),
            Some(ResizeEdge::BottomRight)
        );
        assert_eq!(
            resize_edge(point(gpui::px(500.), gpui::px(695.)), inset, size),
            Some(ResizeEdge::Bottom)
        );
        assert_eq!(
            resize_edge(point(gpui::px(3.), gpui::px(350.)), inset, size),
            Some(ResizeEdge::Left)
        );
        assert_eq!(
            resize_edge(point(gpui::px(995.), gpui::px(350.)), inset, size),
            Some(ResizeEdge::Right)
        );
        assert_eq!(
            resize_edge(point(gpui::px(500.), gpui::px(350.)), inset, size),
            None
        );
    }
}
