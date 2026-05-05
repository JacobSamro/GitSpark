use std::fs;
use std::path::Path;
use std::sync::Arc;

use gpui::*;
use gpui_component::v_flex;

use crate::ui::theme;
use crate::ui::theme::z;

pub(crate) fn render_image_diff_panel(repo_path: Option<&Path>, file_path: &str) -> Div {
    let image_path = repo_path.map(|repo_path| repo_path.join(file_path));
    let mut panel = v_flex()
        .relative()
        .size_full()
        .items_center()
        .gap(z(12.0))
        .p(z(20.0))
        .text_size(z(14.0))
        .text_color(theme::text_muted());

    panel = match image_path
        .as_ref()
        .and_then(|path| image_source_from_path(path))
    {
        Some(source) => panel.child(
            div()
                .w_full()
                .h(z(240.0))
                .flex_shrink_0()
                .min_h_0()
                .overflow_hidden()
                .items_center()
                .justify_center()
                .child(
                    img(source)
                        .id("diff-image-preview")
                        .size(z(220.0))
                        .object_fit(ObjectFit::Contain)
                        .with_loading(|| image_message("Loading image preview").into_any_element())
                        .with_fallback(|| {
                            image_message("Image preview unavailable").into_any_element()
                        }),
                ),
        ),
        _ => panel.child(image_message("Image preview unavailable")),
    };

    panel
}

fn image_source_from_path(path: &Path) -> Option<Arc<Image>> {
    let format = image_format_for_path(path)?;
    let bytes = fs::read(path).ok()?;
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

fn image_format_for_path(path: &Path) -> Option<ImageFormat> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "webp" => Some(ImageFormat::Webp),
        "gif" => Some(ImageFormat::Gif),
        "bmp" => Some(ImageFormat::Bmp),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        "svg" => Some(ImageFormat::Svg),
        _ => None,
    }
}

fn image_message(message: &'static str) -> Div {
    div()
        .text_size(z(14.0))
        .text_color(theme::text_muted())
        .child(message)
}
