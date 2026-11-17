//! 开发与测试期的进程内窗口自截屏工具（macOS 免 TCC 捕获当前进程自有窗口）。

use std::path::Path;

#[cfg(target_os = "macos")]
pub fn capture_own_main_window_png(output_path: &Path) -> anyhow::Result<()> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};
    use core_graphics::window::{
        CGWindowID, copy_window_info, create_image, kCGNullWindowID,
        kCGWindowImageBoundsIgnoreFraming, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionIncludingWindow, kCGWindowListOptionOnScreenOnly,
    };
    use std::fs::File;
    use std::io::BufWriter;

    let my_pid = std::process::id() as i64;
    let info_array = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )
    .ok_or_else(|| anyhow::anyhow!("CGWindowListCopyWindowInfo returned null"))?;

    let owner_key = CFString::new("kCGWindowOwnerPID");
    let layer_key = CFString::new("kCGWindowLayer");
    let number_key = CFString::new("kCGWindowNumber");

    let mut target_window_id: Option<CGWindowID> = None;

    for idx in 0..info_array.len() {
        let Some(item_ptr) = info_array.get(idx) else {
            continue;
        };
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule((*item_ptr) as *const _) };

        let owner_pid = dict
            .find(&owner_key)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64());
        if owner_pid != Some(my_pid) {
            continue;
        }
        let layer = dict
            .find(&layer_key)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(-1);
        if layer != 0 {
            continue;
        }
        if let Some(win_num) = dict
            .find(&number_key)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
        {
            target_window_id = Some(win_num as CGWindowID);
            break;
        }
    }

    let window_id =
        target_window_id.ok_or_else(|| anyhow::anyhow!("no on-screen layer-0 window for pid {my_pid}"))?;

    let cg_rect_null = CGRect::new(
        &CGPoint::new(f64::INFINITY, f64::INFINITY),
        &CGSize::new(0.0, 0.0),
    );
    let cg_image = create_image(
        cg_rect_null,
        kCGWindowListOptionIncludingWindow,
        window_id,
        kCGWindowImageBoundsIgnoreFraming,
    )
    .ok_or_else(|| anyhow::anyhow!("CGWindowListCreateImage returned null for window {window_id}"))?;

    let width = cg_image.width();
    let height = cg_image.height();
    let bytes_per_row = cg_image.bytes_per_row();
    let raw_data = cg_image.data();
    let src = raw_data.bytes();

    if width == 0 || height == 0 {
        anyhow::bail!("captured empty image ({width}x{height})");
    }

    // CoreGraphics window capture produces BGRA8888 (little-endian 32-bit premultiplied first).
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row_start = y * bytes_per_row;
        for x in 0..width {
            let px_off = row_start + x * 4;
            if px_off + 3 < src.len() {
                let b = src[px_off];
                let g = src[px_off + 1];
                let r = src[px_off + 2];
                let a = src[px_off + 3];
                rgba.extend_from_slice(&[r, g, b, a]);
            }
        }
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(output_path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder.write_header()?;
    png_writer.write_image_data(&rgba)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn capture_own_main_window_png(_output_path: &Path) -> anyhow::Result<()> {
    Ok(())
}
