use crate::gpu_mirror_display::{state::Application, utility_texture::OverlayImage};
use std::time::{Duration, SystemTime};

pub(crate) fn remove_expired_mouse_events(app: &mut Application) {
    let mut i = 0;

    while i < app.user_interaction.mouse_clicks.len() {
        let current = app.user_interaction.mouse_clicks[i].1 + Duration::from_millis(300);

        if SystemTime::now() > current {
            app.user_interaction.mouse_clicks.remove(i);
        } else {
            i += 1;
        }
    }

    let mut i = 0;

    while i < app.user_interaction.mouse_downs.len() {
        let current = app.user_interaction.mouse_downs[i].1 + Duration::from_millis(300);

        if SystemTime::now() > current {
            app.user_interaction.mouse_downs.remove(i);
        } else {
            i += 1;
        }
    }
}

/// Checks if any active clicks are found inside the coordinates of the image. If so,
/// it removes them. It then returns a bool that represents if it removed anything.
pub(crate) fn found_remove_mouse_click(
    mouse_clicks: &mut Vec<((u32, u32), SystemTime)>,
    img_posiition: &(i32, i32),
    img: &OverlayImage,
) -> bool {
    if let Some(idx) = first_in_range(
        &mouse_clicks,
        &(
            *img_posiition,
            (
                (img_posiition.0 + img.dimensions.width as i32),
                (img_posiition.1 + img.dimensions.height as i32),
            ),
        ),
    ) {
        mouse_clicks.remove(idx);
        true
    } else {
        false
    }
}

pub(crate) fn mouse_in_img_bounds(
    mouse_position: &(i32, i32),
    img_posiition: &(i32, i32),
    img: &OverlayImage,
    set_hover: &mut bool,
) -> bool {
    let (mouse_x, mouse_y) = mouse_position.clone();
    let (exit_x, exit_y) = img_posiition.clone();

    if mouse_x > exit_x
        && mouse_y > exit_y
        && mouse_x < exit_x + img.dimensions.width as i32
        && mouse_y < exit_y + img.dimensions.height as i32
    {
        *set_hover = true;
        true
    } else {
        false
    }
}

pub(crate) fn in_range(
    mouse_click: &((u32, u32), SystemTime),
    range: &((i32, i32), (i32, i32)),
) -> bool {
    let mouse_click: (i32, i32) = (mouse_click.0.0 as i32, mouse_click.0.1 as i32);
    let (min_x, max_x) = (range.0.0.min(range.1.0), range.0.0.max(range.1.0));
    let (min_y, max_y) = (range.0.1.min(range.1.1), range.0.1.max(range.1.1));

    if mouse_click.0 >= min_x && mouse_click.0 <= max_x {
        if mouse_click.1 >= min_y && mouse_click.1 <= max_y {
            return true;
        }
    }

    false
}

pub(crate) fn first_in_range(
    clicks: &Vec<((u32, u32), SystemTime)>,
    range: &((i32, i32), (i32, i32)),
) -> Option<usize> {
    for i in 0..clicks.len() {
        if in_range(&clicks[i], range) {
            return Some(i);
        }
    }

    None
}
