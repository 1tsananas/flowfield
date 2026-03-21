use clap::*;
use image::*;
use imageproc::{
    drawing::{draw_antialiased_line_segment_mut, draw_filled_circle_mut},
    pixelops::interpolate,
};
use noise::*;
use rand::*;
use std::f64::consts::PI;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = 2080)]
    width: u32,

    #[arg(long, default_value_t = 2080)]
    height: u32,

    #[arg(long, default_value_t = 0.01)]
    step_size: f64,

    #[arg(long, default_value_t = 100)]
    steps: u32,

    #[arg(long, default_value_t = 2.0)]
    noise: f64,

    #[arg(long, default_value_t = 1.0)]
    blur: f32,

    #[arg(
        long,
        default_value = "0, 0, 0, 255",
        help = "RRBA: \"0-255, 0-255, 0-255, 0-255\" (actual transparent backgrounds possibel)"
    )]
    bg_color: String,

    #[arg(
        long,
        default_value = "255, 255, 255, 255",
        help = "RRBA: \"0-255, 0-255, 0-255, 0-255\""
    )]
    fg_color: String,

    #[arg(long, default_value_t = 5000)]
    particles: u32,

    #[arg(long, default_value_t = true)]
    line: bool,

    #[arg(short, long, default_value = "flowfield.png")]
    output: String,

    #[arg(short, long)]
    seed: Option<u32>,
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
        aspect: f64,
        line: bool,
        fg_color: Rgba<u8>,
    ) -> bool {
        let ang = ang(perlin.get([self.x * noise * aspect, self.y * noise]));

        let x = self.x + f64::cos(ang) * weight / aspect;
        let y = self.y + f64::sin(ang) * weight;

        self.prev_x = self.x;
        self.prev_y = self.y;
        self.x = x;
        self.y = y;

        if let Some((cx1, cy1, cx2, cy2)) = clipper(self.prev_x, self.prev_y, self.x, self.y) {
            if line {
                draw_antialiased_line_segment_mut(
                    img,
                    ((cx1 * width as f64) as i32, (cy1 * height as f64) as i32),
                    ((cx2 * width as f64) as i32, (cy2 * height as f64) as i32),
                    fg_color,
                    interpolate,
                );
                draw_filled_circle_mut(
                    img,
                    ((cx1 * width as f64) as i32, (cy1 * height as f64) as i32),
                    0,
                    fg_color,
                );
            } else {
                img.put_pixel(
                    (cx1 * width as f64) as u32,
                    (cy1 * height as f64) as u32,
                    fg_color,
                );
            }
            true
        } else {
            false
        }
    }
}

fn main() {
    let mut args = Args::parse();

    if args.seed.is_none() {
        args.seed = Some(random::<u32>());
    }

    let bg_color = string_to_rgba(&args.bg_color);
    let fg_color = string_to_rgba(&args.fg_color);

    let render_width = (args.width as f64 * 1.25) as u32;
    let render_height = (args.height as f64 * 1.25) as u32;

    let aspect = render_width as f64 / render_height as f64;

    let mut img = RgbaImage::new(render_width, render_height);
    img.pixels_mut().for_each(|p| *p = bg_color);
    let perlin = Perlin::new(args.seed.unwrap());

    for _ in 0..args.particles {
        let mut particle = Particle::new();
        for _ in 0..args.steps {
            if !particle.step(
                &perlin,
                args.noise,
                &mut img,
                render_width,
                render_height,
                args.step_size,
                aspect,
                args.line,
                fg_color,
            ) {
                break;
            }
        }
    }
    let img = image::imageops::crop_imm(
        &mut img,
        (render_width - args.width) / 2,
        (render_height - args.height) / 2,
        args.width,
        args.height,
    )
    .to_image();
    let img = image::imageops::blur(&img, args.blur);
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
        u1 = f64::max(u1, q1 / p1);
    } else if p1 > 0.0 {
        u2 = f64::min(u2, q1 / p1);
    }

    if p2 < 0.0 {
        u1 = f64::max(u1, q2 / p2);
    } else if p2 > 0.0 {
        u2 = f64::min(u2, q2 / p2);
    }

    if p3 < 0.0 {
        u1 = f64::max(u1, q3 / p3);
    } else if p3 > 0.0 {
        u2 = f64::min(u2, q3 / p3);
    }

    if p4 < 0.0 {
        u1 = f64::max(u1, q4 / p4);
    } else if p4 > 0.0 {
        u2 = f64::min(u2, q4 / p4);
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

fn string_to_rgba(color: &str) -> Rgba<u8> {
    let numbers: Vec<u8> = color
        .split(',')
        .map(|s| s.trim().parse::<u8>().unwrap_or(255))
        .collect();
    Rgba([numbers[0], numbers[1], numbers[2], numbers[3]])
}

//gradient paths
//gradient background (perlin noise background)
//fehlerbehandlung und ordentliches speicher und so mit path
