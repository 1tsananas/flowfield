use clap::*;
use image::*;
use imageproc::{drawing::draw_antialiased_line_segment_mut, pixelops::interpolate};
use noise::*;
use rand::*;
use std::f64::consts::PI;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = 2048)]
    width: u32,

    #[arg(long, default_value_t = 2048)]
    height: u32,

    #[arg(long, default_value_t = 0.005)]
    step: f64,

    #[arg(long, default_value_t = 1.0)]
    noise: f64,

    #[arg(long, default_value_t = 5000)]
    particals: u32,

    #[arg(short, long, default_value = "flowfield.png")]
    output: String,

    #[arg(short, long, default_value_t = 1)]
    seed: u32,
    //seed: Option<u32>,
}

struct Particle {
    x: f64,
    prev_x: f64,
    y: f64,
    prev_y: f64,
}

impl Particle {
    fn new() -> Particle {
        let x_rand = random::<f64>();
        let y_rand = random::<f64>();

        Particle {
            x: x_rand,
            prev_x: x_rand,
            y: y_rand,
            prev_y: y_rand,
        }
    }

    fn step(
        &mut self,
        perlin: &Perlin,
        noise: f64,
        img: &mut RgbaImage,
        width: u32,
        height: u32,
        weight: f64,
    ) -> bool {
        let ang = ang(perlin.get([self.x * noise, self.y * noise]));
        let x = self.x + f64::cos(ang) * weight;
        let y = self.y + f64::sin(ang) * weight;

        self.prev_x = self.x;
        self.prev_y = self.y;
        self.x = x;
        self.y = y;

        if let Some((cx1, cy1, cx2, cy2)) = clipper(self.prev_x, self.prev_y, self.x, self.y) {
            draw_antialiased_line_segment_mut(
                img,
                ((cx1 * width as f64) as i32, (cy1 * height as f64) as i32),
                ((cx2 * width as f64) as i32, (cy2 * height as f64) as i32),
                Rgba([255, 255, 255, 255]),
                interpolate,
            );
            return true;
        } else {
            draw_antialiased_line_segment_mut(
                img,
                (
                    (self.prev_x * width as f64) as i32,
                    (self.prev_y * height as f64) as i32,
                ),
                (
                    (self.x * width as f64) as i32,
                    (self.y * height as f64) as i32,
                ),
                Rgba([255, 255, 255, 255]),
                interpolate,
            );
            return false;
        }
    }
}

fn main() {
    let mut args = Args::parse();

    //if args.seed == None {
    //args.seed = Some(random::<u32>());
    //}

    let mut img = RgbaImage::new(args.width, args.height);
    //let perlin = Perlin::new(args.seed.unwrap());
    let perlin = Perlin::new(args.seed);

    for _ in 0..args.particals {
        let mut particle = Particle::new();
        for _ in 0..500 {
            if !particle.step(
                &perlin,
                args.noise,
                &mut img,
                args.width,
                args.height,
                args.step,
            ) {
                break;
            }
        }
    }
    let _ = img.save(&args.output);
}

fn ang(val: f64) -> f64 {
    return (val + 1.0) * PI;
}

fn clipper(prev_x: f64, prev_y: f64, x: f64, y: f64) -> Option<(f64, f64, f64, f64)> {
    let dx = x - prev_x;
    let dy = y - prev_y;

    let p1 = -dx;
    let p2 = dx;
    let p3 = -dy;
    let p4 = dy;

    let q1 = prev_x;
    let q2 = 1.0 - prev_x;
    let q3 = prev_y;
    let q4 = 1.0 - prev_y;

    if p1 == 0.0 && q1 < 0.0
        || p2 == 0.0 && q2 < 0.0
        || p3 == 0.0 && q3 < 0.0
        || p4 == 0.0 && q4 < 0.0
    {
        return None;
    }

    let mut u1 = 0.0;
    let mut u2 = 1.0;

    if p1 < 0.0 {
        u1 = f64::max(q1 / p1, 0.0);
    } else if p1 > 0.0 {
        u2 = f64::min(q1 / p1, 1.0);
    }

    if p1 < 0.0 {
        u1 = f64::max(q2 / p2, 0.0);
    } else if p2 > 0.0 {
        u2 = f64::min(q2 / p2, 1.0);
    }

    if p3 < 0.0 {
        u1 = f64::max(q3 / p3, 0.0);
    } else if p3 > 0.0 {
        u2 = f64::min(q3 / p3, 1.0);
    }

    if p4 < 0.0 {
        u1 = f64::max(q4 / p4, 0.0);
    } else if p4 > 0.0 {
        u2 = f64::min(q4 / p4, 1.0);
    }

    if u1 > u2 {
        return None;
    } else {
        return Some((
            prev_x + u1 * dx,
            prev_y + u1 * dy,
            prev_x + u2 * dx,
            prev_y + u2 * dy,
        ));
    }
}
