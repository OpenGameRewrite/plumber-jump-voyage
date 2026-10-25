//! The console's single-precision library functions (`sinf`, `cosf`, `asinf`, `acosf`, `atanf`,
//! `atan2f`) as the 1.0.0 image links them. The library is a musl derivative: `sinf` and `cosf` go
//! through double-precision kernels, the inverse functions are single-precision polynomials. They
//! are not correctly rounded (about one `acosf` in ten and one `atan2f` in four differ from the
//! correctly rounded value in the last bit), and the last bit of a turn or a pose decides which
//! side of a comparison a body lands on, so the controller computes with these and not the host's.
//!
//! No fused multiply-add is involved (the console's code multiplies and adds separately, as Rust
//! does). `sqrtf` is the hardware instruction, which equals `f32::sqrt`.
//!
//! Checked bit for bit against the library running in a local interpreter (`PJV_LIBM_CASES`, see
//! `tests/console_libm.rs`). Arguments beyond `2^28 * pi / 2` for the circular functions need the
//! library's multi-word reduction, which no movement code reaches; they return NaN here.
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use std::f64::consts::FRAC_PI_2 as FRAC_PI_2_F64;

/// `pi / 2` with its low bits cleared (0x3fc90fda), not the nearest single (0x3fc90fdb).
const PIO2_HI: f32 = f32::from_bits(0x3fc9_0fda);
const PIO2_LO: f32 = 7.5497894e-08;
const P_S0: f32 = 0.16666587;
const P_S1: f32 = -0.042743422;
const P_S2: f32 = -0.008656363;
const Q_S1: f32 = -0.70662963;

/// `R(z)`, the rational approximation of the arcsine's remainder.
fn arcsine_remainder(z: f32) -> f32 {
    let p = z * (P_S0 + z * (P_S1 + z * P_S2));
    let q = 1.0 + z * Q_S1;
    p / q
}

pub fn acosf(x: f32) -> f32 {
    let bits = x.to_bits();
    let magnitude = bits & 0x7fff_ffff;
    if magnitude >= 0x3f80_0000 {
        if magnitude == 0x3f80_0000 {
            return if bits >> 31 != 0 { 2.0 * PIO2_HI } else { 0.0 };
        }
        return f32::NAN;
    }
    if magnitude < 0x3f00_0000 {
        if magnitude <= 0x3280_0000 {
            return PIO2_HI;
        }
        return PIO2_HI - (x - (PIO2_LO - x * arcsine_remainder(x * x)));
    }
    if bits >> 31 != 0 {
        let z = (1.0 + x) * 0.5;
        let s = z.sqrt();
        let w = arcsine_remainder(z) * s - PIO2_LO;
        return 2.0 * (PIO2_HI - (s + w));
    }
    let z = (1.0 - x) * 0.5;
    let s = z.sqrt();
    // The square root with its low twelve bits dropped splits the correction term.
    let df = f32::from_bits(s.to_bits() & 0xffff_f000);
    let c = (z - df * df) / (s + df);
    let w = arcsine_remainder(z) * s + c;
    2.0 * (df + w)
}

pub fn asinf(x: f32) -> f32 {
    let bits = x.to_bits();
    let magnitude = bits & 0x7fff_ffff;
    if magnitude >= 0x3f80_0000 {
        if magnitude == 0x3f80_0000 {
            // Evaluated in double precision, with the inexact-raising tiny term the library adds.
            return (f64::from(x) * FRAC_PI_2_F64 + 7.523_163_845_262_64e-37) as f32;
        }
        return f32::NAN;
    }
    if magnitude < 0x3f00_0000 {
        if (0x0080_0000..0x3980_0000).contains(&magnitude) {
            return x;
        }
        return x + x * arcsine_remainder(x * x);
    }
    let z = (1.0 - x.abs()) * 0.5;
    let s = f64::from(z).sqrt();
    let value = (FRAC_PI_2_F64 - 2.0 * (s + s * f64::from(arcsine_remainder(z)))) as f32;
    if bits >> 31 != 0 { -value } else { value }
}

// The anchors of the argument reduction: atan(0.5), atan(1), atan(1.5) and atan(inf) as single
// precision highs (the last two are pi/4 and pi/2 with their low bits cleared).
const ATAN_HI: [f32; 4] = [
    0.4636476,
    f32::from_bits(0x3f49_0fda),
    0.9827937,
    f32::from_bits(0x3fc9_0fda),
];
const ATAN_LO: [f32; 4] = [5.0121582e-09, 3.7748947e-08, 3.4473217e-08, 7.5497894e-08];
const ATAN_POLY: [f32; 5] = [
    0.33333328,
    -0.19999158,
    0.14253636,
    -0.106480174,
    0.061687607,
];

