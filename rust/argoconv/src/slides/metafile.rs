//! 메타파일(EMF·WMF)과 낯선 래스터(BMP·TIFF) 그림.
//!
//! EMF·WMF 는 rhwp 의 변환기로 SVG 로 옮긴다. rhwp 를 빼고 지으면 건너뛴다.

#[cfg(feature = "rhwp")]
pub(crate) fn emf(bytes: &[u8]) -> Option<Vec<u8>> {
    std::panic::catch_unwind(|| rhwp::emf::convert_to_standalone_svg(bytes)).ok().flatten()
}

#[cfg(not(feature = "rhwp"))]
pub(crate) fn emf(_bytes: &[u8]) -> Option<Vec<u8>> {
    None
}

#[cfg(feature = "rhwp")]
pub(crate) fn wmf(bytes: &[u8]) -> Option<Vec<u8>> {
    use rhwp::wmf::converter::{SVGPlayer, WMFConverter};
    std::panic::catch_unwind(|| {
        let player = SVGPlayer::new();
        WMFConverter::new(bytes, player).run().ok()
    })
    .ok()
    .flatten()
}

#[cfg(not(feature = "rhwp"))]
pub(crate) fn wmf(_bytes: &[u8]) -> Option<Vec<u8>> {
    None
}

/// BMP·TIFF 를 PNG 로.
pub(crate) fn raster(bytes: &[u8]) -> Option<Vec<u8>> {
    let format = if bytes.starts_with(b"BM") {
        image::ImageFormat::Bmp
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        image::ImageFormat::Tiff
    } else {
        return None;
    };
    let img = std::panic::catch_unwind(|| image::load_from_memory_with_format(bytes, format).ok()).ok().flatten()?;
    if img.width() as u64 * img.height() as u64 > 40_000_000 {
        return None;
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}
