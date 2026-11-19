//! The player as the Skins tab shows it: Minecraft's player model (head, body, arms and legs,
//! their outer layer, and the cape) drawn in software, so the launcher needs no 3D view of its
//! own. Each part is a box whose six faces take the skin's standard box layout; the boxes are
//! turned by a yaw and a pitch, drawn with a depth buffer, nearest texel, and a light from the
//! front so their sides read.
//!
//! Model units are skin pixels: the feet at y 0, the top of the head at y 32, the player facing
//! +Z (toward the viewer at yaw 0), their right side at −X.

/// A picture as RGBA texels, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture {
    pub texels: Vec<[u8; 4]>,
    pub w: usize,
    pub h: usize,
}

impl Texture {
    fn texel(&self, x: usize, y: usize) -> [u8; 4] {
        self.texels[y.min(self.h - 1) * self.w + x.min(self.w - 1)]
    }
}

/// A skin picture as the model wears it: 64 by 64 (or a multiple), the old 64 by 32 skins given
/// their left arm and leg the way the game gives them (`processLegacySkin`). None for any other
/// shape or something that isn't a PNG.
pub fn skin(png: &[u8]) -> Option<Texture> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w == 0 || w % 64 != 0 || w > 1024 || (h != w && h * 2 != w) {
        return None;
    }
    let mut texels = vec![[0; 4]; w * w];
    for (i, p) in image.pixels().enumerate() {
        texels[i] = p.0;
    }
    if h * 2 == w {
        let k = (w / 64) as i64;
        // (from x, from y, to x offset, to y offset, width, height): the right arm and leg,
        // mirrored into the left ones' places, as vanilla's `processLegacySkin`.
        const RECTS: [(i64, i64, i64, i64, i64, i64); 12] = [
            (4, 16, 16, 32, 4, 4),
            (8, 16, 16, 32, 4, 4),
            (0, 20, 24, 32, 4, 12),
            (4, 20, 16, 32, 4, 12),
            (8, 20, 8, 32, 4, 12),
            (12, 20, 16, 32, 4, 12),
            (44, 16, -8, 32, 4, 4),
            (48, 16, -8, 32, 4, 4),
            (40, 20, 0, 32, 4, 12),
            (44, 20, -8, 32, 4, 12),
            (48, 20, -16, 32, 4, 12),
            (52, 20, -8, 32, 4, 12),
        ];
        let at = |x: i64, y: i64| (y * w as i64 + x) as usize;
        for (sx, sy, ox, oy, rw, rh) in RECTS {
            let (sx, sy, ox, oy, rw, rh) = (sx * k, sy * k, ox * k, oy * k, rw * k, rh * k);
            for y in 0..rh {
                for x in 0..rw {
                    texels[at(sx + ox + rw - 1 - x, sy + oy + y)] = texels[at(sx + x, sy + y)];
                }
            }
        }
        let k = k as usize;
        let area = |x0: usize, y0: usize, x1: usize, y1: usize| {
            (y0 * k..y1 * k).flat_map(move |y| (x0 * k..x1 * k).map(move |x| y * w + x))
        };
        // The base layer is opaque (`setNoAlpha`), and a hat with no clear texel at all is an
        // old skin's filler, not a hat: it is cleared (`doNotchTransparencyHack`).
        for i in area(0, 0, 32, 16)
            .chain(area(0, 16, 64, 32))
            .chain(area(16, 48, 48, 64))
        {
            texels[i][3] = 255;
        }
        if area(32, 0, 64, 16).all(|i| texels[i][3] >= 128) {
            for i in area(32, 0, 64, 16) {
                texels[i] = [0; 4];
            }
        }
    }
    Some(Texture { texels, w, h: w })
}

