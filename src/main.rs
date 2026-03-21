use clap::*;
use image::*;
use imageproc::{drawing::draw_antialiased_line_segment_mut, pixelops::interpolate};
use noise::*;
use rand::*;
use std::{error::Error, f64::consts::PI, fs::create_dir_all, io::stdin, path::*};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = 2080, value_parser = value_parser!(u32).range(1..))]
    width: u32,

    #[arg(long, default_value_t = 2080, value_parser = value_parser!(u32).range(1..))]
    height: u32,

    #[arg(long, default_value_t = 0.01)]
    step_size: f64,

    #[arg(long, default_value_t = 100, value_parser = value_parser!(u32).range(1..))]
    steps: u32,

    #[arg(long, default_value_t = 1.0)]
    noise: f64,

    #[arg(long, default_value_t = 1.0, value_parser = positive)]
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

    #[arg(long, default_value_t = 5000, value_parser = value_parser!(u32).range(1..))]
    particles: u32,

    #[arg(long, default_value_t = false)]
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

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let path = Path::new(&args.output);
    let path = if path.extension().is_none() {
        path.join("flowfield.png")
    } else {
        path.to_path_buf()
    };
    if let Some(parent) = path.parent() {
        if !parent.exists() && parent != Path::new("") {
            println!("Directory {:?} does not exist. Create it? (y/n)", path);
            loop {
                let mut input = String::new();
                stdin().read_line(&mut input)?;
                match input.trim().to_lowercase().as_str() {
                    "y" => {
                        create_dir_all(parent)?;
                        break;
                    }
                    "n" => return Err("Directory does not exist".into()),
                    _ => println!("Please enter (y/n)"),
                }
            }
        }
    }

    let seed = args.seed.unwrap_or_else(|| random::<u32>());

    let bg_color = string_to_rgba(&args.bg_color)?;
    let fg_color = string_to_rgba(&args.fg_color)?;

    let render_width = (args.width as f64 * 1.25) as u32;
    let render_height = (args.height as f64 * 1.25) as u32;

    let aspect = render_width as f64 / render_height as f64;

    let mut img = RgbaImage::new(render_width, render_height);
    img.pixels_mut().for_each(|p| *p = bg_color);
    let perlin = Perlin::new(seed);

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

    let free_path = find_free_path(&path);

    img.save(&free_path)?;

    println!("seed: {}, saved as {}", seed, free_path.to_string_lossy());

    Ok(())
}

fn ang(val: f64) -> f64 {
    (val + 1.0) * PI
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
        None
    } else {
        Some((
            prev_x + u1 * dx,
            prev_y + u1 * dy,
            prev_x + u2 * dx,
            prev_y + u2 * dy,
        ))
    }
}

fn string_to_rgba(color: &str) -> Result<Rgba<u8>, String> {
    let numbers: Vec<u8> = color
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<u8>()
                .map_err(|_| "Value must be in Range of 0 - 255".to_string())
        })
        .collect::<Result<Vec<u8>, String>>()?;
    if numbers.len() == 4 {
        Ok(Rgba([numbers[0], numbers[1], numbers[2], numbers[3]]))
    } else {
        Err("Color needs exactly 4 Values: R, G, B, A".to_string())
    }
}

fn positive(number: &str) -> Result<f32, String> {
    let v: f32 = number.parse().map_err(|_| "Must be a number".to_string())?;
    if v > 0.0 {
        Ok(v)
    } else {
        Err("Must be greater than 0".to_string())
    }
}

fn find_free_path(path: &Path) -> PathBuf {
    let mut i = 0;
    let mut possible_path = path.to_path_buf();
    loop {
        if !Path::new(&possible_path).exists() {
            return possible_path;
        } else {
            possible_path = path.with_file_name(format!(
                "{}_{}.{}",
                path.file_stem().unwrap_or_default().to_string_lossy(),
                i,
                path.extension().unwrap_or_default().to_string_lossy()
            ));
            i += 1;
        }
    }
}