pub fn atanf(x: f32) -> f32 {
    let bits = x.to_bits();
    let negative = bits >> 31 != 0;
    let magnitude = bits & 0x7fff_ffff;
    let mut x = x;
    // Which of the four anchor angles the argument was reduced about, -1 for none.
    let id: i32;
    if magnitude >= 0x4c80_0000 {
        if x.is_nan() {
            return x;
        }
        let z = ATAN_HI[3];
        return if negative { -z } else { z };
    }
    if magnitude < 0x3ee0_0000 {
        if magnitude < 0x3980_0000 {
            return x;
        }
        id = -1;
    } else {
        x = x.abs();
        if magnitude < 0x3f98_0000 {
            if magnitude < 0x3f30_0000 {
                id = 0;
                x = (2.0 * x - 1.0) / (2.0 + x);
            } else {
                id = 1;
                x = (x - 1.0) / (x + 1.0);
            }
        } else if magnitude < 0x401c_0000 {
            id = 2;
            x = (x - 1.5) / (1.0 + 1.5 * x);
        } else {
            id = 3;
            x = -1.0 / x;
        }
    }
    let z = x * x;
    let w = z * z;
    // The sum of the odd and even terms, split as the library does.
    let odd = z * (ATAN_POLY[0] + w * (ATAN_POLY[2] + w * ATAN_POLY[4]));
    let even = w * (ATAN_POLY[1] + w * ATAN_POLY[3]);
    if id < 0 {
        return x - x * (odd + even);
    }
    let id = id as usize;
    let z = ATAN_HI[id] - ((x * (odd + even) - ATAN_LO[id]) - x);
    if negative { -z } else { z }
}

const PI_LO: f32 = -8.742278e-08;

pub fn atan2f(y: f32, x: f32) -> f32 {
    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    let ix = x.to_bits();
    let iy = y.to_bits();
    if ix == 0x3f80_0000 {
        return atanf(y);
    }
    // 2 * sign(x) + sign(y)
    let quadrant = ((iy >> 31) & 1) | ((ix >> 30) & 2);
    let ix = ix & 0x7fff_ffff;
    let iy = iy & 0x7fff_ffff;
    if iy == 0 {
        return match quadrant {
            0 | 1 => y,
            2 => PI,
            _ => -PI,
        };
    }
    if ix == 0 {
        return if quadrant & 1 != 0 {
            -FRAC_PI_2
        } else {
            FRAC_PI_2
        };
    }
    if ix == 0x7f80_0000 {
        if iy == 0x7f80_0000 {
            return match quadrant {
                0 => FRAC_PI_4,
                1 => -FRAC_PI_4,
                2 => 3.0 * FRAC_PI_4,
                _ => -3.0 * FRAC_PI_4,
            };
        }
        return match quadrant {
            0 => 0.0,
            1 => -0.0,
            2 => PI,
            _ => -PI,
        };
    }
    if ix.wrapping_add(26 << 23) < iy || iy == 0x7f80_0000 {
        return if quadrant & 1 != 0 {
            -FRAC_PI_2
        } else {
            FRAC_PI_2
        };
    }
    let z = if quadrant & 2 != 0 && iy.wrapping_add(26 << 23) < ix {
        0.0
    } else {
        atanf((y / x).abs())
    };
    match quadrant {
        0 => z,
        1 => -z,
        2 => PI - (z - PI_LO),
        _ => (z - PI_LO) - PI,
    }
}

// The kernels of `sinf` and `cosf`: polynomials of the reduced argument in double precision.
const S1: f64 = -0.166_666_666_416_265_24;
const S2: f64 = 0.008_333_329_385_889_463;
const S3: f64 = -0.000_198_393_348_360_966_32;
const S4: f64 = 2.718_311_493_989_822e-6;
const C0: f64 = -0.499_999_997_251_031;
const C1: f64 = 0.041_666_623_323_739_06;
const C2: f64 = -0.001_388_676_377_460_993;
const C3: f64 = 2.439_044_879_627_741e-5;