/// A cape picture on the smallest 64·2ⁿ by 32·2ⁿ canvas it fits, as OptiFine lays its own out:
/// Mojang's 64 by 32 capes, OptiFine's 46 by 22 and 92 by 44, and the old 22 by 17 ones all keep
/// the 64 by 32 layout's coordinates.
pub fn cape(png: &[u8]) -> Option<Texture> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w == 0 || h == 0 || w > 2048 || h > 1024 {
        return None;
    }
    let mut k = 1;
    while 64 * k < w || 32 * k < h {
        k *= 2;
    }
    // The outer face must be inside the picture.
    if 11 * k > w || 17 * k > h {
        return None;
    }
    let (cw, ch) = (64 * k, 32 * k);
    let mut texels = vec![[0; 4]; cw * ch];
    for (x, y, p) in image.enumerate_pixels() {
        texels[y as usize * cw + x as usize] = p.0;
    }
    Some(Texture {
        texels,
        w: cw,
        h: ch,
    })
}

/// The cape's outer face (10 by 16 of the 64 by 32 layout, at 1, 1), as a player behind it sees
/// it: what a cape's tile shows.
pub fn cape_front(cape: &Texture) -> Texture {
    let k = cape.w / 64;
    let (w, h) = (10 * k, 16 * k);
    let texels = (k..k + h)
        .flat_map(|y| (k..k + w).map(move |x| (x, y)))
        .map(|(x, y)| cape.texel(x, y))
        .collect();
    Texture { texels, w, h }
}

/// What the model wears and how it is turned.
pub struct Pose<'a> {
    pub skin: &'a Texture,
    /// Slim (3 pixel) arms instead of classic (4 pixel) ones.
    pub slim: bool,
    pub cape: Option<&'a Texture>,
    /// Turned about the vertical, in radians: 0 faces the viewer.
    pub yaw: f32,
    /// Tipped toward the viewer, in radians.
    pub pitch: f32,
}

type V3 = [f32; 3];
/// A part: its box's corner and size, its base and outer layers' texels, and how much the outer
/// layer grows.
type Part = (V3, V3, (f32, f32), (f32, f32), f32);

#[derive(Clone, Copy, PartialEq)]
enum Sheet {
    Skin,
    Cape,
}

/// One face: its corners top left, top right, bottom right, bottom left (as its texels lie), the
/// texels it takes (in 64 by 64 layout units), and its outward normal.
struct Quad {
    corners: [V3; 4],
    uv: [f32; 4],
    normal: V3,
    sheet: Sheet,
}

