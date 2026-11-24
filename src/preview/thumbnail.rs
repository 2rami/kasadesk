use openh264::formats::YUVSource;

/// Old screen services can encode the native desktop, including portrait or ultrawide displays.
pub(super) fn fits_input(w: u32, h: u32) -> bool {
    w.min(h) > 0 && w.max(h) <= 3840 && w.min(h) <= 2160
}

pub(super) fn fitted(w: usize, h: usize, limit: (usize, usize)) -> (usize, usize) {
    let (mw, mh) = limit;
    if w <= mw && h <= mh {
        (w, h)
    } else if mw * h <= mh * w {
        (mw, (h * mw / w).max(1))
    } else {
        ((w * mh / h).max(1), mh)
    }
}

fn sample_pair(i: usize, source: usize, output: usize) -> [usize; 2] {
    [(4 * i + 1) * source / (4 * output), (4 * i + 3) * source / (4 * output)]
}

/// Filter the decoder's strided I420 planes at the destination resolution, avoiding a full-size
/// RGBA allocation and conversion for every thumbnail. Use the same limited-range BT.601
/// transform as OpenH264's write_rgba8.
pub(super) fn downsample(source: &impl YUVSource, (width, height): (usize, usize)) -> Vec<u8> {
    let (w, h) = source.dimensions();
    let (sy, su, sv) = source.strides();
    let (y, u, v) = (source.y(), source.u(), source.v());
    let columns: Vec<_> = (0..width).map(|i| sample_pair(i, w, width)).collect();
    let mut pixels = vec![0; width * height * 4];
    for (row, dst) in pixels.chunks_exact_mut(width * 4).enumerate() {
        let [a, b] = sample_pair(row, h, height);
        for (&[l, r], rgba) in columns.iter().zip(dst.chunks_exact_mut(4)) {
            let average = |plane: &[u8], stride: usize, shift: usize| {
                let (top, bottom) = ((a >> shift) * stride, (b >> shift) * stride);
                let (left, right) = (l >> shift, r >> shift);
                (i32::from(plane[top + left])
                    + i32::from(plane[top + right])
                    + i32::from(plane[bottom + left])
                    + i32::from(plane[bottom + right])
                    + 2)
                    / 4
            };
            let luminance = 298 * (average(y, sy, 0) - 16);
            let cb = average(u, su, 1) - 128;
            let cr = average(v, sv, 1) - 128;
            let channel = |value: i32| ((value + 128) >> 8).clamp(0, 255) as u8;
            rgba.copy_from_slice(&[
                channel(luminance + 409 * cr),
                channel(luminance - 100 * cb - 208 * cr),
                channel(luminance + 516 * cb),
                255,
            ]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    use openh264::formats::YUVSlices;

    #[test]
    fn native_and_legacy_screens_keep_their_aspect_and_output_budget() {
        assert_eq!(fitted(192, 108, (192, 120)), (192, 108));
        assert_eq!(fitted(1920, 1080, (320, 200)), (320, 180));
        assert_eq!(fitted(3840, 1600, (512, 320)), (512, 213));
        assert_eq!(fitted(1080, 1920, (512, 320)), (180, 320));
        assert!(fits_input(2160, 3840));
        assert!(!fits_input(3841, 1080) && !fits_input(2161, 2161) && !fits_input(0, 1080));
    }

    #[test]
    fn resize_reads_plane_strides_and_converts_only_output_pixels() {
        let mut y = [0; 48];
        for row in y.chunks_exact_mut(12) {
            row[..4].fill(16);
            row[4..8].fill(235);
        }
        let mut chroma = [0; 12];
        for row in chroma.chunks_exact_mut(6) {
            row[..4].fill(128);
        }
        let source = YUVSlices::new((&y, &chroma, &chroma), (8, 4), (12, 6, 6));
        assert_eq!(downsample(&source, (2, 1)), [0, 0, 0, 255, 255, 255, 255, 255]);
    }
}