fn sine_kernel(x: f64) -> f32 {
    let z = x * x;
    let w = z * z;
    let r = S3 + z * S4;
    let s = z * x;
    ((x + s * (S1 + z * S2)) + s * w * r) as f32
}

fn cosine_kernel(x: f64) -> f32 {
    let z = x * x;
    let w = z * z;
    let r = C2 + z * C3;
    (((1.0 + z * C0) + w * C1) + (w * z) * r) as f32
}

const PIO2_1: f64 = 1.570_796_310_901_641_8;
const PIO2_1T: f64 = 1.589_325_477_352_819_6e-8;
const INV_PIO2: f64 = std::f64::consts::FRAC_2_PI;
const TO_INT: f64 = 6_755_399_441_055_744.0;

/// `__rem_pio2f` for the medium sizes: the quadrant and the remainder of `x` by `pi / 2`.
fn reduce_by_half_pi(x: f32) -> (i32, f64) {
    let x = f64::from(x);
    let rounded = x * INV_PIO2 + TO_INT - TO_INT;
    let quadrant = rounded as i32;
    let remainder = x + rounded * -PIO2_1 + rounded * -PIO2_1T;
    (quadrant, remainder)
}

/// `k * pi / 2` in double precision, the shifts of the near-multiple branches.
fn half_pis(k: f64) -> f64 {
    k * FRAC_PI_2_F64
}

pub fn sinf(x: f32) -> f32 {
    let bits = x.to_bits();
    let negative = bits >> 31 != 0;
    let magnitude = bits & 0x7fff_ffff;
    let x64 = f64::from(x);
    if magnitude <= 0x3f49_0fda {
        if magnitude < 0x3980_0000 {
            return x;
        }
        return sine_kernel(x64);
    }
    if magnitude <= 0x407b_53d1 {
        if magnitude <= 0x4016_cbe3 {
            return if negative {
                -cosine_kernel(x64 + half_pis(1.0))
            } else {
                cosine_kernel(x64 - half_pis(1.0))
            };
        }
        return sine_kernel(if negative {
            -(x64 + half_pis(2.0))
        } else {
            -(x64 - half_pis(2.0))
        });
    }
    if magnitude <= 0x40e2_31d5 {
        if magnitude <= 0x40af_eddf {
            return if negative {
                cosine_kernel(x64 + half_pis(3.0))
            } else {
                -cosine_kernel(x64 - half_pis(3.0))
            };
        }
        return sine_kernel(if negative {
            x64 + half_pis(4.0)
        } else {
            x64 - half_pis(4.0)
        });
    }
    if magnitude >= 0x4dc9_0fdb {
        return f32::NAN;
    }
    let (quadrant, y) = reduce_by_half_pi(x);
    match quadrant & 3 {
        0 => sine_kernel(y),
        1 => cosine_kernel(y),
        2 => sine_kernel(-y),
        _ => -cosine_kernel(y),
    }
}

pub fn cosf(x: f32) -> f32 {
    let bits = x.to_bits();
    let negative = bits >> 31 != 0;
    let magnitude = bits & 0x7fff_ffff;
    let x64 = f64::from(x);
    if magnitude <= 0x3f49_0fda {
        if magnitude < 0x3980_0000 {
            return 1.0;
        }
        return cosine_kernel(x64);
    }
    if magnitude <= 0x407b_53d1 {
        if magnitude > 0x4016_cbe3 {
            return -cosine_kernel(if negative {
                x64 + half_pis(2.0)
            } else {
                x64 - half_pis(2.0)
            });
        }
        return if negative {
            sine_kernel(x64 + half_pis(1.0))
        } else {
            sine_kernel(half_pis(1.0) - x64)
        };
    }
    if magnitude <= 0x40e2_31d5 {
        if magnitude > 0x40af_eddf {
            return cosine_kernel(if negative {
                x64 + half_pis(4.0)
            } else {
                x64 - half_pis(4.0)
            });
        }
        return if negative {
            sine_kernel(-half_pis(3.0) - x64)
        } else {
            sine_kernel(x64 - half_pis(3.0))
        };
    }
    if magnitude >= 0x4dc9_0fdb {
        return f32::NAN;
    }
    let (quadrant, y) = reduce_by_half_pi(x);
    match quadrant & 3 {
        0 => cosine_kernel(y),
        1 => sine_kernel(-y),
        2 => -cosine_kernel(y),
        _ => sine_kernel(y),
    }
}

#[path = "../tests/console_libm.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