/// A box from `min` of `size`, its faces from the box layout at `uv`, grown by `grow` on every
/// side (an outer layer) without changing its texels.
fn cuboid(min: V3, size: V3, uv: (f32, f32), grow: f32, sheet: Sheet) -> Vec<Quad> {
    let [w, h, d] = size;
    let (u, v) = uv;
    let [x0, y0, z0] = [min[0] - grow, min[1] - grow, min[2] - grow];
    let [x1, y1, z1] = [min[0] + w + grow, min[1] + h + grow, min[2] + d + grow];
    let quad = |corners: [V3; 4], uv: [f32; 4], normal: V3| Quad {
        corners,
        uv,
        normal,
        sheet,
    };
    vec![
        // Front, facing +Z.
        quad(
            [[x0, y1, z1], [x1, y1, z1], [x1, y0, z1], [x0, y0, z1]],
            [u + d, v + d, u + d + w, v + d + h],
            [0., 0., 1.],
        ),
        // Back: seen from behind, its texels run toward −X.
        quad(
            [[x1, y1, z0], [x0, y1, z0], [x0, y0, z0], [x1, y0, z0]],
            [u + 2. * d + w, v + d, u + 2. * d + 2. * w, v + d + h],
            [0., 0., -1.],
        ),
        // The model's right side (−X): its texels run from the back to the front.
        quad(
            [[x0, y1, z0], [x0, y1, z1], [x0, y0, z1], [x0, y0, z0]],
            [u, v + d, u + d, v + d + h],
            [-1., 0., 0.],
        ),
        // Its left side (+X): from the front to the back.
        quad(
            [[x1, y1, z1], [x1, y1, z0], [x1, y0, z0], [x1, y0, z1]],
            [u + d + w, v + d, u + 2. * d + w, v + d + h],
            [1., 0., 0.],
        ),
        // Top: its far edge at the back.
        quad(
            [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
            [u + d, v, u + d + w, v + d],
            [0., 1., 0.],
        ),
        // Bottom.
        quad(
            [[x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0]],
            [u + d + w, v, u + d + 2. * w, v + d],
            [0., -1., 0.],
        ),
    ]
}

/// Every face of the model in `pose`, the cape's included.
fn faces(slim: bool, cape: bool) -> Vec<Quad> {
    let arm = if slim { 3. } else { 4. };
    let mut all = Vec::new();
    // (min, size, base layer's texels, outer layer's texels, how much the outer layer grows)
    let parts: [Part; 6] = [
        ([-4., 24., -4.], [8., 8., 8.], (0., 0.), (32., 0.), 0.5),
        ([-4., 12., -2.], [8., 12., 4.], (16., 16.), (16., 32.), 0.25),
        (
            [-4. - arm, 12., -2.],
            [arm, 12., 4.],
            (40., 16.),
            (40., 32.),
            0.25,
        ),
        ([4., 12., -2.], [arm, 12., 4.], (32., 48.), (48., 48.), 0.25),
        ([-4., 0., -2.], [4., 12., 4.], (0., 16.), (0., 32.), 0.25),
        ([0., 0., -2.], [4., 12., 4.], (16., 48.), (0., 48.), 0.25),
    ];
    for (min, size, base, outer, grow) in parts {
        all.extend(cuboid(min, size, base, 0., Sheet::Skin));
        all.extend(cuboid(min, size, outer, grow, Sheet::Skin));
    }
    if cape {
        // The cape box (10 by 16 by 1) hangs from the shoulders behind the back, turned half
        // round so its outer face looks backward, as the game's model is, and swung back a
        // little from its top edge.
        let tilt = 8f32.to_radians();
        let (top, back) = (24., -2.);
        for mut q in cuboid([-5., 8., -3.], [10., 16., 1.], (0., 0.), 0., Sheet::Cape) {
            let turn = |p: V3| [-p[0], p[1], 2. * -2.5 - p[2]];
            let swing = |p: V3| {
                let (dy, dz) = (p[1] - top, p[2] - back);
                [
                    p[0],
                    top + dy * tilt.cos() - dz * tilt.sin(),
                    back + dz * tilt.cos() + dy * tilt.sin(),
                ]
            };
            q.corners = q.corners.map(|p| swing(turn(p)));
            let n = q.normal;
            let n = [-n[0], n[1], -n[2]];
            q.normal = [
                n[0],
                n[1] * tilt.cos() - n[2] * tilt.sin(),
                n[2] * tilt.cos() + n[1] * tilt.sin(),
            ];
            all.push(q);
        }
    }
    all
}

/// The model in `pose`, `width` by `height` texels, row by row as RGBA; transparent around it.
pub fn render(pose: &Pose, width: usize, height: usize) -> Vec<[u8; 4]> {
    let mut out = vec![[0u8; 4]; width * height];
    let mut depth = vec![f32::NEG_INFINITY; width * height];
    if width == 0 || height == 0 {
        return out;
    }
    let (sy, cy) = pose.yaw.sin_cos();
    let (sp, cp) = pose.pitch.sin_cos();
    let turn = |p: V3| {
        let x = p[0] * cy + p[2] * sy;
        let z = -p[0] * sy + p[2] * cy;
        let y = p[1] - 16.;
        [x, y * cp - z * sp, y * sp + z * cp]
    };
    // The model is 32 units tall (36 with its outer layer, the hat and room around it), and as
    // wide as its turn makes it.
    let scale = (height as f32 / 38.).min(width as f32 / 24.);
    let (mid_x, mid_y) = (width as f32 / 2., height as f32 / 2.);
    let to_screen = |p: V3| [mid_x + p[0] * scale, mid_y - p[1] * scale, p[2]];
    let light = {
        let l: V3 = [-0.35, 0.55, 0.75];
        let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        [l[0] / n, l[1] / n, l[2] / n]
    };
    for quad in faces(pose.slim, pose.cape.is_some()) {
        let normal = turn([quad.normal[0], quad.normal[1] + 16., quad.normal[2]]);
        // Faces turned away aren't drawn.
        if normal[2] <= 0. {
            continue;
        }
        let texture = match quad.sheet {
            Sheet::Skin => pose.skin,
            Sheet::Cape => match pose.cape {
                Some(cape) => cape,
                None => continue,
            },
        };
        let lit = 0.55
            + 0.45 * (normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2]).max(0.);
        let k = texture.w as f32 / 64.;
        let corners = quad.corners.map(|p| to_screen(turn(p)));
        let [u0, v0, u1, v1] = quad.uv;
        let uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        // The texel a point of the face takes, kept inside the face's own texels.
        let sample = |u: f32, v: f32| {
            let x = (u.clamp(u0.min(u1), u0.max(u1) - 0.001) * k) as usize;
            let y = (v.clamp(v0.min(v1), v0.max(v1) - 0.001) * k) as usize;
            texture.texel(x, y)
        };
        for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
            let (pa, pb, pc) = (corners[a], corners[b], corners[c]);
            let area = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0]);
            if area.abs() < 1e-6 {
                continue;
            }
            let lo_x = pa[0].min(pb[0]).min(pc[0]).floor().max(0.) as usize;
            let hi_x = (pa[0].max(pb[0]).max(pc[0]).ceil() as usize).min(width);
            let lo_y = pa[1].min(pb[1]).min(pc[1]).floor().max(0.) as usize;
            let hi_y = (pa[1].max(pb[1]).max(pc[1]).ceil() as usize).min(height);
            for y in lo_y..hi_y {
                for x in lo_x..hi_x {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let wa = ((pb[0] - px) * (pc[1] - py) - (pb[1] - py) * (pc[0] - px)) / area;
                    let wb = ((pc[0] - px) * (pa[1] - py) - (pc[1] - py) * (pa[0] - px)) / area;
                    let wc = 1. - wa - wb;
                    if wa < 0. || wb < 0. || wc < 0. {
                        continue;
                    }
                    let z = wa * pa[2] + wb * pb[2] + wc * pc[2];
                    let i = y * width + x;
                    if z <= depth[i] {
                        continue;
                    }
                    let u = wa * uvs[a][0] + wb * uvs[b][0] + wc * uvs[c][0];
                    let v = wa * uvs[a][1] + wb * uvs[b][1] + wc * uvs[c][1];
                    let [r, g, bl, al] = sample(u, v);
                    // Clear texels (the outer layer's gaps, a cape's corners) let what is behind
                    // show, as the game's cutout does.
                    if al < 128 {
                        continue;
                    }
                    let shade = |c: u8| (c as f32 * lit).round().min(255.) as u8;
                    out[i] = [shade(r), shade(g), shade(bl), 255];
                    depth[i] = z;
                }
            }
        }
    }
    out
}

/// Whether a skin is made for slim arms: its right arm's fourth column of front texels is clear,
/// as slim skins leave it.
pub fn looks_slim(skin: &Texture) -> bool {
    let k = skin.w / 64;
    (20 * k..32 * k).all(|y| skin.texel(54 * k, y)[3] == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba(f(x, y)));
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// A skin whose every base texel says which part it belongs to: red for the head, green for
    /// the body, blue for the arms and legs; its outer layer clear.
    fn marked() -> Texture {
        skin(&png(64, 64, |x, y| {
            let outer =
                y < 16 && x >= 32 || (32..48).contains(&y) || y >= 48 && !(16..48).contains(&x);
            if outer {
                [0, 0, 0, 0]
            } else if y < 16 {
                [255, 0, 0, 255]
            } else if (16..32).contains(&y) && (16..40).contains(&x) {
                [0, 255, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        }))
        .unwrap()
    }

    fn colour_at(out: &[[u8; 4]], w: usize, x: usize, y: usize) -> [u8; 4] {
        out[y * w + x]
    }

    #[test]
    fn the_model_stands_head_up_and_centred() {
        let skin = marked();
        let pose = Pose {
            skin: &skin,
            slim: false,
            cape: None,
            yaw: 0.,
            pitch: 0.,
        };
        let (w, h) = (120, 190);
        let out = render(&pose, w, h);
        let head = colour_at(&out, w, w / 2, h / 2 - 60);
        let body = colour_at(&out, w, w / 2, h / 2 - 15);
        assert!(
            head[0] > 100 && head[1] == 0,
            "the head is up top: {head:?}"
        );
        assert!(body[1] > 100 && body[0] == 0, "the body under it: {body:?}");
        assert_eq!(colour_at(&out, w, 2, 2)[3], 0, "clear around the model");
    }

    #[test]
    fn a_turned_model_shows_its_back_and_the_cape_on_it() {
        let skin = marked();
        let cape = cape(&png(64, 32, |_, _| [250, 250, 0, 255])).unwrap();
        let (w, h) = (120, 190);
        let pose = |yaw: f32, cape: Option<&Texture>| {
            render(
                &Pose {
                    skin: &skin,
                    slim: false,
                    cape,
                    yaw,
                    pitch: 0.,
                },
                w,
                h,
            )
        };
        let front = pose(0., Some(&cape));
        let at_back = colour_at(&front, w, w / 2, h / 2 - 15);
        assert!(
            at_back[1] > 100 && at_back[2] == 0,
            "the body hides the cape from the front"
        );
        let back = pose(std::f32::consts::PI, Some(&cape));
        let caped = colour_at(&back, w, w / 2, h / 2 - 15);
        assert!(
            caped[0] > 100 && caped[1] > 100,
            "the cape covers the back: {caped:?}"
        );
        let bare = pose(std::f32::consts::PI, None);
        let body = colour_at(&bare, w, w / 2, h / 2 - 15);
        assert!(
            body[1] > 100 && body[0] == 0,
            "no cape, the body's back: {body:?}"
        );
    }

    #[test]
    fn legacy_skins_get_their_left_limbs_and_others_are_refused() {
        let legacy = skin(&png(64, 32, |x, y| [x as u8, y as u8, 9, 255])).unwrap();
        assert_eq!((legacy.w, legacy.h), (64, 64));
        // The left leg's front (20, 52) mirrors the right leg's front (4..8, 20..32).
        assert_eq!(legacy.texel(20, 52), [7, 20, 9, 255]);
        assert!(skin(&png(64, 48, |_, _| [0; 4])).is_none());
        assert!(skin(b"not a png").is_none());
        assert_eq!(skin(&png(128, 128, |_, _| [1, 2, 3, 255])).unwrap().w, 128);
    }

    #[test]
    fn capes_keep_their_layout_on_their_canvas() {
        for (w, h, k) in [
            (64, 32, 1),
            (46, 22, 1),
            (22, 17, 1),
            (92, 44, 2),
            (128, 64, 2),
        ] {
            let laid = cape(&png(w, h, |x, y| [x as u8, y as u8, 7, 255])).unwrap();
            assert_eq!((laid.w, laid.h), (64 * k, 32 * k), "{w}x{h}");
            let front = cape_front(&laid);
            assert_eq!((front.w, front.h), (10 * k, 16 * k));
            assert_eq!(front.texels[0], [k as u8, k as u8, 7, 255], "{w}x{h}");
        }
        assert!(cape(&png(8, 8, |_, _| [0; 4])).is_none());
    }

    #[test]
    fn slim_skins_are_told_by_their_clear_arm_column() {
        let classic = marked();
        assert!(!looks_slim(&classic));
        let slim = skin(&png(64, 64, |x, y| {
            if x == 54 && (20..32).contains(&y) {
                [0; 4]
            } else {
                [9, 9, 9, 255]
            }
        }))
        .unwrap();
        assert!(looks_slim(&slim));
    }
}
